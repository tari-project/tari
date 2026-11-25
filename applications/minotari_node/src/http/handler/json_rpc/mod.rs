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

use std::{
    collections::HashMap,
    error::Error,
    fmt,
    io,
    mem,
    net::SocketAddr,
    sync::{Arc, Mutex, PoisonError},
    time::{Duration, Instant},
};

use axum::{
    Extension,
    Json,
    body::{Body, HttpBody},
    extract::ConnectInfo,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use http_body_util::{BodyExt, LengthLimitError, Limited};
use log::{debug, warn};
use serde::{Deserialize, Serialize, de};
use serde_json::{error::Category, value::RawValue};
use tari_common_types::types::CompressedPublicKey;
use tari_comms::{decode_budget::check_decode_budget, message::MessageExt, utils::text::sanitize_for_display};
use tari_core::{
    base_node::rpc::query_service,
    chain_storage::BlockchainBackend,
    mempool::service::MempoolHandle,
    proto,
};
use tari_script::{Opcode, StackItem, TariScript};
use tari_transaction_components::{
    rpc::models::TxSubmissionResponseV1,
    transaction_components::{SpentOutput, Transaction, TransactionInput, TransactionKernel, TransactionOutput},
};
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore, oneshot},
    task,
    time::timeout,
};

use crate::http::handler::ErrorResponse;

pub mod submit_transaction;

const LOG_TARGET: &str = "c::base_node::rpc::http::handler::json_rpc";

/// A large transaction with 2_316 inputs, 154 outputs and byte size 2_109_809 translated to 4_853_330 JSON object
/// bytes, ~ 2.3 times larger. So the body limit is 2.5 times 4 MB.
pub const MAX_JSON_RPC_BODY_BYTES: usize = 25 * 4 * 1024 * 1024 / 10;
/// The body bytes all in-flight requests may hold together. It is charged per chunk as the chunk arrives, so a stalled
/// upload holds only what it has sent.
const JSON_RPC_BODY_BUDGET_BYTES: usize = 64 * 1024 * 1024;
/// How long a chunk waits for body budget, and a received body for a decode slot, before the request gets a 503
const JSON_RPC_ADMISSION_WAIT: Duration = Duration::from_secs(5);
const MAX_CONCURRENT_JSON_RPC_DECODES: usize = 2;
/// Every window must deliver `JSON_RPC_BODY_MIN_WINDOW_BYTES` of body (or the end of it), or the request gets a 408,
/// so a trickle can not hold budget indefinitely. Time spent waiting for budget is not counted.
const JSON_RPC_BODY_PROGRESS_WINDOW: Duration = Duration::from_secs(10);
const JSON_RPC_BODY_MIN_WINDOW_BYTES: usize = 64 * 1024;
/// A body still unfinished this long (wall clock, budget waits included) after its first chunk was charged to the
/// budget gets a 408. A steady upload of a ~5 MB transaction at about 42 KiB/s or more (e.g. over Tor) gets through.
const JSON_RPC_BODY_MAX_HOLD: Duration = Duration::from_secs(120);
/// The estimated heap all decoded transactions waiting on the mempool may hold together (see `decoded_size`)
const JSON_RPC_DECODED_BUDGET_BYTES: usize = 256 * 1024 * 1024;
/// A larger decoded transaction gets a 413. Kept well below the budget because waiters are served in order, so a
/// near-budget charge at the head of the queue would hold up every smaller one.
const JSON_RPC_MAX_DECODED_TX_BYTES: usize = JSON_RPC_DECODED_BUDGET_BYTES / 4;
/// Decoded transactions waiting on the mempool, whose local request queue is unbounded
const MAX_PENDING_JSON_RPC_SUBMISSIONS: usize = 16;
const REJECTION_WARN_INTERVAL: Duration = Duration::from_secs(60);
const MAX_JSONRPC_BYTES: usize = 16;
const MAX_ID_BYTES: usize = 256;
const MAX_METHOD_BYTES: usize = 128;
const MAX_ERROR_DETAIL_BYTES: usize = 256;

type Rejection = (StatusCode, Json<ErrorResponse>);

fn reject(status: StatusCode, error: impl Into<String>) -> Rejection {
    (status, Json(ErrorResponse::new(error.into())))
}

fn busy() -> Rejection {
    reject(StatusCode::SERVICE_UNAVAILABLE, "The node is busy, try again later")
}

/// Admission state shared by every `/json_rpc` request
pub struct JsonRpcLimits {
    body_budget: Arc<Semaphore>,
    decodes: Arc<Semaphore>,
    pending_submissions: Arc<Semaphore>,
    decoded_budget: Arc<Semaphore>,
    max_decoded_tx_bytes: usize,
    admission_wait: Duration,
    progress_window: Duration,
    min_window_bytes: usize,
    max_hold: Duration,
    /// Per status: when a rejection was last logged, and how many were not logged since
    rejection_warnings: Mutex<HashMap<StatusCode, (Option<Instant>, u64)>>,
}

impl Default for JsonRpcLimits {
    fn default() -> Self {
        Self {
            body_budget: Arc::new(Semaphore::new(JSON_RPC_BODY_BUDGET_BYTES)),
            decodes: Arc::new(Semaphore::new(MAX_CONCURRENT_JSON_RPC_DECODES)),
            pending_submissions: Arc::new(Semaphore::new(MAX_PENDING_JSON_RPC_SUBMISSIONS)),
            decoded_budget: Arc::new(Semaphore::new(JSON_RPC_DECODED_BUDGET_BYTES)),
            max_decoded_tx_bytes: JSON_RPC_MAX_DECODED_TX_BYTES,
            admission_wait: JSON_RPC_ADMISSION_WAIT,
            progress_window: JSON_RPC_BODY_PROGRESS_WINDOW,
            min_window_bytes: JSON_RPC_BODY_MIN_WINDOW_BYTES,
            max_hold: JSON_RPC_BODY_MAX_HOLD,
            rejection_warnings: Mutex::new(HashMap::new()),
        }
    }
}

impl JsonRpcLimits {
    fn warn_rejection(&self, peer: SocketAddr, status: StatusCode) {
        let now = Instant::now();
        let mut warnings = self.rejection_warnings.lock().unwrap_or_else(PoisonError::into_inner);
        let (last_warning, suppressed) = warnings.entry(status).or_default();
        if last_warning.is_some_and(|last| now.saturating_duration_since(last) < REJECTION_WARN_INTERVAL) {
            *suppressed = suppressed.saturating_add(1);
            return;
        }
        *last_warning = Some(now);
        let suppressed = mem::take(suppressed);
        warn!(
            target: LOG_TARGET,
            "Rejected a JSON-RPC request from {peer} with {status} ({suppressed} similar rejections not logged)"
        );
    }
}

pub async fn handle<B: BlockchainBackend + 'static>(
    Extension(query_service): Extension<Arc<query_service::Service<B>>>,
    Extension(mempool_service): Extension<MempoolHandle>,
    Extension(limits): Extension<Arc<JsonRpcLimits>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    match handle_request(query_service, mempool_service, &limits, &headers, body).await {
        Ok(response) => response.into_response(),
        Err((status, error)) => {
            if matches!(
                status,
                StatusCode::PAYLOAD_TOO_LARGE |
                    StatusCode::REQUEST_TIMEOUT |
                    StatusCode::UNSUPPORTED_MEDIA_TYPE |
                    StatusCode::SERVICE_UNAVAILABLE
            ) {
                limits.warn_rejection(peer, status);
            }
            if status == StatusCode::SERVICE_UNAVAILABLE {
                let retry_after = JSON_RPC_ADMISSION_WAIT.as_secs().to_string();
                return (status, [(header::RETRY_AFTER, retry_after)], error).into_response();
            }
            (status, error).into_response()
        },
    }
}

