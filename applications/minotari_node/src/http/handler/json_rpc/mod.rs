// Copyright 2025. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{error::Error as StdError, sync::Arc, time::Duration};

use axum::{
    Extension,
    Json,
    body::Body,
    extract::{DefaultBodyLimit, FromRequestParts},
    http::{HeaderMap, StatusCode, header, request::Parts},
    routing::{MethodRouter, post},
};
use http_body_util::{BodyExt, LengthLimitError};
use log::{debug, warn};
use serde::{Deserialize, Serialize};
use tari_comms::{
    decode_budget::check_decode_budget,
    message::MessageExt,
    protocol::messaging::MAX_FRAME_LENGTH,
    utils::text::sanitize_for_display,
};
use tari_core::{
    base_node::rpc::query_service,
    chain_storage::BlockchainBackend,
    mempool::service::MempoolHandle,
    proto,
};
use tari_transaction_components::{rpc::models::TxSubmissionResponseV1, transaction_components::Transaction};
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore},
    task,
    time,
};

use crate::http::handler::ErrorResponse;

pub mod submit_transaction;

const LOG_TARGET: &str = "c::base_node::rpc::http::handler::json_rpc";

/// Maximum JSON-RPC HTTP request body size (10 MiB, matching the HTTP router's `RequestBodyLimitLayer`).
pub const MAX_JSON_RPC_BODY_BYTES: usize = 25 * 4 * 1024 * 1024 / 10;
/// Maximum aggregate HTTP request body bytes buffered across all concurrent `/json_rpc` uploads and decodes (80 MiB).
/// Permits are charged per non-empty body chunk as data arrives so stalled header-only connections hold zero permits
/// and cannot starve active wallets, and are released as soon as decoding completes before
/// `submit_transaction::handle`.
pub const MAX_IN_FLIGHT_JSON_RPC_BODY_BYTES: usize = 8 * MAX_JSON_RPC_BODY_BYTES;
/// Maximum number of JSON-RPC requests running synchronous JSON + protobuf transaction decoding at once,
/// matching `MempoolService`'s `MAX_CONCURRENT_HANDLE_SUBMISSIONS`.
pub const MAX_CONCURRENT_JSON_RPC_DECODES: usize = 2;
/// Maximum idle time allowed waiting for the next non-empty HTTP body chunk on `/json_rpc`.
/// Allows large transactions over slow links (e.g. Tor or mobile) to succeed as long as bytes continue arriving, while
/// aborting stalled uploads with `408 Request Timeout`.
pub const DEFAULT_JSON_RPC_BODY_CHUNK_IDLE_TIMEOUT: Duration = Duration::from_secs(15);

const MAX_JSONRPC_VERSION_BYTES: usize = 16;
const MAX_JSON_RPC_ID_BYTES: usize = 256;
const MAX_JSON_RPC_METHOD_BYTES: usize = 128;

/// Per-server buffered-byte, decode-concurrency, and per-chunk idle timeout bounds for `/json_rpc`.
#[derive(Clone, Debug)]
pub struct JsonRpcLimiter {
    in_flight_bytes: Arc<Semaphore>,
    decode: Arc<Semaphore>,
    chunk_idle_timeout: Duration,
}

impl Default for JsonRpcLimiter {
    fn default() -> Self {
        Self::with_limits(MAX_IN_FLIGHT_JSON_RPC_BODY_BYTES, MAX_CONCURRENT_JSON_RPC_DECODES)
    }
}

impl JsonRpcLimiter {
    pub fn with_limits(max_in_flight_bytes: usize, max_concurrent_decodes: usize) -> Self {
        Self::with_limits_and_idle_timeout(
            max_in_flight_bytes,
            max_concurrent_decodes,
            DEFAULT_JSON_RPC_BODY_CHUNK_IDLE_TIMEOUT,
        )
    }

    pub fn with_limits_and_idle_timeout(
        max_in_flight_bytes: usize,
        max_concurrent_decodes: usize,
        chunk_idle_timeout: Duration,
    ) -> Self {
        let max_semaphore_permits = Semaphore::MAX_PERMITS.min(usize::try_from(u32::MAX).unwrap_or(usize::MAX));
        let capped_bytes = max_in_flight_bytes.clamp(1, max_semaphore_permits);
        Self {
            in_flight_bytes: Arc::new(Semaphore::new(capped_bytes)),
            decode: Arc::new(Semaphore::new(max_concurrent_decodes.max(1))),
            chunk_idle_timeout,
        }
    }
}

/// Pre-body header validator and limiter extractor for `/json_rpc`.
///
/// Validates `Content-Length` inside [`FromRequestParts`] before reading the HTTP body without reserving permits up
/// front: byte permits are charged per non-empty chunk in [`read_bounded_body`], so connections that send no body bytes
/// hold nothing and cannot lock out well-behaved wallets.
#[derive(Debug)]
pub struct JsonRpcAdmission {
    limiter: JsonRpcLimiter,
}

impl JsonRpcAdmission {
    pub fn new(limiter: JsonRpcLimiter) -> Self {
        Self { limiter }
    }
}

impl<S: Send + Sync> FromRequestParts<S> for JsonRpcAdmission {
    type Rejection = (StatusCode, Json<ErrorResponse>);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        validate_content_length(&parts.headers)?;
        let limiter = parts.extensions.get::<JsonRpcLimiter>().cloned().ok_or_else(|| {
            warn!(target: LOG_TARGET, "Missing JsonRpcLimiter extension on /json_rpc route");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("JSON-RPC limiter unavailable".to_string())),
            )
        })?;
        Ok(Self::new(limiter))
    }
}

/// Builds the `/json_rpc` [`MethodRouter`] with Axum's default 2 MiB limit disabled so [`JsonRpcAdmission`] and
/// [`handle`] enforce pre-body `Content-Length` checks, per-chunk byte permits, the 10 MiB body cap, and idle timeouts.
pub fn route<B: BlockchainBackend + 'static>() -> MethodRouter {
    post(handle::<B>).layer(DefaultBodyLimit::disable())
}

fn payload_too_large_error() -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::PAYLOAD_TOO_LARGE,
        Json(ErrorResponse::new(format!(
            "JSON-RPC request body exceeds maximum allowed size ({MAX_JSON_RPC_BODY_BYTES} bytes)"
        ))),
    )
}

