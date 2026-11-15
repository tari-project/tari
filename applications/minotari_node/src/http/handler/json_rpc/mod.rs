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

use std::sync::Arc;

use axum::{Extension, Json, body::Bytes, http::StatusCode};
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
use tokio::{sync::Semaphore, task};

use crate::http::handler::ErrorResponse;

pub mod submit_transaction;

const LOG_TARGET: &str = "c::base_node::rpc::http::handler::json_rpc";

/// Maximum JSON-RPC HTTP request body size (10 MiB, matching the HTTP router's `RequestBodyLimitLayer`).
pub const MAX_JSON_RPC_BODY_BYTES: usize = 25 * 4 * 1024 * 1024 / 10;
/// Maximum number of JSON-RPC requests allowed in flight (decoding or awaiting mempool submission) at once.
/// Requests beyond this cap are shed immediately with `429 Too Many Requests` so raw request bodies cannot pile up.
pub const MAX_IN_FLIGHT_JSON_RPC_REQUESTS: usize = 8;
/// Maximum number of JSON-RPC requests running synchronous JSON + protobuf transaction decoding at once,
/// matching `MempoolService`'s `MAX_CONCURRENT_HANDLE_SUBMISSIONS`.
pub const MAX_CONCURRENT_JSON_RPC_DECODES: usize = 2;

const MAX_JSONRPC_VERSION_BYTES: usize = 16;
const MAX_JSON_RPC_ID_BYTES: usize = 256;
const MAX_JSON_RPC_METHOD_BYTES: usize = 128;

/// Per-server concurrency bounds for the unauthenticated `/json_rpc` ingress.
#[derive(Clone, Debug)]
pub struct JsonRpcLimiter {
    in_flight: Arc<Semaphore>,
    decode: Arc<Semaphore>,
}

impl Default for JsonRpcLimiter {
    fn default() -> Self {
        Self::with_limits(MAX_IN_FLIGHT_JSON_RPC_REQUESTS, MAX_CONCURRENT_JSON_RPC_DECODES)
    }
}

impl JsonRpcLimiter {
    pub fn with_limits(max_in_flight: usize, max_concurrent_decodes: usize) -> Self {
        Self {
            in_flight: Arc::new(Semaphore::new(max_in_flight.max(1))),
            decode: Arc::new(Semaphore::new(max_concurrent_decodes.max(1))),
        }
    }
}