async fn handle_request<B: BlockchainBackend + 'static>(
    query_service: Arc<query_service::Service<B>>,
    mut mempool_service: MempoolHandle,
    limits: &JsonRpcLimits,
    headers: &HeaderMap,
    body: Body,
) -> Result<Json<JsonRpcResponse>, Rejection> {
    if !is_json_content_type(headers) {
        return Err(reject(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Expected request with `Content-Type: application/json`",
        ));
    }
    let (body, budget) = read_body(limits, body).await?;
    let decode_permit = timeout(limits.admission_wait, limits.decodes.clone().acquire_owned())
        .await
        .ok()
        .and_then(Result::ok)
        .ok_or_else(busy)?;
    // The blocking task owns the body and the permits, so a client that disconnects mid-decode frees them only once
    // the decode is done. The decode slot comes back with the result: a decoded transaction is held under it until it
    // is charged to the decoded budget, so at most `MAX_CONCURRENT_JSON_RPC_DECODES` are held uncharged.
    let (decoded, decode_permit) = task::spawn_blocking(move || {
        let decoded = decode_request(&body).map(|(id, version, transaction)| {
            let size = decoded_size(&transaction);
            (id, version, transaction, size)
        });
        drop((body, budget));
        (decoded, decode_permit)
    })
    .await
    .map_err(|e| {
        warn!(target: LOG_TARGET, "JSON-RPC decode task failed: {e}");
        reject(StatusCode::INTERNAL_SERVER_ERROR, "Internal error decoding the request")
    })?;
    let (id, version, transaction, size) = decoded?;

    // Only the decoded-size charge and a pending slot are held while the mempool handles the transaction. Every request
    // takes the decode slot, the decoded charge and the pending slot in this order, so they can not deadlock.
    let too_large = || reject(StatusCode::PAYLOAD_TOO_LARGE, "The decoded transaction is too large");
    if size > limits.max_decoded_tx_bytes {
        return Err(too_large());
    }
    let size = u32::try_from(size).map_err(|_| too_large())?;
    let decoded_charge = timeout(
        limits.admission_wait,
        limits.decoded_budget.clone().acquire_many_owned(size),
    )
    .await
    .ok()
    .and_then(Result::ok)
    .ok_or_else(busy)?;
    drop(decode_permit);
    let pending = timeout(
        limits.admission_wait,
        limits.pending_submissions.clone().acquire_owned(),
    )
    .await
    .ok()
    .and_then(Result::ok)
    .ok_or_else(busy)?;
    // The submission runs in its own task that holds the decoded charge and the pending slot, so a client that
    // disconnects can not free them while the mempool is still validating. The task outlives an abandoned request, but
    // each one holds a pending slot, so there are at most `MAX_PENDING_JSON_RPC_SUBMISSIONS` of them.
    let (reply, submitted) = oneshot::channel();
    task::spawn(async move {
        let submitted = submit_transaction::handle(query_service, &mut mempool_service, transaction).await;
        drop((pending, decoded_charge));
        let _ignore = reply.send(submitted);
    });
    let submitted = submitted.await.map_err(|_| {
        reject(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal error submitting the transaction",
        )
    })?;
    match submitted {
        Ok(response) => {
            // Version defaults to 1 for backward compatibility with older wallets.
            // V2 adds the optional `details` field in the response.
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

/// `application/json` or `application/<anything>+json`, as axum's `Json` extractor accepts
fn is_json_content_type(headers: &HeaderMap) -> bool {
    let Some(content_type) = headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()) else {
        return false;
    };
    let essence = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    essence == "application/json" || (essence.starts_with("application/") && essence.ends_with("+json"))
}

/// Reads the body, charging every chunk to the shared body budget before keeping it. Returns the body and the budget
/// it holds.
async fn read_body(limits: &JsonRpcLimits, body: Body) -> Result<(Vec<u8>, OwnedSemaphorePermit), Rejection> {
    let too_large = || {
        reject(
            StatusCode::PAYLOAD_TOO_LARGE,
            format!("JSON-RPC request body exceeds the maximum of {MAX_JSON_RPC_BODY_BYTES} bytes"),
        )
    };
    // The size hint's lower bound is the Content-Length
    if body.size_hint().lower() > MAX_JSON_RPC_BODY_BYTES as u64 {
        return Err(too_large());
    }
    let mut body = Limited::new(body, MAX_JSON_RPC_BODY_BYTES);
    let mut bytes = Vec::new();
    let mut charged = limits
        .body_budget
        .clone()
        .try_acquire_many_owned(0)
        .map_err(|_| busy())?;
    let too_slow = || {
        reject(
            StatusCode::REQUEST_TIMEOUT,
            "The JSON-RPC request body is arriving too slowly",
        )
    };
    // The progress window leaves out the time this request spent waiting for budget, which is the server's delay; the
    // hold cap is wall clock, so a contended budget can not stretch it
    let mut window_start = Instant::now();
    let mut window_waited = Duration::ZERO;
    let mut window_bytes = 0usize;
    let mut hold_start: Option<Instant> = None;
    let hold_left = |hold_start: Option<Instant>| {
        hold_start.map_or(Duration::MAX, |start| limits.max_hold.saturating_sub(start.elapsed()))
    };
    loop {
        if hold_left(hold_start).is_zero() {
            return Err(too_slow());
        }
        let window_left = limits
            .progress_window
            .saturating_sub(window_start.elapsed().saturating_sub(window_waited));
        let frame = match timeout(window_left.min(hold_left(hold_start)), body.frame()).await {
            Err(_) => return Err(too_slow()),
            Ok(None) => return Ok((bytes, charged)),
            Ok(Some(Err(e))) => {
                let mut sources = std::iter::successors(Some(&*e as &(dyn Error + 'static)), |&e| e.source());
                if sources.any(|e| e.is::<LengthLimitError>()) {
                    return Err(too_large());
                }
                return Err(reject(
                    StatusCode::BAD_REQUEST,
                    format!(
                        "Failed to read the JSON-RPC request body: {}",
                        sanitize_for_display(&e.to_string(), MAX_ERROR_DETAIL_BYTES)
                    ),
                ));
            },
            Ok(Some(Ok(frame))) => frame,
        };
        let Ok(data) = frame.into_data() else {
            continue;
        };
        window_bytes = window_bytes.saturating_add(data.len());
        if window_bytes >= limits.min_window_bytes {
            window_start = Instant::now();
            window_waited = Duration::ZERO;
            window_bytes = 0;
        }
        let len = u32::try_from(data.len()).map_err(|_| too_large())?;
        let wait_start = Instant::now();
        let permit = match timeout(
            limits.admission_wait.min(hold_left(hold_start)),
            limits.body_budget.clone().acquire_many_owned(len),
        )
        .await
        {
            Ok(Ok(permit)) => permit,
            _ if hold_left(hold_start).is_zero() => return Err(too_slow()),
            _ => return Err(busy()),
        };
        charged.merge(permit);
        window_waited = window_waited.saturating_add(wait_start.elapsed());
        hold_start.get_or_insert_with(Instant::now);
        // shortcut: the budget counts body bytes, not the Vec's capacity, which growth can double; charge capacity if
        // the budget is ever sized close to available memory
        bytes.extend_from_slice(&data);
    }
}

/// Parses the envelope and, only for `submit_transaction`, its params
fn decode_request(body: &[u8]) -> Result<(String, u64, Transaction), Rejection> {
    let request = serde_json::from_slice::<JsonRpcRequest>(body).map_err(|e| invalid_json("request", &e))?;
    debug!(
        target: LOG_TARGET,
        "Received JSON-RPC request: method={}, id={}",
        sanitize_for_display(&request.method, MAX_METHOD_BYTES),
        sanitize_for_display(&request.id, MAX_ID_BYTES),
    );
    if request.method != "submit_transaction" {
        return Err(reject(StatusCode::BAD_REQUEST, "Method not found"));
    }
    let (transaction, version) = decode_transaction(request.params.as_deref())?;
    Ok((request.id, version, transaction))
}

/// Decodes the params of a `submit_transaction` request: the transaction and the response version.
///
/// The transaction is decoded and then round-tripped through the P2P protobuf conversion (`Transaction` <->
/// `tari_core::proto::types::Transaction`), i.e. the conversion that transactions received from peers are decoded
/// with, under the same decode budget. The P2P conversion is the canonical decode-time validator for every submission
/// entry point (the gRPC `submit_transaction` handler applies the same round-trip after its own, more lenient, gRPC
/// conversion), so this entry point cannot hand the mempool a transaction that a peer could not have sent.
fn decode_transaction(params: Option<&RawValue>) -> Result<(Transaction, u64), Rejection> {
    let bad_request = |e: String| reject(StatusCode::BAD_REQUEST, e);
    let missing = || bad_request("Missing transaction parameter".to_string());
    let params = serde_json::from_str::<SubmitTransactionParams>(params.ok_or_else(missing)?.get())
        .map_err(|e| invalid_json("submit_transaction params", &e))?;
    // Older wallets send no version, and the old decoder read anything but an unsigned integer as 1
    let version = params
        .version
        .and_then(|v| serde_json::from_str::<u64>(v.get()).ok())
        .unwrap_or(1);
    let transaction = params.transaction.ok_or_else(missing)?;
    let proto = proto::types::Transaction::try_from(transaction).map_err(|e| {
        bad_request(format!(
            "Invalid transaction: {}",
            sanitize_for_display(&e, MAX_ERROR_DETAIL_BYTES)
        ))
    })?;
    check_decode_budget::<proto::types::Transaction>(&proto.to_encoded_bytes(), proto::BODY_MAX_DECODE_ITEMS).map_err(
        |e| {
            bad_request(format!(
                "Transaction exceeds the P2P decode budget ({} embedded items, at most {} allowed)",
                e.items, e.max
            ))
        },
    )?;
    let transaction = Transaction::try_from(proto).map_err(|_| bad_request("Invalid transaction".to_string()))?;
    Ok((transaction, version))
}

/// A 400 that names where a JSON decode failed, without serde's message: that quotes the offending value in full
fn invalid_json(what: &str, e: &serde_json::Error) -> Rejection {
    let category = match e.classify() {
        Category::Io => "I/O",
        Category::Syntax => "syntax",
        Category::Data => "data",
        Category::Eof => "unexpected end of input",
    };
    reject(
        StatusCode::BAD_REQUEST,
        format!(
            "Invalid JSON-RPC {what}: {category} error at line {} column {}",
            e.line(),
            e.column()
        ),
    )
}

/// An upper bound, within about 2x, on the heap a decoded transaction holds:
/// - each input, output and kernel struct, and each execution stack item and script opcode, counted twice because the
///   vectors holding them may have up to twice their length in capacity;
/// - the borsh length of each input, output and kernel, as every buffer they own is at least as long encoded;
/// - for scripts, `size_of::<CompressedPublicKey>()` per 32 encoded bytes on top, as an opcode can box its keys.
fn decoded_size(transaction: &Transaction) -> usize {
    let encoded = |len: io::Result<usize>| len.unwrap_or(usize::MAX);
    let script = |script: &TariScript| {
        script
            .size()
            .saturating_mul(size_of::<Opcode>().saturating_mul(2))
            .saturating_add(
                encoded(borsh::object_length(script)).saturating_mul(size_of::<CompressedPublicKey>().div_ceil(32)),
            )
    };
    let inputs = transaction.body.inputs().iter().map(|input| {
        let spent_script = match &input.spent_output {
            SpentOutput::OutputData { script: s, .. } => script(s),
            SpentOutput::OutputHash(_) => 0,
        };
        size_of::<TransactionInput>()
            .saturating_mul(2)
            .saturating_add(
                input
                    .input_data
                    .size()
                    .saturating_mul(size_of::<StackItem>().saturating_mul(2)),
            )
            .saturating_add(encoded(borsh::object_length(input)))
            .saturating_add(spent_script)
    });
    let outputs = transaction.body.outputs().iter().map(|output| {
        size_of::<TransactionOutput>()
            .saturating_mul(2)
            .saturating_add(encoded(borsh::object_length(output)))
            .saturating_add(script(&output.script))
    });
    let kernels = transaction.body.kernels().iter().map(|kernel| {
        size_of::<TransactionKernel>()
            .saturating_mul(2)
            .saturating_add(encoded(borsh::object_length(kernel)))
    });
    inputs
        .chain(outputs)
        .chain(kernels)
        .fold(size_of::<Transaction>(), usize::saturating_add)
}

/// The JSON-RPC envelope. `params` is kept as raw text, so no JSON tree is built for it.
#[derive(Deserialize)]
struct JsonRpcRequest {
    #[serde(rename = "jsonrpc", deserialize_with = "bounded_string::<MAX_JSONRPC_BYTES, _>")]
    _jsonrpc: String,
    #[serde(deserialize_with = "bounded_string::<MAX_ID_BYTES, _>")]
    id: String,
    #[serde(deserialize_with = "bounded_string::<MAX_METHOD_BYTES, _>")]
    method: String,
    params: Option<Box<RawValue>>,
}

#[derive(Deserialize)]
struct SubmitTransactionParams {
    transaction: Option<Transaction>,
    version: Option<Box<RawValue>>,
}

/// A string of at most `MAX_BYTES` bytes, checked before it is copied
fn bounded_string<'de, const MAX_BYTES: usize, D: de::Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    struct Bounded<const MAX_BYTES: usize>;

    impl<const MAX_BYTES: usize> de::Visitor<'_> for Bounded<MAX_BYTES> {
        type Value = String;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(formatter, "a string of at most {MAX_BYTES} bytes")
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<String, E> {
            if v.len() > MAX_BYTES {
                return Err(E::invalid_length(v.len(), &self));
            }
            Ok(v.to_owned())
        }
    }

    deserializer.deserialize_str(Bounded::<MAX_BYTES>)
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
    use axum::http::HeaderValue;
    use tari_common::configuration::Network;
    use tari_common_types::types::{ComAndPubSignature, FixedHash, MAX_RANGE_PROOF_BYTES, PrivateKey, RangeProof};
    use tari_core::{
        base_node::{StateMachineHandle, state_machine_service::states::StatusInfo},
        block_specs,
        chain_storage::async_db::AsyncBlockchainDb,
        mempool::{TxStorageResponse, test_utils::mock::create_mempool_service_mock},
        test_helpers::blockchain::{TempDatabase, create_main_chain, create_new_blockchain_with_network},
    };
    use tari_script::ExecutionStack;
    use tari_shutdown::Shutdown;
    use tari_transaction_components::transaction_components::{
        TransactionInput,
        TransactionInputVersion,
        TransactionKernel,
        TransactionOutput,
        encrypted_data::{EncryptedData, MAX_ENCRYPTED_DATA_SIZE, STATIC_ENCRYPTED_DATA_SIZE_TOTAL},
    };
    use tari_utilities::hex::to_hex;
    use tokio::{
        io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream, tcp::OwnedWriteHalf},
        sync::{broadcast, watch},
        task::JoinHandle,
        time::sleep,
    };

    use super::*;

    fn make_query_service(shutdown: &Shutdown, mempool: MempoolHandle) -> Arc<query_service::Service<TempDatabase>> {
        let db = AsyncBlockchainDb::from(create_new_blockchain_with_network(Network::LocalNet));
        let (state_tx, _state_rx) = broadcast::channel(10);
        let (_status_tx, status_rx) = watch::channel(StatusInfo::new());
        let state_machine = StateMachineHandle::new(state_tx, status_rx, shutdown.to_signal());
        Arc::new(query_service::Service::new(db, state_machine, mempool))
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

    fn request(transaction: serde_json::Value) -> String {
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": "1",
            "method": "submit_transaction",
            "params": { "transaction": transaction },
        })
        .to_string()
    }

    fn json_headers() -> HeaderMap {
        HeaderMap::from_iter([(header::CONTENT_TYPE, HeaderValue::from_static("application/json"))])
    }

    async fn call(
        query_service: Arc<query_service::Service<TempDatabase>>,
        mempool: MempoolHandle,
        body: impl Into<Body>,
    ) -> Result<Json<JsonRpcResponse>, Rejection> {
        let limits = JsonRpcLimits::default();
        handle_request(query_service, mempool, &limits, &json_headers(), body.into()).await
    }

    async fn submit(
        query_service: Arc<query_service::Service<TempDatabase>>,
        mempool: MempoolHandle,
        body: impl Into<Body>,
    ) -> JsonRpcResponse {
        match call(query_service, mempool, body).await {
            Ok(Json(response)) => response,
            Err((status, Json(err))) => panic!("{status}: {}", serde_json::to_string(&err).unwrap()),
        }
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

        let (status, Json(err)) = call(query_service, mempool, request(tx)).await.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let err = serde_json::to_string(&err).unwrap();
        assert!(err.contains("Invalid JSON-RPC submit_transaction params"), "{}", err);
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

        let response = submit(query_service, mempool, request(transaction_json(None))).await;
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

        let response = submit(query_service, mempool, request(transaction_json(None))).await;
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

        let response = submit(query_service, mempool, request(json)).await;
        assert!(response.error.is_none(), "{:?}", response.error);
        let submitted = mempool_state.submitted_transactions().await;
        assert_eq!(submitted.len(), 1);
        assert_eq!(submitted[0].body.inputs()[0].version, TransactionInputVersion::V0);
    }

    #[tokio::test]
    async fn submit_transaction_distinguishes_double_spend_from_already_mined() {
        let shutdown = Shutdown::new();
        let (mempool, mempool_state) = create_mempool_service_mock();

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
        let mined_tx = Transaction::new(
            vec![],
            vec![TransactionOutput::default()],
            vec![mined_kernel],
            PrivateKey::default(),
            PrivateKey::default(),
        );
        let mined_json = serde_json::to_value(&mined_tx).unwrap();

        for storage_response in [
            TxStorageResponse::NotStoredAlreadySpent,
            TxStorageResponse::ReorgPool,
            TxStorageResponse::NotStoredAlreadyMined,
        ] {
            mempool_state
                .set_submit_transaction_response(storage_response.clone())
                .await;
            for (tx, expected) in [
                (transaction_json(None), "DoubleSpend"),
                (mined_json.clone(), "AlreadyMined"),
            ] {
                let response = submit(query_service.clone(), mempool.clone(), request(tx)).await;
                assert!(response.error.is_none(), "{:?}", response.error);
                assert_eq!(response.result["accepted"], false, "{storage_response:?}");
                assert_eq!(response.result["rejection_reason"], expected, "{storage_response:?}");
            }
        }
    }

    /// Counts the heap the current thread holds while tracking is on
    mod heap {
        use std::{
            alloc::{GlobalAlloc, Layout, System},
            cell::Cell,
        };

        thread_local! {
            static TRACKING: Cell<bool> = const { Cell::new(false) };
            static LIVE: Cell<usize> = const { Cell::new(0) };
            static PEAK: Cell<usize> = const { Cell::new(0) };
        }

        fn record(update: impl FnOnce(usize) -> usize) {
            let _ignore = TRACKING.try_with(|tracking| {
                if tracking.get() {
                    let live = LIVE.with(|live| {
                        live.set(update(live.get()));
                        live.get()
                    });
                    PEAK.with(|peak| peak.set(peak.get().max(live)));
                }
            });
        }

        struct CountingAllocator;

        // SAFETY: every call is forwarded unchanged to `System` under the caller's `GlobalAlloc` contract. `record`
        // neither allocates nor panics: its thread-locals are const-initialised `Cell`s without destructors, reached
        // through `try_with`, and the counts saturate.
        unsafe impl GlobalAlloc for CountingAllocator {
            unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
                // SAFETY: `layout` is forwarded unchanged under the caller's `GlobalAlloc::alloc` contract
                let ptr = unsafe { System.alloc(layout) };
                if !ptr.is_null() {
                    record(|live| live.saturating_add(layout.size()));
                }
                ptr
            }

            unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
                // SAFETY: `ptr` and `layout` are forwarded unchanged under the caller's `GlobalAlloc::dealloc`
                // contract; every allocation came from `System`
                unsafe { System.dealloc(ptr, layout) };
                // Saturating: the memory may have been allocated before tracking started
                record(|live| live.saturating_sub(layout.size()));
            }

            unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
                // SAFETY: the arguments are forwarded unchanged under the caller's `GlobalAlloc::realloc` contract;
                // every allocation came from `System`
                let new_ptr = unsafe { System.realloc(ptr, layout, new_size) };
                if !new_ptr.is_null() {
                    record(|live| live.saturating_sub(layout.size()).saturating_add(new_size));
                }
                new_ptr
            }
        }

        #[global_allocator]
        static ALLOCATOR: CountingAllocator = CountingAllocator;

        /// Runs `f` and returns the most heap it allocated at any one time on this thread, and the heap it still
        /// holds when it returns (including its result)
        pub fn measure<R>(f: impl FnOnce() -> R) -> (R, usize, usize) {
            LIVE.with(|live| live.set(0));
            PEAK.with(|peak| peak.set(0));
            TRACKING.with(|tracking| tracking.set(true));
            let result = f();
            TRACKING.with(|tracking| tracking.set(false));
            (result, PEAK.with(Cell::get), LIVE.with(Cell::get))
        }
    }

    /// An unknown method is rejected without decoding `params` into a JSON tree, which would cost tens of times the
    /// body size
    #[test]
    fn unknown_method_does_not_decode_params() {
        let params = format!("[{}[]]", "[[]],".repeat(2_000_000));
        let body = format!(r#"{{"jsonrpc":"2.0","id":"1","method":"unknown","params":{params}}}"#);
        assert!(body.len() > 9_000_000 && body.len() <= MAX_JSON_RPC_BODY_BYTES);

        let (result, peak, _) = heap::measure(|| decode_request(body.as_bytes()).map(|_| ()));
        let (status, Json(err)) = result.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(serde_json::to_string(&err).unwrap().contains("Method not found"));
        // The raw `params` text is copied once
        assert!(
            peak < body.len().saturating_mul(2),
            "peak {peak} for a {} byte body",
            body.len()
        );
    }

    #[test]
    fn oversized_envelope_strings_are_rejected() {
        let long = |n: usize| "x".repeat(n);
        for (jsonrpc, id, method) in [
            (
                long(MAX_JSONRPC_BYTES + 1),
                "1".to_string(),
                "submit_transaction".to_string(),
            ),
            (
                "2.0".to_string(),
                long(MAX_ID_BYTES + 1),
                "submit_transaction".to_string(),
            ),
            ("2.0".to_string(), "1".to_string(), long(MAX_METHOD_BYTES + 1)),
        ] {
            let body = serde_json::json!({ "jsonrpc": jsonrpc, "id": id, "method": method }).to_string();
            let (status, Json(err)) = decode_request(body.as_bytes()).unwrap_err();
            assert_eq!(status, StatusCode::BAD_REQUEST);
            assert!(
                serde_json::to_string(&err)
                    .unwrap()
                    .contains("Invalid JSON-RPC request")
            );
        }
    }

    /// Request shapes wallets send today keep working; only an integer version of 2 or more gets the V2 response
    #[tokio::test]
    async fn submit_transaction_accepts_existing_request_shapes() {
        let shutdown = Shutdown::new();
        let (mempool, mempool_state) = create_mempool_service_mock();
        // The V2 response carries the policy reason in `details`, the V1 response does not
        mempool_state
            .set_submit_transaction_response(TxStorageResponse::NotStoredMempoolPolicy("full".to_string()))
            .await;
        let query_service = make_query_service(&shutdown, mempool.clone());
        let tx = transaction_json(None);
        let is_v2 = |response: &JsonRpcResponse| response.result.get("details").is_some();

        // The request `clients/rust/base_node_wallet_client` sends
        let client_request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": "1",
            "method": "submit_transaction",
            "params": { "transaction": tx, "version": 2 },
        });
        let response = submit(query_service.clone(), mempool.clone(), client_request.to_string()).await;
        assert!(is_v2(&response), "{:?}", response.result);

        for version in [
            serde_json::json!("2"),
            serde_json::json!(2.0),
            serde_json::json!(-1),
            serde_json::Value::Null,
        ] {
            let body = serde_json::json!({
                "jsonrpc": "2.0",
                "id": "1",
                "method": "submit_transaction",
                "params": { "transaction": tx, "version": version },
            });
            let response = submit(query_service.clone(), mempool.clone(), body.to_string()).await;
            assert!(!is_v2(&response), "version {version}: {:?}", response.result);
        }
        let response = submit(query_service.clone(), mempool.clone(), request(tx.clone())).await;
        assert!(!is_v2(&response), "{:?}", response.result);

        let reordered = format!(
            r#"{{"params":{{"version":2,"transaction":{tx}}},"method":"submit_transaction","id":"7","jsonrpc":"2.0"}}"#
        );
        let response = submit(query_service, mempool, reordered).await;
        assert!(is_v2(&response), "{:?}", response.result);
        assert_eq!(response.id, "7");
        assert_eq!(mempool_state.get_call_count(), 7);
    }

    #[tokio::test]
    async fn submit_transaction_rejects_a_missing_transaction_wrong_content_type_and_oversized_body() {
        let shutdown = Shutdown::new();
        let (mempool, mempool_state) = create_mempool_service_mock();
        let query_service = make_query_service(&shutdown, mempool.clone());

        for body in [
            r#"{"jsonrpc":"2.0","id":"1","method":"submit_transaction"}"#,
            r#"{"jsonrpc":"2.0","id":"1","method":"submit_transaction","params":null}"#,
            r#"{"jsonrpc":"2.0","id":"1","method":"submit_transaction","params":{}}"#,
            r#"{"jsonrpc":"2.0","id":"1","method":"submit_transaction","params":{"transaction":null,"version":2}}"#,
        ] {
            let (status, Json(err)) = call(query_service.clone(), mempool.clone(), body).await.unwrap_err();
            assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
            assert_eq!(
                serde_json::to_value(&err).unwrap(),
                serde_json::json!({ "error": "Missing transaction parameter" })
            );
        }

        let limits = JsonRpcLimits::default();
        for headers in [
            HeaderMap::new(),
            HeaderMap::from_iter([(header::CONTENT_TYPE, HeaderValue::from_static("text/plain"))]),
        ] {
            let body = Body::from(request(transaction_json(None)));
            let (status, _) = handle_request(query_service.clone(), mempool.clone(), &limits, &headers, body)
                .await
                .unwrap_err();
            assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
        }

        let oversized = vec![b' '; MAX_JSON_RPC_BODY_BYTES + 1];
        let (status, _) = call(query_service, mempool, oversized).await.unwrap_err();
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(mempool_state.get_call_count(), 0);
    }

    async fn serve(
        query_service: Arc<query_service::Service<TempDatabase>>,
        mempool: MempoolHandle,
        limits: Arc<JsonRpcLimits>,
    ) -> SocketAddr {
        let router = crate::http::server::routes::<TempDatabase>()
            .layer(Extension(query_service))
            .layer(Extension(mempool))
            .layer(Extension(limits));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(
            async move { axum::serve(listener, router.into_make_service_with_connect_info::<SocketAddr>()).await },
        );
        address
    }

    /// Sends the headers of a `/json_rpc` request with the given `Content-Length`, followed by `body`
    async fn post(address: SocketAddr, content_length: usize, body: &[u8]) -> TcpStream {
        let mut stream = TcpStream::connect(address).await.unwrap();
        let headers = format!(
            "POST /json_rpc HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nConnection: \
             close\r\nContent-Length: {content_length}\r\n\r\n"
        );
        stream.write_all(headers.as_bytes()).await.unwrap();
        stream.write_all(body).await.unwrap();
        stream
    }

    /// Reads the response up to the server closing the connection
    async fn read_response(stream: &mut (impl AsyncRead + Unpin)) -> String {
        let mut response = Vec::new();
        let _ignore = timeout(Duration::from_secs(10), stream.read_to_end(&mut response)).await;
        String::from_utf8_lossy(&response).into_owned()
    }

    const LONG: Duration = Duration::from_secs(10);

    async fn wait_for_budget(limits: &JsonRpcLimits, available: usize, within: Duration) {
        timeout(within, async {
            while limits.body_budget.available_permits() != available {
                sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("body budget never reached {available}"));
    }

    /// Stalled uploads hold only the bytes they sent and no slot, so a submission still gets through; each stalled one
    /// gets a 408 after one progress window
    #[tokio::test]
    async fn stalled_uploads_do_not_starve_a_submission() {
        let shutdown = Shutdown::new();
        let (mempool, mempool_state) = create_mempool_service_mock();
        mempool_state
            .set_submit_transaction_response(TxStorageResponse::UnconfirmedPool)
            .await;
        let query_service = make_query_service(&shutdown, mempool.clone());
        let limits = Arc::new(JsonRpcLimits {
            progress_window: Duration::from_millis(500),
            ..Default::default()
        });
        let address = serve(query_service, mempool, limits.clone()).await;

        let mut stalled = Vec::new();
        for _ in 0..16 {
            stalled.push(post(address, 1_000_000, br#"{"jsonrpc":"2.0","#).await);
        }
        let body = request(transaction_json(None));
        let mut submission = post(address, body.len(), body.as_bytes()).await;
        let response = read_response(&mut submission).await;
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        assert!(response.contains(r#""error":null"#), "{response}");
        assert_eq!(mempool_state.get_call_count(), 1);

        for mut stream in stalled {
            let response = read_response(&mut stream).await;
            assert!(response.starts_with("HTTP/1.1 408"), "{response}");
        }
        wait_for_budget(&limits, JSON_RPC_BODY_BUDGET_BYTES, LONG).await;
    }

    /// Body bytes beyond the budget wait and then get a 503; the budget comes back on a timeout, a disconnect and after
    /// the decode
    #[tokio::test]
    async fn body_budget_is_charged_per_chunk_and_returned() {
        const BUDGET: usize = 64 * 1024;
        let shutdown = Shutdown::new();
        let (mempool, mempool_state) = create_mempool_service_mock();
        mempool_state
            .set_submit_transaction_response(TxStorageResponse::UnconfirmedPool)
            .await;
        let query_service = make_query_service(&shutdown, mempool.clone());
        let limits = Arc::new(JsonRpcLimits {
            body_budget: Arc::new(Semaphore::new(BUDGET)),
            admission_wait: Duration::from_millis(200),
            progress_window: Duration::from_secs(2),
            ..Default::default()
        });
        let address = serve(query_service, mempool, limits.clone()).await;

        // One stalled upload holds the whole budget
        let mut holder = post(address, 2 * BUDGET, &[b' '; BUDGET]).await;
        wait_for_budget(&limits, 0, LONG).await;
        let mut waiting = post(address, 100, b" ").await;
        let response = read_response(&mut waiting).await;
        assert!(response.starts_with("HTTP/1.1 503"), "{response}");
        assert!(response.to_ascii_lowercase().contains("retry-after: 5"), "{response}");

        // Returned when the stalled upload times out
        let response = read_response(&mut holder).await;
        assert!(response.starts_with("HTTP/1.1 408"), "{response}");
        wait_for_budget(&limits, BUDGET, LONG).await;

        // Returned when the client disconnects, well before the progress window would end it
        let partial = post(address, 2 * BUDGET, &[b' '; 1000]).await;
        wait_for_budget(&limits, BUDGET - 1000, LONG).await;
        drop(partial);
        wait_for_budget(&limits, BUDGET, Duration::from_millis(500)).await;

        // Returned after the decode, with the decode slot
        let body = request(transaction_json(None));
        assert!(body.len() < BUDGET);
        let mut submission = post(address, body.len(), body.as_bytes()).await;
        let response = read_response(&mut submission).await;
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        wait_for_budget(&limits, BUDGET, LONG).await;
        assert_eq!(limits.decodes.available_permits(), MAX_CONCURRENT_JSON_RPC_DECODES);
        assert_eq!(mempool_state.get_call_count(), 1);
    }

    /// A fast prefix followed by a trickle below the per-window minimum gets a 408 and frees its budget
    #[tokio::test]
    async fn a_trickle_after_a_fast_prefix_times_out_and_frees_the_budget() {
        const BUDGET: usize = 256 * 1024;
        const PREFIX: usize = BUDGET - 1024;
        let shutdown = Shutdown::new();
        let (mempool, mempool_state) = create_mempool_service_mock();
        mempool_state
            .set_submit_transaction_response(TxStorageResponse::UnconfirmedPool)
            .await;
        let query_service = make_query_service(&shutdown, mempool.clone());
        let limits = Arc::new(JsonRpcLimits {
            body_budget: Arc::new(Semaphore::new(BUDGET)),
            admission_wait: Duration::from_millis(50),
            progress_window: Duration::from_secs(3),
            ..Default::default()
        });
        let address = serve(query_service, mempool, limits.clone()).await;

        let (mut reader, writer) = post(address, 2 * BUDGET, &[b' '; PREFIX]).await.into_split();
        wait_for_budget(&limits, BUDGET - PREFIX, LONG).await;
        let trickle = trickle(writer);
        // While the trickle holds the budget, a submission does not fit
        let body = request(transaction_json(None));
        assert!(body.len() > BUDGET - PREFIX);
        let response = read_response(&mut post(address, body.len(), body.as_bytes()).await).await;
        assert!(response.starts_with("HTTP/1.1 503"), "{response}");

        let response = read_response(&mut reader).await;
        assert!(response.starts_with("HTTP/1.1 408"), "{response}");
        trickle.abort();
        wait_for_budget(&limits, BUDGET, LONG).await;
        let response = read_response(&mut post(address, body.len(), body.as_bytes()).await).await;
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        assert_eq!(mempool_state.get_call_count(), 1);
    }

    /// The router-wide body limit does not cover `/json_rpc`, so its handler rejects (and logs) an oversized body
    #[tokio::test]
    async fn an_oversized_content_length_is_rejected_by_the_handler() {
        let shutdown = Shutdown::new();
        let (mempool, mempool_state) = create_mempool_service_mock();
        let query_service = make_query_service(&shutdown, mempool.clone());
        let limits = Arc::new(JsonRpcLimits::default());
        let address = serve(query_service, mempool, limits.clone()).await;

        let response = read_response(&mut post(address, 11 * 1024 * 1024, b"").await).await;
        assert!(response.starts_with("HTTP/1.1 413"), "{response}");
        assert!(response.contains("exceeds the maximum"), "{response}");
        let warnings = limits.rejection_warnings.lock().unwrap();
        assert!(warnings.contains_key(&StatusCode::PAYLOAD_TOO_LARGE));
        assert_eq!(mempool_state.get_call_count(), 0);
    }

    #[test]
    fn decode_transaction_applies_the_p2p_decode_budget() {
        let decode_items = |outputs: usize| {
            let tx = Transaction::new(
                vec![],
                vec![TransactionOutput::default(); outputs],
                vec![],
                PrivateKey::default(),
                PrivateKey::default(),
            );
            let encoded = proto::types::Transaction::try_from(tx).unwrap().to_encoded_bytes();
            check_decode_budget::<proto::types::Transaction>(&encoded, usize::MAX).unwrap()
        };
        let per_output = decode_items(2).checked_sub(decode_items(1)).unwrap();
        let outputs = proto::BODY_MAX_DECODE_ITEMS
            .checked_div(per_output)
            .unwrap()
            .saturating_add(1);
        let tx = Transaction::new(
            vec![],
            vec![TransactionOutput::default(); outputs],
            vec![],
            PrivateKey::default(),
            PrivateKey::default(),
        );
        let params = RawValue::from_string(serde_json::json!({ "transaction": tx }).to_string()).unwrap();

        let (status, Json(err)) = decode_transaction(Some(&params)).unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let err = serde_json::to_string(&err).unwrap();
        assert!(err.contains("P2P decode budget"), "{err}");
    }

    /// Writes one byte every 50 ms until the connection fails
    fn trickle(mut writer: OwnedWriteHalf) -> JoinHandle<()> {
        tokio::spawn(async move {
            while writer.write_all(b" ").await.is_ok() {
                sleep(Duration::from_millis(50)).await;
            }
        })
    }

    /// A body that keeps making progress still gets a 408 once it has held budget for the maximum time
    #[tokio::test]
    async fn a_body_holding_budget_too_long_times_out() {
        let shutdown = Shutdown::new();
        let (mempool, _mempool_state) = create_mempool_service_mock();
        let query_service = make_query_service(&shutdown, mempool.clone());
        let limits = Arc::new(JsonRpcLimits {
            progress_window: Duration::from_secs(3),
            min_window_bytes: 1,
            max_hold: Duration::from_millis(500),
            ..Default::default()
        });
        let address = serve(query_service, mempool, limits.clone()).await;

        let started = Instant::now();
        let (mut reader, writer) = post(address, 1_000_000, &[b' '; 1024]).await.into_split();
        let trickle = trickle(writer);
        let response = read_response(&mut reader).await;
        assert!(response.starts_with("HTTP/1.1 408"), "{response}");
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "ended by the hold cap, not the window"
        );
        trickle.abort();
        wait_for_budget(&limits, JSON_RPC_BODY_BUDGET_BYTES, LONG).await;
    }

    /// Time spent waiting for budget is the server's, so it does not use up the client's progress window
    #[tokio::test]
    async fn waiting_for_budget_does_not_count_against_the_client() {
        const BUDGET: usize = 1024 * 1024;
        let shutdown = Shutdown::new();
        let (mempool, mempool_state) = create_mempool_service_mock();
        mempool_state
            .set_submit_transaction_response(TxStorageResponse::UnconfirmedPool)
            .await;
        let query_service = make_query_service(&shutdown, mempool.clone());
        let limits = Arc::new(JsonRpcLimits {
            body_budget: Arc::new(Semaphore::new(BUDGET)),
            admission_wait: Duration::from_secs(5),
            progress_window: Duration::from_secs(1),
            ..Default::default()
        });
        let address = serve(query_service, mempool, limits.clone()).await;

        let held = limits
            .body_budget
            .clone()
            .acquire_many_owned(u32::try_from(BUDGET).unwrap())
            .await
            .unwrap();
        let body = request(transaction_json(None));
        let (first, rest) = body.as_bytes().split_at(100);
        // The first chunk waits for budget for longer than the progress window
        let mut submission = post(address, body.len(), first).await;
        sleep(Duration::from_millis(2500)).await;
        drop(held);
        // The rest arrives well inside the window, counted from the end of the wait
        sleep(Duration::from_millis(100)).await;
        submission.write_all(rest).await.unwrap();
        let response = read_response(&mut submission).await;
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        assert_eq!(mempool_state.get_call_count(), 1);
    }

    /// A decoded submission waits for a pending slot and gets a 503 if none frees up; the slot comes back once the
    /// mempool call returns
    #[tokio::test]
    async fn a_decoded_submission_waits_for_a_pending_slot() {
        let shutdown = Shutdown::new();
        let (mempool, mempool_state) = create_mempool_service_mock();
        mempool_state
            .set_submit_transaction_response(TxStorageResponse::UnconfirmedPool)
            .await;
        mempool_state.set_submit_transaction_delay(Duration::from_secs(2)).await;
        let query_service = make_query_service(&shutdown, mempool.clone());
        let limits = Arc::new(JsonRpcLimits {
            pending_submissions: Arc::new(Semaphore::new(1)),
            admission_wait: Duration::from_millis(200),
            ..Default::default()
        });

        let first = tokio::spawn({
            let (query_service, mempool, limits) = (query_service.clone(), mempool.clone(), limits.clone());
            async move {
                let body = Body::from(request(transaction_json(None)));
                handle_request(query_service, mempool, &limits, &json_headers(), body)
                    .await
                    .map(|Json(response)| response)
                    .map_err(|(status, _)| status)
            }
        });
        timeout(LONG, async {
            while limits.pending_submissions.available_permits() != 0 {
                sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("the first submission never took the pending slot");

        let body = Body::from(request(transaction_json(None)));
        let (status, _) = handle_request(query_service, mempool, &limits, &json_headers(), body)
            .await
            .unwrap_err();
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);

        let response = first.await.unwrap().unwrap();
        assert!(response.error.is_none(), "{:?}", response.error);
        assert_eq!(limits.pending_submissions.available_permits(), 1);
        assert_eq!(mempool_state.get_call_count(), 1);
    }

    /// A decode error names where it failed, but does not quote the offending value
    #[tokio::test]
    async fn decode_errors_do_not_echo_the_request() {
        let shutdown = Shutdown::new();
        let (mempool, mempool_state) = create_mempool_service_mock();
        let query_service = make_query_service(&shutdown, mempool.clone());
        let long = "\u{7f}".repeat(10_000_000);
        for body in [
            format!(r#""{long}""#),
            format!(r#"{{"jsonrpc":"2.0","id":"1","method":"submit_transaction","params":"{long}"}}"#),
            format!(
                r#"{{"jsonrpc":"2.0","id":"1","method":"submit_transaction","params":{{"transaction":"{long}"}}}}"#
            ),
        ] {
            assert!(body.len() <= MAX_JSON_RPC_BODY_BYTES);
            let (status, Json(err)) = call(query_service.clone(), mempool.clone(), body).await.unwrap_err();
            assert_eq!(status, StatusCode::BAD_REQUEST);
            let err = serde_json::to_string(&err).unwrap();
            assert!(err.len() < 1024, "{} byte error", err.len());
            assert!(err.contains("Invalid JSON-RPC"), "{err}");
        }
        assert_eq!(mempool_state.get_call_count(), 0);
    }

    /// The decoded-size estimate covers the heap a decoded transaction holds, without overstating it much, for the
    /// shapes that hold the most heap per encoded byte
    #[test]
    fn decoded_size_bounds_the_decoded_heap() {
        let stack_of = |item: StackItem| ExecutionStack::new(vec![item; 255]);
        // A stack just over a power of two, where pushing one item at a time leaves the most spare capacity
        let partial_stack = ExecutionStack::new(vec![StackItem::Number(1); 129]);
        let compact_inputs = |stack: ExecutionStack| -> Vec<TransactionInput> {
            (0..100)
                .map(|_| {
                    TransactionInput::new_with_output_hash(
                        FixedHash::zero(),
                        stack.clone(),
                        ComAndPubSignature::default(),
                    )
                })
                .collect()
        };
        // A script of boxed keys and a 32-key multisig, a full range proof and full encrypted data
        let rich_output = || {
            let mut script = vec![Opcode::PushPubKey(Box::default()); 90];
            script.push(Opcode::CheckMultiSig(
                1,
                32,
                vec![CompressedPublicKey::default(); 32],
                Box::new([0u8; 32]),
            ));
            TransactionOutput {
                script: TariScript::new(script).unwrap(),
                proof: Some(RangeProof::from(vec![1u8; MAX_RANGE_PROOF_BYTES])),
                encrypted_data: EncryptedData::from_bytes(&[1u8; MAX_ENCRYPTED_DATA_SIZE]).unwrap(),
                ..Default::default()
            }
        };
        let full_inputs = || -> Vec<TransactionInput> {
            (0..50)
                .map(|_| {
                    TransactionInput::new_current_version(
                        SpentOutput::create_from_output(rich_output()),
                        ExecutionStack::default(),
                        ComAndPubSignature::default(),
                    )
                })
                .collect()
        };
        let multisig_output = |n: u8| TransactionOutput {
            script: TariScript::new(vec![Opcode::CheckMultiSig(
                1,
                n,
                vec![CompressedPublicKey::default(); usize::from(n)],
                Box::new([0u8; 32]),
            )])
            .unwrap(),
            ..Default::default()
        };
        let mut cases: Vec<(String, Vec<TransactionInput>, Vec<TransactionOutput>)> = vec![
            (
                "number stacks".into(),
                compact_inputs(stack_of(StackItem::Number(1))),
                vec![],
            ),
            ("129-item number stacks".into(), compact_inputs(partial_stack), vec![]),
            (
                "public key stacks".into(),
                compact_inputs(stack_of(StackItem::PublicKey(CompressedPublicKey::default()))),
                vec![],
            ),
            (
                "signature stacks".into(),
                compact_inputs(stack_of(StackItem::Signature(Default::default()))),
                vec![],
            ),
            (
                "commitment stacks".into(),
                compact_inputs(stack_of(StackItem::Commitment(Default::default()))),
                vec![],
            ),
            ("full spent outputs".into(), full_inputs(), vec![]),
            ("rich outputs".into(), vec![], (0..50).map(|_| rich_output()).collect()),
        ];
        // Decode does not enforce the 32-key multisig limit
        for n in [1, 5, 17, 33, 65] {
            cases.push((
                format!("{n}-key multisig scripts"),
                vec![],
                (0..100).map(|_| multisig_output(n)).collect(),
            ));
        }
        for (name, inputs, mut outputs) in cases {
            outputs.push(TransactionOutput::default());
            let tx = Transaction::new(
                inputs,
                outputs,
                vec![TransactionKernel::default()],
                PrivateKey::default(),
                PrivateKey::default(),
            );
            let params = RawValue::from_string(serde_json::json!({ "transaction": tx }).to_string()).unwrap();

            let (decoded, _, held) = heap::measure(|| decode_transaction(Some(&params)).ok());
            let (decoded, _) = decoded.unwrap_or_else(|| panic!("{name}: the transaction does not decode"));
            let estimate = decoded_size(&decoded);
            assert!(
                estimate >= held && estimate <= held.saturating_mul(3),
                "{name}: estimate {estimate} for {held} decoded bytes"
            );
        }
    }

    /// A decoded transaction keeps its decode slot until it is charged to the decoded budget
    #[tokio::test]
    async fn the_decode_slot_is_held_until_the_decoded_charge() {
        let shutdown = Shutdown::new();
        let (mempool, mempool_state) = create_mempool_service_mock();
        let query_service = make_query_service(&shutdown, mempool.clone());
        let body = request(transaction_json(None));
        let (_, _, decoded) = decode_request(body.as_bytes()).ok().unwrap();
        let size = decoded_size(&decoded);
        let limits = Arc::new(JsonRpcLimits {
            decoded_budget: Arc::new(Semaphore::new(size)),
            admission_wait: Duration::from_secs(1),
            ..Default::default()
        });
        let held = limits
            .decoded_budget
            .acquire_many(u32::try_from(size).unwrap())
            .await
            .unwrap();

        let submission = tokio::spawn({
            let limits = limits.clone();
            async move {
                handle_request(query_service, mempool, &limits, &json_headers(), Body::from(body))
                    .await
                    .map(|_| ())
                    .map_err(|(status, _)| status)
            }
        });
        // By now the transaction is decoded and waiting for the decoded budget
        sleep(Duration::from_millis(300)).await;
        loop {
            let available = limits.decodes.available_permits();
            if submission.is_finished() {
                break;
            }
            assert!(
                available < MAX_CONCURRENT_JSON_RPC_DECODES,
                "the decode slot was freed early"
            );
            sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(submission.await.unwrap(), Err(StatusCode::SERVICE_UNAVAILABLE));
        assert_eq!(limits.decodes.available_permits(), MAX_CONCURRENT_JSON_RPC_DECODES);
        assert_eq!(mempool_state.get_call_count(), 0);
        drop(held);
    }

    /// A transaction above the per-transaction decoded-size cap gets a 413; one that has to wait for the decoded budget
    /// too long a 503
    #[tokio::test]
    async fn the_decoded_size_budget_rejects_and_waits() {
        let shutdown = Shutdown::new();
        let (mempool, mempool_state) = create_mempool_service_mock();
        mempool_state
            .set_submit_transaction_response(TxStorageResponse::UnconfirmedPool)
            .await;
        let query_service = make_query_service(&shutdown, mempool.clone());
        let body = request(transaction_json(None));
        let (_, _, decoded) = decode_request(body.as_bytes()).ok().unwrap();
        let size = decoded_size(&decoded);
        let limits_of = |max_tx: usize| JsonRpcLimits {
            decoded_budget: Arc::new(Semaphore::new(size)),
            max_decoded_tx_bytes: max_tx,
            admission_wait: Duration::from_millis(200),
            ..Default::default()
        };
        let submit = |limits: Arc<JsonRpcLimits>| {
            let (query_service, mempool, body) = (query_service.clone(), mempool.clone(), body.clone());
            async move { handle_request(query_service, mempool, &limits, &json_headers(), Body::from(body)).await }
        };

        let (status, _) = submit(Arc::new(limits_of(size.checked_sub(1).unwrap())))
            .await
            .unwrap_err();
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);

        let limits = Arc::new(limits_of(size));
        let held = limits.decoded_budget.acquire().await.unwrap();
        let (status, _) = submit(limits.clone()).await.unwrap_err();
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(mempool_state.get_call_count(), 0);

        drop(held);
        let Json(response) = submit(limits.clone()).await.ok().unwrap();
        assert!(response.error.is_none(), "{:?}", response.error);
        assert_eq!(limits.decoded_budget.available_permits(), size);
        assert_eq!(mempool_state.get_call_count(), 1);
    }

    /// The hold cap is wall clock: a body whose chunk keeps waiting for budget still ends at it
    #[tokio::test]
    async fn a_body_waiting_for_budget_still_hits_the_hold_cap() {
        const BUDGET: usize = 64 * 1024;
        let shutdown = Shutdown::new();
        let (mempool, _mempool_state) = create_mempool_service_mock();
        let query_service = make_query_service(&shutdown, mempool.clone());
        let limits = Arc::new(JsonRpcLimits {
            body_budget: Arc::new(Semaphore::new(BUDGET)),
            admission_wait: Duration::from_secs(5),
            progress_window: Duration::from_secs(5),
            max_hold: Duration::from_secs(1),
            ..Default::default()
        });
        let address = serve(query_service, mempool, limits.clone()).await;

        let started = Instant::now();
        let mut stream = post(address, 4096, &[b' '; 1024]).await;
        wait_for_budget(&limits, BUDGET - 1024, LONG).await;
        let held = limits
            .body_budget
            .clone()
            .acquire_many_owned(u32::try_from(BUDGET - 1024).unwrap())
            .await
            .unwrap();
        stream.write_all(&[b' '; 1024]).await.unwrap();
        let response = read_response(&mut stream).await;
        assert!(response.starts_with("HTTP/1.1 408"), "{response}");
        assert!(started.elapsed() < Duration::from_secs(4), "ended by the hold cap");
        drop(held);
        wait_for_budget(&limits, BUDGET, LONG).await;
    }

    /// A client that goes away while the mempool handles its transaction does not free the decoded charge or the
    /// pending slot before the mempool replies
    #[tokio::test]
    async fn an_abandoned_submission_keeps_its_charges_until_the_mempool_replies() {
        let shutdown = Shutdown::new();
        let (mempool, mempool_state) = create_mempool_service_mock();
        mempool_state
            .set_submit_transaction_response(TxStorageResponse::UnconfirmedPool)
            .await;
        mempool_state.set_submit_transaction_delay(Duration::from_secs(1)).await;
        let query_service = make_query_service(&shutdown, mempool.clone());
        let limits = Arc::new(JsonRpcLimits::default());

        let client = tokio::spawn({
            let limits = limits.clone();
            async move {
                let body = Body::from(request(transaction_json(None)));
                let _ignore = handle_request(query_service, mempool, &limits, &json_headers(), body).await;
            }
        });
        let pending_in_use = || MAX_PENDING_JSON_RPC_SUBMISSIONS - limits.pending_submissions.available_permits();
        timeout(LONG, async {
            while pending_in_use() == 0 {
                sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("the submission never took a pending slot");
        client.abort();
        assert!(client.await.unwrap_err().is_cancelled());

        sleep(Duration::from_millis(300)).await;
        assert_eq!(pending_in_use(), 1);
        assert!(limits.decoded_budget.available_permits() < JSON_RPC_DECODED_BUDGET_BYTES);

        timeout(LONG, async {
            while pending_in_use() != 0 || limits.decoded_budget.available_permits() != JSON_RPC_DECODED_BUDGET_BYTES {
                sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("the charges never came back");
        assert_eq!(mempool_state.get_call_count(), 1);
    }
}