fn validate_content_length(headers: &HeaderMap) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    for raw_len in headers.get_all(header::CONTENT_LENGTH) {
        let len_str = raw_len
            .to_str()
            .map_err(|_| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse::new("Invalid Content-Length header".to_string())),
                )
            })?
            .trim();
        if len_str.is_empty() || !len_str.bytes().all(|b| b.is_ascii_digit()) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new("Invalid Content-Length header".to_string())),
            ));
        }
        match len_str.parse::<usize>() {
            Ok(len) if len <= MAX_JSON_RPC_BODY_BYTES => {},
            Ok(len) => {
                return Err((
                    StatusCode::PAYLOAD_TOO_LARGE,
                    Json(ErrorResponse::new(format!(
                        "JSON-RPC request body ({len} bytes) exceeds maximum allowed size ({MAX_JSON_RPC_BODY_BYTES} \
                         bytes)"
                    ))),
                ));
            },
            Err(_) => {
                return Err(payload_too_large_error());
            },
        }
    }
    Ok(())
}

fn is_length_limit_error(err: &(dyn StdError + 'static)) -> bool {
    let mut current: Option<&(dyn StdError + 'static)> = Some(err);
    while let Some(source) = current {
        if source.is::<LengthLimitError>() {
            return true;
        }
        current = source.source();
    }
    false
}

struct BufferedJsonRpcBody {
    bytes: Vec<u8>,
    byte_permit: Option<OwnedSemaphorePermit>,
}

async fn read_bounded_body(
    mut body: Body,
    limiter: &JsonRpcLimiter,
) -> Result<BufferedJsonRpcBody, (StatusCode, Json<ErrorResponse>)> {
    let mut buffered = Vec::new();
    let mut byte_permit: Option<OwnedSemaphorePermit> = None;
    let mut idle_deadline = time::Instant::now()
        .checked_add(limiter.chunk_idle_timeout)
        .unwrap_or_else(time::Instant::now);

    loop {
        let next_frame = time::timeout_at(idle_deadline, body.frame()).await.map_err(|_| {
            (
                StatusCode::REQUEST_TIMEOUT,
                Json(ErrorResponse::new(
                    "Timed out reading JSON-RPC request body".to_string(),
                )),
            )
        })?;

        let Some(frame_result) = next_frame else {
            break;
        };

        let frame = frame_result.map_err(|err| {
            if is_length_limit_error(&err) {
                payload_too_large_error()
            } else {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse::new(format!(
                        "Failed to read JSON-RPC request body: {err}"
                    ))),
                )
            }
        })?;

        let Ok(chunk) = frame.into_data() else {
            continue;
        };
        if chunk.is_empty() {
            continue;
        }

        let new_total = buffered
            .len()
            .checked_add(chunk.len())
            .ok_or_else(payload_too_large_error)?;
        if new_total > MAX_JSON_RPC_BODY_BYTES {
            return Err(payload_too_large_error());
        }

        let chunk_permits = u32::try_from(chunk.len()).map_err(|_| payload_too_large_error())?;
        let chunk_permit = limiter
            .in_flight_bytes
            .clone()
            .try_acquire_many_owned(chunk_permits)
            .map_err(|_| {
                (
                    StatusCode::TOO_MANY_REQUESTS,
                    Json(ErrorResponse::new(
                        "Too many concurrent JSON-RPC request bytes buffered; try again later".to_string(),
                    )),
                )
            })?;

        match &mut byte_permit {
            Some(existing) => existing.merge(chunk_permit),
            None => byte_permit = Some(chunk_permit),
        }

        if new_total > buffered.capacity() {
            let target_cap = buffered
                .capacity()
                .saturating_mul(2)
                .max(new_total)
                .min(MAX_JSON_RPC_BODY_BYTES);
            buffered.reserve_exact(target_cap.saturating_sub(buffered.len()));
        }
        buffered.extend_from_slice(&chunk);

        idle_deadline = time::Instant::now()
            .checked_add(limiter.chunk_idle_timeout)
            .unwrap_or_else(time::Instant::now);
    }

    Ok(BufferedJsonRpcBody {
        bytes: buffered,
        byte_permit,
    })
}

async fn read_and_decode_transaction(
    body: Body,
    limiter: &JsonRpcLimiter,
) -> Result<DecodedSubmitTransaction, (StatusCode, Json<ErrorResponse>)> {
    let BufferedJsonRpcBody {
        bytes: body,
        byte_permit,
    } = read_bounded_body(body, limiter).await?;

    let decode_permit = limiter.decode.clone().acquire_owned().await.map_err(|_| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse::new("JSON-RPC decode limiter closed".to_string())),
        )
    })?;

    // Move `body`, `byte_permit`, and `decode_permit` into the blocking task so that:
    // 1) If an HTTP client disconnects mid-decode, both permits remain held until the blocking thread finishes freeing
    //    `body`.
    // 2) Both `byte_permit` and `decode_permit` are dropped as soon as decoding finishes, strictly before calling
    //    `submit_transaction::handle` (which is bounded separately by the mempool's own submission permits).
    task::spawn_blocking(move || {
        let _decode_permit = decode_permit;
        let _byte_permit = byte_permit;
        let body = body;
        decode_transaction(&body)
    })
    .await
    .map_err(|e| {
        warn!(target: LOG_TARGET, "JSON-RPC decode task failed: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("Internal error decoding request".to_string())),
        )
    })?
}

pub async fn handle<B: BlockchainBackend + 'static>(
    admission: JsonRpcAdmission,
    Extension(query_service): Extension<Arc<query_service::Service<B>>>,
    Extension(mempool_service): Extension<MempoolHandle>,
    body: Body,
) -> Result<Json<JsonRpcResponse>, (StatusCode, Json<ErrorResponse>)> {
    let DecodedSubmitTransaction {
        id,
        version,
        transaction,
    } = read_and_decode_transaction(body, &admission.limiter).await?;
    drop(admission);

    let submission_result =
        submit_transaction::handle(query_service.clone(), &mut (mempool_service.clone()), transaction).await;

    match submission_result {
        Ok(response) => {
            let result = if version >= 2 {
                serde_json::to_value(response).unwrap_or_else(|e| {
                    warn!(target: LOG_TARGET, "Failed to serialize response: {e}");
                    serde_json::Value::Null
                })
            } else {
                serde_json::to_value(TxSubmissionResponseV1::from(response)).unwrap_or_else(|e| {
                    warn!(target: LOG_TARGET, "Failed to serialize V1 response: {e}");
                    serde_json::Value::Null
                })
            };
            Ok(Json(JsonRpcResponse {
                result,
                error: None,
                id,
            }))
        },
        Err(e) => {
            debug!(target: LOG_TARGET, "Error submitting transaction: {e}");

            Ok(Json(JsonRpcResponse {
                result: serde_json::Value::Null,
                error: Some(e.to_string()),
                id,
            }))
        },
    }
}