pub async fn handle<B: BlockchainBackend + 'static>(
    Extension(query_service): Extension<Arc<query_service::Service<B>>>,
    Extension(mempool_service): Extension<MempoolHandle>,
    Extension(limiter): Extension<JsonRpcLimiter>,
    body: Bytes,
) -> Result<Json<JsonRpcResponse>, (StatusCode, Json<ErrorResponse>)> {
    if body.len() > MAX_JSON_RPC_BODY_BYTES {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(ErrorResponse::new(format!(
                "JSON-RPC request body ({} bytes) exceeds maximum allowed size ({MAX_JSON_RPC_BODY_BYTES} bytes)",
                body.len()
            ))),
        ));
    }

    let in_flight_permit = limiter.in_flight.clone().try_acquire_owned().map_err(|_| {
        (
            StatusCode::TOO_MANY_REQUESTS,
            Json(ErrorResponse::new(
                "Too many concurrent JSON-RPC requests; try again later".to_string(),
            )),
        )
    })?;

    let decode_permit = limiter.decode.clone().acquire_owned().await.map_err(|_| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse::new("JSON-RPC decode limiter closed".to_string())),
        )
    })?;

    // Hold both permits inside the blocking task so that an HTTP client disconnecting mid-decode cannot release
    // its decode or in-flight slot before the blocking thread finishes freeing the request buffers.
    let (_in_flight_permit, decoded) = task::spawn_blocking(move || {
        let _decode_permit = decode_permit;
        let result = decode_transaction(&body);
        (in_flight_permit, result)
    })
    .await
    .map_err(|e| {
        warn!(target: LOG_TARGET, "JSON-RPC decode task failed: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("Internal error decoding request".to_string())),
        )
    })?;

    let DecodedSubmitTransaction {
        id,
        version,
        transaction,
    } = decoded?;

    match submit_transaction::handle(query_service.clone(), &mut (mempool_service.clone()), transaction).await {
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

#[derive(Default, Deserialize)]
struct SubmitTransactionParams {
    #[serde(default, deserialize_with = "deserialize_present_transaction")]
    transaction: Option<Transaction>,
    #[serde(default)]
    version: Option<u64>,
}

fn deserialize_present_transaction<'de, D>(deserializer: D) -> Result<Option<Transaction>, D::Error>
where D: serde::Deserializer<'de> {
    Transaction::deserialize(deserializer).map(Some)
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
    // Version defaults to 1 for backward compatibility with older wallets.
    // V2 adds the optional `details` field in the response.
    let version = submit.params.version.unwrap_or(1);
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
    use tari_shutdown::Shutdown;
    use tari_transaction_components::transaction_components::{
        TransactionInput,
        TransactionInputVersion,
        TransactionKernel,
        TransactionOutput,
        encrypted_data::STATIC_ENCRYPTED_DATA_SIZE_TOTAL,
    };
    use tari_utilities::hex::to_hex;
    use tokio::sync::{broadcast, watch};

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

        let (status, Json(err)) = handle(
            Extension(query_service),
            Extension(mempool),
            Extension(JsonRpcLimiter::default()),
            request(tx),
        )
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

        let response = match handle(
            Extension(query_service),
            Extension(mempool),
            Extension(JsonRpcLimiter::default()),
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

        let response = match handle(
            Extension(query_service),
            Extension(mempool),
            Extension(JsonRpcLimiter::default()),
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

        let response = match handle(
            Extension(query_service),
            Extension(mempool),
            Extension(JsonRpcLimiter::default()),
            request(json),
        )
        .await
        {
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
        let response = match handle(
            Extension(query_service.clone()),
            Extension(mempool.clone()),
            Extension(JsonRpcLimiter::default()),
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
        let response = match handle(
            Extension(query_service),
            Extension(mempool),
            Extension(JsonRpcLimiter::default()),
            request(mined_json),
        )
        .await
        {
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
            let (status, Json(err)) = handle(
                Extension(query_service.clone()),
                Extension(mempool.clone()),
                Extension(limiter.clone()),
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
            let (status, _) = handle(
                Extension(query_service.clone()),
                Extension(mempool.clone()),
                Extension(limiter.clone()),
                Bytes::from(body),
            )
            .await
            .unwrap_err();
            assert_eq!(status, StatusCode::BAD_REQUEST);
        }

        let (status, _) = handle(
            Extension(query_service.clone()),
            Extension(mempool.clone()),
            Extension(limiter.clone()),
            Bytes::from(vec![b' '; MAX_JSON_RPC_BODY_BYTES + 1]),
        )
        .await
        .unwrap_err();
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
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
        let resp = match handle(
            Extension(query_service),
            Extension(mempool),
            Extension(limiter),
            Bytes::from(ok_body),
        )
        .await
        {
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
            let (status, Json(err)) = handle(
                Extension(query_service.clone()),
                Extension(mempool.clone()),
                Extension(limiter.clone()),
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
            let (status, _) = handle(
                Extension(query_service.clone()),
                Extension(mempool.clone()),
                Extension(limiter.clone()),
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
    async fn limiter_sheds_requests_above_in_flight_cap_and_releases_slots_after_completion() {
        let shutdown = Shutdown::new();
        let (mempool, mempool_state) = create_mempool_service_mock();
        mempool_state
            .set_submit_transaction_response(TxStorageResponse::UnconfirmedPool)
            .await;
        let query_service = make_query_service(&shutdown, mempool.clone());
        let limiter = JsonRpcLimiter::with_limits(2, 1);

        let permit_one = limiter.in_flight.clone().try_acquire_owned().unwrap();
        let permit_two = limiter.in_flight.clone().try_acquire_owned().unwrap();

        let (status, Json(err)) = handle(
            Extension(query_service.clone()),
            Extension(mempool.clone()),
            Extension(limiter.clone()),
            request(transaction_json(None)),
        )
        .await
        .unwrap_err();
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
        assert!(serde_json::to_string(&err).unwrap().contains("Too many concurrent"));
        assert_eq!(mempool_state.get_call_count(), 0);

        drop((permit_one, permit_two));

        let resp = match handle(
            Extension(query_service),
            Extension(mempool),
            Extension(limiter),
            request(transaction_json(None)),
        )
        .await
        {
            Ok(Json(resp)) => resp,
            Err((status, Json(err))) => panic!("{status}: {}", serde_json::to_string(&err).unwrap()),
        };
        assert!(resp.error.is_none());
        assert_eq!(mempool_state.get_call_count(), 1);
    }
}