struct DecodedSubmitTransaction {
    id: String,
    version: u64,
    transaction: Transaction,
}

struct BoundedString<const MAX_BYTES: usize>(String);

impl<const MAX_BYTES: usize> BoundedString<MAX_BYTES> {
    fn into_inner(self) -> String {
        self.0
    }
}

impl<'de, const MAX_BYTES: usize> Deserialize<'de> for BoundedString<MAX_BYTES> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: serde::Deserializer<'de> {
        struct BoundedVisitor<const MAX_BYTES: usize>;

        impl<const MAX_BYTES: usize> serde::de::Visitor<'_> for BoundedVisitor<MAX_BYTES> {
            type Value = BoundedString<MAX_BYTES>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "a string of at most {MAX_BYTES} bytes")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where E: serde::de::Error {
                if v.len() > MAX_BYTES {
                    return Err(E::custom(format!(
                        "string length {} exceeds maximum of {MAX_BYTES} bytes",
                        v.len()
                    )));
                }
                Ok(BoundedString(v.to_owned()))
            }

            fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
            where E: serde::de::Error {
                if v.len() > MAX_BYTES {
                    return Err(E::custom(format!(
                        "string length {} exceeds maximum of {MAX_BYTES} bytes",
                        v.len()
                    )));
                }
                Ok(BoundedString(v))
            }
        }

        deserializer.deserialize_str(BoundedVisitor::<MAX_BYTES>)
    }
}

#[derive(Deserialize)]
struct JsonRpcEnvelope {
    #[allow(dead_code)]
    jsonrpc: BoundedString<MAX_JSONRPC_VERSION_BYTES>,
    id: BoundedString<MAX_JSON_RPC_ID_BYTES>,
    method: BoundedString<MAX_JSON_RPC_METHOD_BYTES>,
    #[serde(default)]
    #[allow(dead_code)]
    params: serde::de::IgnoredAny,
}

#[derive(Deserialize)]
struct SubmitTransactionEnvelope {
    #[serde(default)]
    params: SubmitTransactionParams,
}

const fn default_submit_tx_version() -> u64 {
    1
}

#[derive(Deserialize)]
struct SubmitTransactionParams {
    #[serde(default, deserialize_with = "deserialize_present_transaction")]
    transaction: Option<Transaction>,
    #[serde(
        default = "default_submit_tx_version",
        deserialize_with = "deserialize_lenient_version"
    )]
    version: u64,
}

impl Default for SubmitTransactionParams {
    fn default() -> Self {
        Self {
            transaction: None,
            version: default_submit_tx_version(),
        }
    }
}

fn deserialize_present_transaction<'de, D>(deserializer: D) -> Result<Option<Transaction>, D::Error>
where D: serde::Deserializer<'de> {
    Transaction::deserialize(deserializer).map(Some)
}

/// Matches legacy `params.get("version").and_then(Value::as_u64).unwrap_or(1)` semantics without allocating an untyped
/// `serde_json::Value` tree: non-negative `u64` integers are preserved, while strings (`"2"`), floats (`2.0`),
/// negative integers (`-1`), booleans, `null`, arrays, and objects fall back to version `1`.
fn deserialize_lenient_version<'de, D>(deserializer: D) -> Result<u64, D::Error>
where D: serde::Deserializer<'de> {
    struct LenientVersionVisitor;

    impl<'de> serde::de::Visitor<'de> for LenientVersionVisitor {
        type Value = u64;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("any JSON value representing an optional RPC version")
        }

        fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E> {
            Ok(v)
        }

        fn visit_u128<E>(self, v: u128) -> Result<Self::Value, E> {
            Ok(u64::try_from(v).unwrap_or(default_submit_tx_version()))
        }

        fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E> {
            Ok(u64::try_from(v).unwrap_or(default_submit_tx_version()))
        }

        fn visit_i128<E>(self, v: i128) -> Result<Self::Value, E> {
            Ok(u64::try_from(v).unwrap_or(default_submit_tx_version()))
        }

        fn visit_f64<E>(self, _v: f64) -> Result<Self::Value, E> {
            Ok(default_submit_tx_version())
        }

        fn visit_bool<E>(self, _v: bool) -> Result<Self::Value, E> {
            Ok(default_submit_tx_version())
        }

        fn visit_str<E>(self, _v: &str) -> Result<Self::Value, E> {
            Ok(default_submit_tx_version())
        }

        fn visit_string<E>(self, _v: String) -> Result<Self::Value, E> {
            Ok(default_submit_tx_version())
        }

        fn visit_none<E>(self) -> Result<Self::Value, E> {
            Ok(default_submit_tx_version())
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E> {
            Ok(default_submit_tx_version())
        }

        fn visit_some<D2>(self, deserializer: D2) -> Result<Self::Value, D2::Error>
        where D2: serde::Deserializer<'de> {
            deserializer.deserialize_any(self)
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where A: serde::de::SeqAccess<'de> {
            while seq.next_element::<serde::de::IgnoredAny>()?.is_some() {}
            Ok(default_submit_tx_version())
        }

        fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
        where A: serde::de::MapAccess<'de> {
            while map
                .next_entry::<serde::de::IgnoredAny, serde::de::IgnoredAny>()?
                .is_some()
            {}
            Ok(default_submit_tx_version())
        }
    }

    deserializer.deserialize_any(LenientVersionVisitor)
}

fn verify_p2p_proto_limits(
    proto: &proto::types::Transaction,
    max_decode_items: usize,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    let bad_request = |e: String| (StatusCode::BAD_REQUEST, Json(ErrorResponse::new(e)));
    let encoded = proto.to_encoded_bytes();
    if encoded.len() > MAX_FRAME_LENGTH {
        return Err(bad_request(format!(
            "Encoded transaction size ({} bytes) exceeds maximum P2P frame size ({MAX_FRAME_LENGTH} bytes)",
            encoded.len()
        )));
    }
    check_decode_budget::<proto::types::Transaction>(&encoded, max_decode_items).map_err(|e| {
        bad_request(format!(
            "Transaction exceeds P2P decode budget ({} embedded items, at most {} allowed)",
            e.items, e.max
        ))
    })?;
    Ok(())
}

/// Decodes a JSON-RPC `submit_transaction` request body without materializing an untyped `serde_json::Value` tree.
///
/// First validates the JSON-RPC envelope (`jsonrpc`, `id`, `method`) while skipping `params` via `IgnoredAny` so
/// unknown methods and oversized strings are rejected with zero `params` heap allocations. Then stream-deserializes
/// `SubmitTransactionParams` directly from `body` and round-trips the `Transaction` through the canonical P2P protobuf
/// conversion (`Transaction` <-> `tari_core::proto::types::Transaction`) and P2P frame/decode-budget checks, ensuring
/// this entry point cannot hand the mempool a transaction that a peer could not have sent.
fn decode_transaction(body: &[u8]) -> Result<DecodedSubmitTransaction, (StatusCode, Json<ErrorResponse>)> {
    let bad_request = |e: String| (StatusCode::BAD_REQUEST, Json(ErrorResponse::new(e)));
    let envelope = serde_json::from_slice::<JsonRpcEnvelope>(body).map_err(|e| bad_request(e.to_string()))?;
    let method = envelope.method.into_inner();
    let id = envelope.id.into_inner();

    debug!(
        target: LOG_TARGET,
        "Received JSON-RPC request: method='{}', id='{}'",
        sanitize_for_display(&method, MAX_JSON_RPC_METHOD_BYTES),
        sanitize_for_display(&id, MAX_JSON_RPC_ID_BYTES),
    );

    if method != "submit_transaction" {
        return Err(bad_request("Method not found".to_string()));
    }

    let submit = serde_json::from_slice::<SubmitTransactionEnvelope>(body).map_err(|e| bad_request(e.to_string()))?;
    // Version defaults to 1 (and falls back to 1 for non-u64 values) for backward compatibility with existing clients.
    // V2 adds the optional `details` field in the response.
    let version = submit.params.version;
    let transaction = submit
        .params
        .transaction
        .ok_or_else(|| bad_request("Missing transaction parameter".to_string()))?;

    let proto = proto::types::Transaction::try_from(transaction).map_err(bad_request)?;
    verify_p2p_proto_limits(&proto, proto::BODY_MAX_DECODE_ITEMS)?;
    let transaction = Transaction::try_from(proto).map_err(|e| bad_request(e.to_string()))?;

    Ok(DecodedSubmitTransaction {
        id,
        version,
        transaction,
    })
}

#[derive(Serialize, Debug)]

pub struct JsonRpcResponse {
    pub result: serde_json::Value,
    pub error: Option<String>,
    pub id: String,
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)]
    use std::net::SocketAddr;

    use axum::body::Bytes;
    use tari_common::configuration::Network;
    use tari_common_types::types::{ComAndPubSignature, FixedHash, PrivateKey};
    use tari_core::{
        base_node::{StateMachineHandle, state_machine_service::states::StatusInfo},
        block_specs,
        chain_storage::async_db::AsyncBlockchainDb,
        mempool::{TxStorageResponse, test_utils::mock::create_mempool_service_mock},
        test_helpers::blockchain::{TempDatabase, create_main_chain, create_new_blockchain_with_network},
    };
    use tari_script::ExecutionStack;
    use tari_shutdown::{Shutdown, ShutdownSignal};
    use tari_transaction_components::transaction_components::{
        TransactionInput,
        TransactionInputVersion,
        TransactionKernel,
        TransactionOutput,
        encrypted_data::STATIC_ENCRYPTED_DATA_SIZE_TOTAL,
    };
    use tari_utilities::hex::to_hex;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
        sync::{broadcast, watch},
    };
    use tower_http::limit::RequestBodyLimitLayer;

    use super::*;

    fn make_query_service(shutdown: &Shutdown, mempool: MempoolHandle) -> Arc<query_service::Service<TempDatabase>> {
        let db = AsyncBlockchainDb::from(create_new_blockchain_with_network(Network::LocalNet));
        let (state_tx, _state_rx) = broadcast::channel(10);
        let (_status_tx, status_rx) = watch::channel(StatusInfo::new());
        let state_machine = StateMachineHandle::new(state_tx, status_rx, shutdown.to_signal());
        Arc::new(query_service::Service::new(db, state_machine, mempool))
    }

    async fn call_handle(
        query_service: Arc<query_service::Service<TempDatabase>>,
        mempool: MempoolHandle,
        limiter: JsonRpcLimiter,
        body: Bytes,
    ) -> Result<Json<JsonRpcResponse>, (StatusCode, Json<ErrorResponse>)> {
        let admission = JsonRpcAdmission::new(limiter);
        handle(
            admission,
            Extension(query_service),
            Extension(mempool),
            Body::from(body),
        )
        .await
    }

    async fn spawn_test_server(
        query_service: Arc<query_service::Service<TempDatabase>>,
        mempool: MempoolHandle,
        limiter: JsonRpcLimiter,
        shutdown: ShutdownSignal,
    ) -> SocketAddr {
        let router = axum::Router::new()
            .route("/json_rpc", route::<TempDatabase>())
            .layer(RequestBodyLimitLayer::new(MAX_JSON_RPC_BODY_BYTES))
            .layer(Extension(query_service))
            .layer(Extension(mempool))
            .layer(Extension(limiter));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(shutdown)
                .await
                .unwrap();
        });
        addr
    }

    async fn read_http_response(stream: &mut TcpStream) -> (u16, String) {
        let mut buf = Vec::new();
        let header_end = loop {
            let mut chunk = [0u8; 1024];
            let n = time::timeout(Duration::from_secs(5), stream.read(&mut chunk))
                .await
                .expect("timed out waiting for HTTP response headers")
                .expect("failed reading HTTP response headers");
            assert!(n > 0, "connection closed before HTTP response headers completed");
            buf.extend_from_slice(&chunk[..n]);
            if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                break pos.saturating_add(4);
            }
        };

        let header_str = std::str::from_utf8(&buf[..header_end]).expect("valid UTF-8 HTTP headers");
        let status_line = header_str.lines().next().expect("HTTP status line");
        let status_code = status_line
            .split_whitespace()
            .nth(1)
            .expect("HTTP status code token")
            .parse::<u16>()
            .expect("numeric HTTP status code");

        let content_length = header_str
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().expect("numeric content-length"))
            })
            .unwrap_or(0);

        let total_expected = header_end.saturating_add(content_length);
        while buf.len() < total_expected {
            let mut chunk = [0u8; 1024];
            let n = time::timeout(Duration::from_secs(5), stream.read(&mut chunk))
                .await
                .expect("timed out waiting for HTTP response body")
                .expect("failed reading HTTP response body");
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
        }

        let body_end = total_expected.min(buf.len());
        let body = String::from_utf8_lossy(&buf[header_end..body_end]).into_owned();
        (status_code, body)
    }

    fn transaction_json(encrypted_data_hex: Option<String>) -> serde_json::Value {
        let tx = Transaction::new(
            vec![],
            vec![TransactionOutput::default()],
            vec![TransactionKernel::default()],
            PrivateKey::default(),
            PrivateKey::default(),
        );
        let mut json = serde_json::to_value(&tx).unwrap();
        if let Some(hex) = encrypted_data_hex {
            json["body"]["outputs"][0]["encrypted_data"]["data"] = serde_json::Value::String(hex);
        }
        json
    }

    fn request(transaction: serde_json::Value) -> Bytes {
        Bytes::from(
            serde_json::to_vec(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": "1",
                "method": "submit_transaction",
                "params": { "transaction": transaction },
            }))
            .unwrap(),
        )
    }

    #[tokio::test]
    async fn submit_transaction_rejects_short_encrypted_data_without_touching_the_mempool() {
        let shutdown = Shutdown::new();
        let (mempool, mempool_state) = create_mempool_service_mock();
        let query_service = make_query_service(&shutdown, mempool.clone());

        let short = to_hex(&[1u8; STATIC_ENCRYPTED_DATA_SIZE_TOTAL - 1]);
        let tx = transaction_json(Some(short));
        // The JSON really does contain the short value
        assert_eq!(
            tx["body"]["outputs"][0]["encrypted_data"]["data"]
                .as_str()
                .unwrap()
                .len(),
            2 * (STATIC_ENCRYPTED_DATA_SIZE_TOTAL - 1)
        );

        let (status, Json(err)) = call_handle(query_service, mempool, JsonRpcLimiter::default(), request(tx))
            .await
            .unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let err = serde_json::to_string(&err).unwrap();
        assert!(err.contains("at least"), "{}", err);
        assert_eq!(mempool_state.get_call_count(), 0, "the mempool must not be called");
    }

    #[tokio::test]
    async fn submit_transaction_forwards_a_well_formed_transaction_to_the_mempool() {
        let shutdown = Shutdown::new();
        let (mempool, mempool_state) = create_mempool_service_mock();
        mempool_state
            .set_submit_transaction_response(TxStorageResponse::UnconfirmedPool)
            .await;
        let query_service = make_query_service(&shutdown, mempool.clone());

        let response = match call_handle(
            query_service,
            mempool,
            JsonRpcLimiter::default(),
            request(transaction_json(None)),
        )
        .await
        {
            Ok(Json(response)) => response,
            Err((status, Json(err))) => panic!("{status}: {}", serde_json::to_string(&err).unwrap()),
        };
        assert!(response.error.is_none(), "{:?}", response.error);
        assert_eq!(mempool_state.get_call_count(), 1);
    }

    /// A mempool policy refusal is reported with its own retryable reason, not as a validation failure
    #[tokio::test]
    async fn submit_transaction_reports_a_mempool_policy_refusal() {
        let shutdown = Shutdown::new();
        let (mempool, mempool_state) = create_mempool_service_mock();
        mempool_state
            .set_submit_transaction_response(TxStorageResponse::NotStoredMempoolPolicy("full".to_string()))
            .await;
        let query_service = make_query_service(&shutdown, mempool.clone());

        let response = match call_handle(
            query_service,
            mempool,
            JsonRpcLimiter::default(),
            request(transaction_json(None)),
        )
        .await
        {
            Ok(Json(response)) => response,
            Err((status, Json(err))) => panic!("{status}: {}", serde_json::to_string(&err).unwrap()),
        };
        let response: TxSubmissionResponseV1 = serde_json::from_value(response.result).unwrap();
        assert!(!response.accepted);
        assert_eq!(
            response.rejection_reason,
            tari_transaction_components::rpc::models::TxSubmissionRejectionReason::MempoolPolicy
        );
    }

    /// Serde carries the version of a compact input but neither protobuf family does, so the P2P round-trip turns a
    /// V1 compact input into V0. The mempool must only ever see the round-tripped transaction, i.e. one a peer could
    /// have sent; this fails if the handler hands it a transaction decoded by serde alone.
    #[tokio::test]
    async fn submit_transaction_hands_the_mempool_the_p2p_round_tripped_transaction() {
        let shutdown = Shutdown::new();
        let (mempool, mempool_state) = create_mempool_service_mock();
        mempool_state
            .set_submit_transaction_response(TxStorageResponse::UnconfirmedPool)
            .await;
        let query_service = make_query_service(&shutdown, mempool.clone());

        let mut input = TransactionInput::new_with_output_hash(
            FixedHash::zero(),
            ExecutionStack::default(),
            ComAndPubSignature::default(),
        );
        input.version = TransactionInputVersion::V1;
        let tx = Transaction::new(
            vec![input],
            vec![TransactionOutput::default()],
            vec![],
            PrivateKey::default(),
            PrivateKey::default(),
        );
        let json = serde_json::to_value(&tx).unwrap();
        // Serde alone keeps V1
        let serde_only = serde_json::from_value::<Transaction>(json.clone()).unwrap();
        assert_eq!(serde_only.body.inputs()[0].version, TransactionInputVersion::V1);

        let response = match call_handle(query_service, mempool, JsonRpcLimiter::default(), request(json)).await {
            Ok(Json(response)) => response,
            Err((status, Json(err))) => panic!("{status}: {}", serde_json::to_string(&err).unwrap()),
        };
        assert!(response.error.is_none(), "{:?}", response.error);
        let submitted = mempool_state.submitted_transactions().await;
        assert_eq!(submitted.len(), 1);
        assert_eq!(submitted[0].body.inputs()[0].version, TransactionInputVersion::V0);
    }

    #[tokio::test]
    async fn submit_transaction_distinguishes_double_spend_from_already_mined() {
        let shutdown = Shutdown::new();
        let (mempool, mempool_state) = create_mempool_service_mock();
        mempool_state
            .set_submit_transaction_response(TxStorageResponse::NotStoredAlreadySpent)
            .await;

        let raw_db = create_new_blockchain_with_network(Network::LocalNet);
        let (_names, chain) = create_main_chain(&raw_db, block_specs!(["A->GB"]));
        let mined_kernel = chain["A"]
            .block()
            .body
            .kernels()
            .first()
            .expect("mined block contains a coinbase kernel")
            .clone();
        let db = AsyncBlockchainDb::from(raw_db);
        let (state_tx, _state_rx) = broadcast::channel(10);
        let (_status_tx, status_rx) = watch::channel(StatusInfo::new());
        let state_machine = StateMachineHandle::new(state_tx, status_rx, shutdown.to_signal());
        let query_service = Arc::new(query_service::Service::new(db, state_machine, mempool.clone()));

        // 1. Kernel not in chain DB -> DoubleSpend
        let response = match call_handle(
            query_service.clone(),
            mempool.clone(),
            JsonRpcLimiter::default(),
            request(transaction_json(None)),
        )
        .await
        {
            Ok(Json(response)) => response,
            Err((status, Json(err))) => panic!("{status}: {}", serde_json::to_string(&err).unwrap()),
        };
        assert!(response.error.is_none(), "{:?}", response.error);
        assert_eq!(response.result["accepted"], false);
        assert_eq!(response.result["rejection_reason"], "DoubleSpend");

        // 2. Kernel present in chain DB -> AlreadyMined
        let mined_tx = Transaction::new(
            vec![],
            vec![TransactionOutput::default()],
            vec![mined_kernel],
            PrivateKey::default(),
            PrivateKey::default(),
        );
        let mined_json = serde_json::to_value(&mined_tx).unwrap();
        let response = match call_handle(query_service, mempool, JsonRpcLimiter::default(), request(mined_json)).await {
            Ok(Json(response)) => response,
            Err((status, Json(err))) => panic!("{status}: {}", serde_json::to_string(&err).unwrap()),
        };
        assert!(response.error.is_none(), "{:?}", response.error);
        assert_eq!(response.result["accepted"], false);
        assert_eq!(response.result["rejection_reason"], "AlreadyMined");
    }

    #[tokio::test]
    async fn unknown_method_skips_malformed_params_in_any_field_order_without_touching_mempool() {
        let shutdown = Shutdown::new();
        let (mempool, mempool_state) = create_mempool_service_mock();
        let query_service = make_query_service(&shutdown, mempool.clone());
        let limiter = JsonRpcLimiter::default();

        let huge_array = format!("[{}]", vec!["[0]"; 50_000].join(","));
        let payloads = [
            format!(r#"{{"jsonrpc":"2.0","id":"1\u001b[31m","method":"bogus\r\nforged","params":{huge_array}}}"#),
            format!(r#"{{"params":{huge_array},"method":"bogus","id":"1","jsonrpc":"2.0"}}"#),
            r#"{"jsonrpc":"2.0","id":"1","method":"bogus"}"#.to_string(),
        ];

        for payload in payloads {
            let (status, Json(err)) = call_handle(
                query_service.clone(),
                mempool.clone(),
                limiter.clone(),
                Bytes::from(payload),
            )
            .await
            .unwrap_err();
            assert_eq!(status, StatusCode::BAD_REQUEST);
            let err_json = serde_json::to_string(&err).unwrap();
            assert!(err_json.contains("Method not found"), "{err_json}");
        }
        assert_eq!(mempool_state.get_call_count(), 0);
    }

    #[tokio::test]
    async fn rejects_oversized_id_method_jsonrpc_and_body_without_touching_mempool() {
        let shutdown = Shutdown::new();
        let (mempool, mempool_state) = create_mempool_service_mock();
        mempool_state
            .set_submit_transaction_response(TxStorageResponse::UnconfirmedPool)
            .await;
        let query_service = make_query_service(&shutdown, mempool.clone());
        let limiter = JsonRpcLimiter::default();

        let oversized_id = "i".repeat(MAX_JSON_RPC_ID_BYTES + 1);
        let oversized_method = "m".repeat(MAX_JSON_RPC_METHOD_BYTES + 1);
        let oversized_jsonrpc = "2".repeat(MAX_JSONRPC_VERSION_BYTES + 1);

        for body in [
            serde_json::to_vec(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": oversized_id,
                "method": "submit_transaction",
                "params": {},
            }))
            .unwrap(),
            serde_json::to_vec(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": "1",
                "method": oversized_method,
                "params": {},
            }))
            .unwrap(),
            serde_json::to_vec(&serde_json::json!({
                "jsonrpc": oversized_jsonrpc,
                "id": "1",
                "method": "submit_transaction",
                "params": {},
            }))
            .unwrap(),
        ] {
            let (status, _) = call_handle(
                query_service.clone(),
                mempool.clone(),
                limiter.clone(),
                Bytes::from(body),
            )
            .await
            .unwrap_err();
            assert_eq!(status, StatusCode::BAD_REQUEST);
        }

        let (status, _) = call_handle(
            query_service.clone(),
            mempool.clone(),
            limiter.clone(),
            Bytes::from(vec![b' '; MAX_JSON_RPC_BODY_BYTES + 1]),
        )
        .await
        .unwrap_err();
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);

        let mut bad_headers = HeaderMap::new();
        bad_headers.insert(header::CONTENT_LENGTH, "not-a-number".parse().unwrap());
        assert_eq!(
            validate_content_length(&bad_headers).err().map(|(s, _)| s),
            Some(StatusCode::BAD_REQUEST)
        );
        let mut huge_headers = HeaderMap::new();
        huge_headers.insert(
            header::CONTENT_LENGTH,
            "99999999999999999999999999999999".parse().unwrap(),
        );
        assert_eq!(
            validate_content_length(&huge_headers).err().map(|(s, _)| s),
            Some(StatusCode::PAYLOAD_TOO_LARGE)
        );
        assert_eq!(mempool_state.get_call_count(), 0);

        // Exact boundary id length succeeds and echoes back unchanged.
        let boundary_id = "x".repeat(MAX_JSON_RPC_ID_BYTES);
        let ok_body = serde_json::to_vec(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": boundary_id,
            "method": "submit_transaction",
            "params": { "transaction": transaction_json(None), "version": 2 },
        }))
        .unwrap();
        let resp = match call_handle(query_service, mempool, limiter, Bytes::from(ok_body)).await {
            Ok(Json(resp)) => resp,
            Err((status, Json(err))) => panic!("{status}: {}", serde_json::to_string(&err).unwrap()),
        };
        assert_eq!(resp.id, boundary_id);
        assert_eq!(mempool_state.get_call_count(), 1);
    }

    #[tokio::test]
    async fn rejects_non_object_params_and_missing_or_null_transaction_without_touching_mempool() {
        let shutdown = Shutdown::new();
        let (mempool, mempool_state) = create_mempool_service_mock();
        let query_service = make_query_service(&shutdown, mempool.clone());
        let limiter = JsonRpcLimiter::default();

        // Missing `transaction` field (both `{}` and omitted `params`) returns "Missing transaction parameter".
        for missing_tx in [
            r#"{"jsonrpc":"2.0","id":"1","method":"submit_transaction","params":{}}"#,
            r#"{"jsonrpc":"2.0","id":"1","method":"submit_transaction"}"#,
        ] {
            let (status, Json(err)) = call_handle(
                query_service.clone(),
                mempool.clone(),
                limiter.clone(),
                Bytes::from_static(missing_tx.as_bytes()),
            )
            .await
            .unwrap_err();
            assert_eq!(status, StatusCode::BAD_REQUEST);
            assert!(
                serde_json::to_string(&err)
                    .unwrap()
                    .contains("Missing transaction parameter")
            );
        }

        // Array `params`, array `version`, and `null` `transaction` fail typed streaming decode directly.
        for malformed in [
            r#"{"jsonrpc":"2.0","id":"1","method":"submit_transaction","params":[0,0,0]}"#,
            r#"{"jsonrpc":"2.0","id":"1","method":"submit_transaction","params":{"version":[0,0,0]}}"#,
            r#"{"jsonrpc":"2.0","id":"1","method":"submit_transaction","params":{"transaction":null}}"#,
        ] {
            let (status, _) = call_handle(
                query_service.clone(),
                mempool.clone(),
                limiter.clone(),
                Bytes::from_static(malformed.as_bytes()),
            )
            .await
            .unwrap_err();
            assert_eq!(status, StatusCode::BAD_REQUEST);
        }
        assert_eq!(mempool_state.get_call_count(), 0);
    }

    #[test]
    fn verify_p2p_proto_limits_rejects_over_budget_transaction() {
        let tx = Transaction::new(
            vec![],
            vec![TransactionOutput::default()],
            vec![TransactionKernel::default()],
            PrivateKey::default(),
            PrivateKey::default(),
        );
        let proto = proto::types::Transaction::try_from(tx).unwrap();
        assert!(verify_p2p_proto_limits(&proto, proto::BODY_MAX_DECODE_ITEMS).is_ok());

        let (status, Json(err)) = verify_p2p_proto_limits(&proto, 2).unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(
            serde_json::to_string(&err)
                .unwrap()
                .contains("exceeds P2P decode budget")
        );
    }

    #[tokio::test]
    async fn limiter_sheds_requests_above_byte_budget_and_releases_permits_before_mempool_submission() {
        let shutdown = Shutdown::new();
        let (mempool, mempool_state) = create_mempool_service_mock();
        mempool_state
            .set_submit_transaction_response(TxStorageResponse::UnconfirmedPool)
            .await;
        let query_service = make_query_service(&shutdown, mempool.clone());
        let payload = request(transaction_json(None));
        let budget = payload.len();
        let limiter = JsonRpcLimiter::with_limits(budget, 1);

        let held_bytes = limiter
            .in_flight_bytes
            .clone()
            .try_acquire_many_owned(u32::try_from(budget).unwrap())
            .unwrap();

        let (status, Json(err)) = call_handle(query_service.clone(), mempool.clone(), limiter.clone(), payload.clone())
            .await
            .unwrap_err();
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
        assert!(serde_json::to_string(&err).unwrap().contains("Too many concurrent"));
        assert_eq!(mempool_state.get_call_count(), 0);

        drop(held_bytes);

        // Prove both `in_flight_bytes` and `decode` permits are already 100% released as soon as
        // `read_and_decode_transaction` returns, strictly before `submit_transaction::handle` executes.
        let decoded = read_and_decode_transaction(Body::from(payload.clone()), &limiter)
            .await
            .unwrap_or_else(|(s, _)| panic!("unexpected decode failure: {s}"));
        assert_eq!(decoded.version, 1);
        assert_eq!(limiter.in_flight_bytes.available_permits(), budget);
        assert_eq!(limiter.decode.available_permits(), 1);

        let resp = match call_handle(query_service, mempool, limiter, payload).await {
            Ok(Json(resp)) => resp,
            Err((status, Json(err))) => panic!("{status}: {}", serde_json::to_string(&err).unwrap()),
        };
        assert!(resp.error.is_none());
        assert_eq!(mempool_state.get_call_count(), 1);
    }

    #[tokio::test]
    async fn stalled_header_only_uploads_hold_zero_bytes_and_cannot_starve_honest_client() {
        let shutdown = Shutdown::new();
        let (mempool, mempool_state) = create_mempool_service_mock();
        mempool_state
            .set_submit_transaction_response(TxStorageResponse::UnconfirmedPool)
            .await;
        let query_service = make_query_service(&shutdown, mempool.clone());
        let valid_body = request(transaction_json(None));
        let byte_budget = valid_body.len().saturating_mul(2);
        let limiter = JsonRpcLimiter::with_limits_and_idle_timeout(byte_budget, 2, Duration::from_secs(30));
        let addr = spawn_test_server(query_service, mempool, limiter.clone(), shutdown.to_signal()).await;

        let stalled_headers = concat!(
            "POST /json_rpc HTTP/1.1\r\n",
            "Host: localhost\r\n",
            "Content-Type: application/json\r\n",
            "Connection: close\r\n",
            "Content-Length: 10485760\r\n\r\n",
        );

        let mut stalled_streams = Vec::with_capacity(12);
        for _ in 0..12 {
            let mut stream = TcpStream::connect(addr).await.unwrap();
            stream.write_all(stalled_headers.as_bytes()).await.unwrap();
            stalled_streams.push(stream);
        }

        time::sleep(Duration::from_millis(50)).await;
        assert_eq!(limiter.in_flight_bytes.available_permits(), byte_budget);

        let valid_http = format!(
            "POST /json_rpc HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nConnection: \
             close\r\nContent-Length: {}\r\n\r\n",
            valid_body.len()
        );
        let mut honest_stream = TcpStream::connect(addr).await.unwrap();
        honest_stream.write_all(valid_http.as_bytes()).await.unwrap();
        honest_stream.write_all(&valid_body).await.unwrap();
        let (ok_status, ok_body) = read_http_response(&mut honest_stream).await;
        assert_eq!(ok_status, StatusCode::OK.as_u16(), "{ok_body}");
        let parsed: serde_json::Value = serde_json::from_str(&ok_body).unwrap();
        assert!(parsed["error"].is_null(), "{ok_body}");
        assert_eq!(mempool_state.get_call_count(), 1);
        drop(stalled_streams);
    }

    #[tokio::test]
    async fn unfinished_uploads_exceeding_byte_budget_shed_excess_and_release_on_abort() {
        let shutdown = Shutdown::new();
        let (mempool, mempool_state) = create_mempool_service_mock();
        mempool_state
            .set_submit_transaction_response(TxStorageResponse::UnconfirmedPool)
            .await;
        let query_service = make_query_service(&shutdown, mempool.clone());
        let limiter = JsonRpcLimiter::with_limits_and_idle_timeout(4_096, 1, Duration::from_secs(30));
        let addr = spawn_test_server(query_service, mempool, limiter.clone(), shutdown.to_signal()).await;

        let unfinished_headers = concat!(
            "POST /json_rpc HTTP/1.1\r\n",
            "Host: localhost\r\n",
            "Content-Type: application/json\r\n",
            "Connection: close\r\n",
            "Content-Length: 65536\r\n\r\n",
        );
        let partial_chunk = vec![b' '; 2_048];

        let mut stream_one = TcpStream::connect(addr).await.unwrap();
        stream_one.write_all(unfinished_headers.as_bytes()).await.unwrap();
        stream_one.write_all(&partial_chunk).await.unwrap();

        let mut stream_two = TcpStream::connect(addr).await.unwrap();
        stream_two.write_all(unfinished_headers.as_bytes()).await.unwrap();
        stream_two.write_all(&partial_chunk).await.unwrap();

        time::timeout(Duration::from_secs(5), async {
            while limiter.in_flight_bytes.available_permits() != 0 {
                time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("unfinished uploads did not charge all 4096 buffered byte permits");

        let mut stream_three = TcpStream::connect(addr).await.unwrap();
        stream_three.write_all(unfinished_headers.as_bytes()).await.unwrap();
        stream_three.write_all(b"{\"jsonrpc\":\"2.0\",").await.unwrap();
        let (status, err_body) = read_http_response(&mut stream_three).await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS.as_u16(), "{err_body}");
        assert!(err_body.contains("Too many concurrent"), "{err_body}");
        assert_eq!(mempool_state.get_call_count(), 0);

        drop((stream_one, stream_two));
        time::timeout(Duration::from_secs(5), async {
            while limiter.in_flight_bytes.available_permits() != 4_096 {
                time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("aborted unfinished uploads did not release their byte permits");
    }

    #[tokio::test]
    async fn slow_streaming_upload_succeeds_past_single_idle_window_while_stalled_upload_times_out() {
        let shutdown = Shutdown::new();
        let (mempool, mempool_state) = create_mempool_service_mock();
        mempool_state
            .set_submit_transaction_response(TxStorageResponse::UnconfirmedPool)
            .await;
        let query_service = make_query_service(&shutdown, mempool.clone());
        let limiter = JsonRpcLimiter::with_limits_and_idle_timeout(65_536, 1, Duration::from_millis(120));
        let addr = spawn_test_server(query_service, mempool, limiter.clone(), shutdown.to_signal()).await;

        // 1) Stream valid request in 8 chunks spaced 35ms apart (~280ms total > 2x the 120ms per-chunk idle timeout).
        let valid_body = request(transaction_json(None));
        let valid_http = format!(
            "POST /json_rpc HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nConnection: \
             close\r\nContent-Length: {}\r\n\r\n",
            valid_body.len()
        );
        let mut slow_stream = TcpStream::connect(addr).await.unwrap();
        slow_stream.write_all(valid_http.as_bytes()).await.unwrap();
        let chunk_size = valid_body.len().div_ceil(8).max(1);
        for chunk in valid_body.chunks(chunk_size) {
            slow_stream.write_all(chunk).await.unwrap();
            slow_stream.flush().await.unwrap();
            time::sleep(Duration::from_millis(35)).await;
        }
        let (ok_status, ok_body) = read_http_response(&mut slow_stream).await;
        assert_eq!(ok_status, StatusCode::OK.as_u16(), "{ok_body}");
        assert_eq!(mempool_state.get_call_count(), 1);

        // 2) An upload that stalls past the 120ms chunk idle window gets 408 and frees all charged byte permits.
        let mut stalled_stream = TcpStream::connect(addr).await.unwrap();
        stalled_stream
            .write_all(
                b"POST /json_rpc HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: 65536\r\n\r\n{\"jsonrpc\":\"2.0\",",
            )
            .await
            .unwrap();
        let (status, err_body) = read_http_response(&mut stalled_stream).await;
        assert_eq!(status, StatusCode::REQUEST_TIMEOUT.as_u16(), "{err_body}");
        assert!(
            err_body.contains("Timed out reading JSON-RPC request body"),
            "{err_body}"
        );
        time::timeout(Duration::from_secs(5), async {
            while limiter.in_flight_bytes.available_permits() != 65_536 {
                time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("timed-out stalled upload did not release its byte permits");
    }

    #[tokio::test]
    async fn lenient_version_deserialization_falls_back_to_v1_for_non_u64_values() {
        let shutdown = Shutdown::new();
        let (mempool, mempool_state) = create_mempool_service_mock();
        mempool_state
            .set_submit_transaction_response(TxStorageResponse::NotStoredMempoolPolicy("full".to_string()))
            .await;
        let query_service = make_query_service(&shutdown, mempool.clone());
        let limiter = JsonRpcLimiter::default();
        let tx = transaction_json(None);

        // `version: 2` (`u64`) uses V2 response schema (includes `details`).
        let v2_body = serde_json::to_vec(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": "v2",
            "method": "submit_transaction",
            "params": { "transaction": tx.clone(), "version": 2 },
        }))
        .unwrap();
        let Json(v2_resp) = call_handle(
            query_service.clone(),
            mempool.clone(),
            limiter.clone(),
            Bytes::from(v2_body),
        )
        .await
        .unwrap_or_else(|(s, _)| panic!("unexpected v2 error: {s}"));
        assert_eq!(v2_resp.result.get("details").and_then(|v| v.as_str()), Some("full"));

        // Non-u64 JSON values (`"2"`, `2.0`, `-1`, `null`, `true`, `[2]`, `{"v": 2}`) must not fail with 400 and
        // must fall back to V1 response schema (`details` omitted), matching `as_u64().unwrap_or(1)`.
        for legacy_version in [
            serde_json::json!("2"),
            serde_json::json!(2.0),
            serde_json::json!(-1),
            serde_json::Value::Null,
            serde_json::json!(true),
            serde_json::json!([2]),
            serde_json::json!({"v": 2}),
        ] {
            let body = serde_json::to_vec(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": "legacy",
                "method": "submit_transaction",
                "params": { "transaction": tx.clone(), "version": legacy_version },
            }))
            .unwrap();
            let Json(resp) = call_handle(
                query_service.clone(),
                mempool.clone(),
                limiter.clone(),
                Bytes::from(body),
            )
            .await
            .unwrap_or_else(|(s, Json(e))| panic!("legacy version {legacy_version} rejected with {s}: {}", e.error));
            assert!(resp.error.is_none());
            assert!(
                resp.result.get("details").is_none(),
                "expected V1 fallback without `details` for version={legacy_version}, got {}",
                resp.result
            );
        }
    }
}
