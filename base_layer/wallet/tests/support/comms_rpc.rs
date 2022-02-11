// Copyright 2020. The Tari Project
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
    cmp::min,
    collections::HashMap,
    convert::TryFrom,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use tari_common_types::types::{HashOutput, Signature};
use tari_comms::{
    protocol::rpc::{NamedProtocolService, Request, Response, RpcClient, RpcStatus, Streaming},
    PeerConnection,
};
use tari_core::{
    base_node::{
        proto::wallet_rpc::{TxLocation, TxQueryResponse, TxSubmissionRejectionReason, TxSubmissionResponse},
        rpc::BaseNodeWalletService,
    },
    blocks::BlockHeader,
    proto,
    proto::{
        base_node::{
            ChainMetadata as ChainMetadataProto,
            FetchMatchingUtxos,
            FetchUtxosResponse,
            QueryDeletedRequest,
            QueryDeletedResponse,
            Signatures as SignaturesProto,
            SyncUtxosByBlockRequest,
            SyncUtxosByBlockResponse,
            TipInfoResponse,
            TxQueryBatchResponses as TxQueryBatchResponsesProto,
            TxQueryResponse as TxQueryResponseProto,
            TxSubmissionResponse as TxSubmissionResponseProto,
            UtxoQueryRequest,
            UtxoQueryResponses,
        },
        types::{
            Signature as SignatureProto,
            Transaction as TransactionProto,
            TransactionOutput as TransactionOutputProto,
        },
    },
    transactions::transaction_components::{Transaction, TransactionOutput},
};
use tari_utilities::Hashable;
use tokio::{sync::mpsc, time::sleep};

pub async fn connect_rpc_client<T>(connection: &mut PeerConnection) -> T
where T: From<RpcClient> + NamedProtocolService {
    let framed = connection
        .open_framed_substream(&T::PROTOCOL_NAME.into(), 1024 * 1024)
        .await
        .unwrap();

    RpcClient::builder()
        .with_protocol_id(T::PROTOCOL_NAME.into())
        .connect(framed)
        .await
        .unwrap()
}

#[derive(Clone, Debug)]
pub struct BaseNodeWalletRpcMockState {
    submit_transaction_calls: Arc<Mutex<Vec<Transaction>>>,
    transaction_query_calls: Arc<Mutex<Vec<Signature>>>,
    transaction_batch_query_calls: Arc<Mutex<Vec<Vec<Signature>>>>,
    utxo_query_calls: Arc<Mutex<Vec<Vec<Vec<u8>>>>>,
    query_deleted_calls: Arc<Mutex<Vec<QueryDeletedRequest>>>,
    get_header_by_height_calls: Arc<Mutex<Vec<u64>>>,
    get_height_at_time_calls: Arc<Mutex<Vec<u64>>>,
    sync_utxo_by_block_calls: Arc<Mutex<Vec<(HashOutput, HashOutput)>>>,
    submit_transaction_response: Arc<Mutex<TxSubmissionResponse>>,
    transaction_query_response: Arc<Mutex<TxQueryResponse>>,
    transaction_query_batch_response: Arc<Mutex<TxQueryBatchResponsesProto>>,
    tip_info_response: Arc<Mutex<TipInfoResponse>>,
    utxo_query_response: Arc<Mutex<UtxoQueryResponses>>,
    query_deleted_response: Arc<Mutex<QueryDeletedResponse>>,
    fetch_utxos_calls: Arc<Mutex<Vec<Vec<Vec<u8>>>>>,
    response_delay: Arc<Mutex<Option<Duration>>>,
    rpc_status_error: Arc<Mutex<Option<RpcStatus>>>,
    get_header_response: Arc<Mutex<Option<BlockHeader>>>,
    synced: Arc<Mutex<bool>>,
    utxos: Arc<Mutex<Vec<TransactionOutput>>>,
    blocks: Arc<Mutex<HashMap<u64, BlockHeader>>>,
    utxos_by_block: Arc<Mutex<Vec<UtxosByBlock>>>,
    sync_utxos_by_block_trigger_channel: Arc<Mutex<Option<mpsc::Receiver<usize>>>>,
}

#[allow(clippy::mutex_atomic)]
impl BaseNodeWalletRpcMockState {
    pub fn new() -> Self {
        Self {
            submit_transaction_calls: Arc::new(Mutex::new(Vec::new())),
            transaction_query_calls: Arc::new(Mutex::new(Vec::new())),
            transaction_batch_query_calls: Arc::new(Mutex::new(Vec::new())),
            utxo_query_calls: Arc::new(Mutex::new(vec![])),
            query_deleted_calls: Arc::new(Mutex::new(vec![])),
            get_header_by_height_calls: Arc::new(Mutex::new(vec![])),
            get_height_at_time_calls: Arc::new(Mutex::new(vec![])),
            sync_utxo_by_block_calls: Arc::new(Mutex::new(vec![])),
            submit_transaction_response: Arc::new(Mutex::new(TxSubmissionResponse {
                accepted: true,
                rejection_reason: TxSubmissionRejectionReason::None,
                is_synced: true,
            })),
            transaction_query_response: Arc::new(Mutex::new(TxQueryResponse {
                location: TxLocation::InMempool,
                block_hash: None,
                confirmations: 0,
                is_synced: true,
                height_of_longest_chain: 0,
            })),
            transaction_query_batch_response: Arc::new(Mutex::new(TxQueryBatchResponsesProto {
                responses: vec![],
                tip_hash: Some(vec![]),
                is_synced: true,
                height_of_longest_chain: 0,
            })),
            tip_info_response: Arc::new(Mutex::new(TipInfoResponse {
                metadata: Some(ChainMetadataProto {
                    height_of_longest_chain: Some(std::i64::MAX as u64),
                    best_block: Some(Vec::new()),
                    accumulated_difficulty: Vec::new(),
                    pruned_height: 0,
                }),
                is_synced: true,
            })),
            utxo_query_response: Arc::new(Mutex::new(UtxoQueryResponses {
                responses: vec![],
                best_block: vec![],
                height_of_longest_chain: 1,
            })),
            query_deleted_response: Arc::new(Mutex::new(QueryDeletedResponse {
                deleted_positions: vec![],
                not_deleted_positions: vec![],
                best_block: vec![],
                height_of_longest_chain: 1,
                heights_deleted_at: vec![],
                blocks_deleted_in: vec![],
            })),
            fetch_utxos_calls: Arc::new(Mutex::new(Vec::new())),
            response_delay: Arc::new(Mutex::new(None)),
            rpc_status_error: Arc::new(Mutex::new(None)),
            get_header_response: Arc::new(Mutex::new(None)),
            synced: Arc::new(Mutex::new(true)),
            utxos: Arc::new(Mutex::new(Vec::new())),
            blocks: Arc::new(Mutex::new(Default::default())),
            utxos_by_block: Arc::new(Mutex::new(vec![])),
            sync_utxos_by_block_trigger_channel: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_tip_info_response(&self, response: TipInfoResponse) {
        let mut lock = acquire_lock!(self.tip_info_response);
        *lock = response;
    }

    pub fn set_submit_transaction_response(&self, response: TxSubmissionResponse) {
        let mut lock = acquire_lock!(self.submit_transaction_response);
        *lock = response;
    }

    pub fn set_transaction_query_response(&self, response: TxQueryResponse) {
        let mut lock = acquire_lock!(self.transaction_query_response);
        *lock = response;
    }

    pub fn set_transaction_query_batch_responses(&self, response: TxQueryBatchResponsesProto) {
        let mut lock = acquire_lock!(self.transaction_query_batch_response);
        *lock = response;
    }

    pub fn set_utxo_query_response(&self, response: UtxoQueryResponses) {
        let mut lock = acquire_lock!(self.utxo_query_response);
        *lock = response;
    }

    pub fn set_query_deleted_response(&self, response: QueryDeletedResponse) {
        let mut lock = acquire_lock!(self.query_deleted_response);
        *lock = response;
    }

    pub fn set_response_delay(&self, delay: Option<Duration>) {
        let mut lock = acquire_lock!(self.response_delay);
        *lock = delay;
    }

    pub fn set_rpc_status_error(&self, rpc_status: Option<RpcStatus>) {
        let mut lock = acquire_lock!(self.rpc_status_error);
        *lock = rpc_status;
    }

    pub fn set_is_synced(&self, synced: bool) {
        let mut lock = acquire_lock!(self.synced);
        *lock = synced;
    }

    /// This method sets the contents of the UTXO set against which the queries will be made
    pub fn set_utxos(&self, utxos: Vec<TransactionOutput>) {
        let mut lock = acquire_lock!(self.utxos);
        *lock = utxos;
    }

    /// This method sets the contents of the UTXO set against which the queries will be made
    pub fn set_blocks(&self, blocks: HashMap<u64, BlockHeader>) {
        let mut lock = acquire_lock!(self.blocks);
        *lock = blocks;
    }

    pub fn set_utxos_by_block(&self, utxos_by_block: Vec<UtxosByBlock>) {
        let mut lock = acquire_lock!(self.utxos_by_block);
        *lock = utxos_by_block;
    }

    /// This channel will used to control which height a sync stream will return to from the testing client
    pub fn set_utxos_by_block_trigger_channel(&self, channel: mpsc::Receiver<usize>) {
        let mut lock = acquire_lock!(self.sync_utxos_by_block_trigger_channel);
        *lock = Some(channel);
    }

    pub fn take_utxo_query_calls(&self) -> Vec<Vec<Vec<u8>>> {
        acquire_lock!(self.utxo_query_calls).drain(..).collect()
    }

    pub fn pop_utxo_query_call(&self) -> Option<Vec<Vec<u8>>> {
        acquire_lock!(self.utxo_query_calls).pop()
    }

    pub fn take_query_deleted_calls(&self) -> Vec<QueryDeletedRequest> {
        acquire_lock!(self.query_deleted_calls).drain(..).collect()
    }

    pub fn pop_query_deleted_call(&self) -> Option<QueryDeletedRequest> {
        acquire_lock!(self.query_deleted_calls).pop()
    }

    pub fn take_submit_transaction_calls(&self) -> Vec<Transaction> {
        acquire_lock!(self.submit_transaction_calls).drain(..).collect()
    }

    pub fn pop_submit_transaction_call(&self) -> Option<Transaction> {
        acquire_lock!(self.submit_transaction_calls).pop()
    }

    pub fn take_transaction_query_calls(&self) -> Vec<Signature> {
        acquire_lock!(self.transaction_query_calls).drain(..).collect()
    }

    pub fn pop_transaction_query_call(&self) -> Option<Signature> {
        acquire_lock!(self.transaction_query_calls).pop()
    }

    pub fn take_transaction_batch_query_calls(&self) -> Vec<Vec<Signature>> {
        acquire_lock!(self.transaction_batch_query_calls).drain(..).collect()
    }

    pub fn pop_transaction_batch_query_call(&self) -> Option<Vec<Signature>> {
        acquire_lock!(self.transaction_batch_query_calls).pop()
    }

    pub fn take_transaction_fetch_utxo_calls(&self) -> Vec<Vec<Vec<u8>>> {
        acquire_lock!(self.fetch_utxos_calls).drain(..).collect()
    }

    pub fn pop_transaction_fetch_utxo_call(&self) -> Option<Vec<Vec<u8>>> {
        acquire_lock!(self.fetch_utxos_calls).pop()
    }

    pub fn take_get_header_by_height_calls(&self) -> Vec<u64> {
        acquire_lock!(self.get_header_by_height_calls).drain(..).collect()
    }

    pub fn pop_get_header_by_height_calls(&self) -> Option<u64> {
        acquire_lock!(self.get_header_by_height_calls).pop()
    }

    pub fn pop_get_height_at_time_calls(&self) -> Option<u64> {
        acquire_lock!(self.get_height_at_time_calls).pop()
    }

    pub fn take_get_height_at_time_calls(&self) -> Vec<u64> {
        acquire_lock!(self.get_height_at_time_calls).drain(..).collect()
    }

    pub fn take_sync_utxos_by_block_calls(&self) -> Vec<(HashOutput, HashOutput)> {
        acquire_lock!(self.sync_utxo_by_block_calls).drain(..).collect()
    }

    pub fn pop_sync_utxos_by_block_calls(&self) -> Option<(HashOutput, HashOutput)> {
        acquire_lock!(self.sync_utxo_by_block_calls).pop()
    }

    pub async fn wait_pop_sync_utxos_by_block_calls(
        &self,
        num_calls: usize,
        timeout: Duration,
    ) -> Result<Vec<(HashOutput, HashOutput)>, String> {
        let now = Instant::now();
        let mut count = 0usize;
        while now.elapsed() < timeout {
            let mut lock = acquire_lock!(self.sync_utxo_by_block_calls);
            count = (*lock).len();
            if (*lock).len() >= num_calls {
                return Ok((*lock).drain(..num_calls).collect());
            }
            drop(lock);
            sleep(Duration::from_millis(100)).await;
        }
        Err(format!(
            "Did not receive enough calls within the timeout period, received {}, expected {}.",
            count, num_calls
        ))
    }

    pub async fn wait_pop_get_header_by_height_calls(
        &self,
        num_calls: usize,
        timeout: Duration,
    ) -> Result<Vec<u64>, String> {
        let now = Instant::now();
        let mut count = 0usize;
        while now.elapsed() < timeout {
            let mut lock = acquire_lock!(self.get_header_by_height_calls);
            count = (*lock).len();
            if (*lock).len() >= num_calls {
                return Ok((*lock).drain(..num_calls).collect());
            }
            drop(lock);
            sleep(Duration::from_millis(100)).await;
        }
        Err(format!(
            "Did not receive enough calls within the timeout period, received {}, expected {}.",
            count, num_calls
        ))
    }

    pub async fn wait_pop_get_height_at_time(&self, num_calls: usize, timeout: Duration) -> Result<Vec<u64>, String> {
        let now = Instant::now();
        let mut count = 0usize;
        while now.elapsed() < timeout {
            let mut lock = acquire_lock!(self.get_height_at_time_calls);
            count = (*lock).len();
            if (*lock).len() >= num_calls {
                return Ok((*lock).drain(..num_calls).collect());
            }
            drop(lock);
            sleep(Duration::from_millis(100)).await;
        }
        Err(format!(
            "Did not receive enough calls within the timeout period, received {}, expected {}.",
            count, num_calls
        ))
    }

    pub async fn wait_pop_utxo_query_calls(
        &self,
        num_calls: usize,
        timeout: Duration,
    ) -> Result<Vec<Vec<Vec<u8>>>, String> {
        let now = Instant::now();
        let mut count = 0usize;
        while now.elapsed() < timeout {
            let mut lock = acquire_lock!(self.utxo_query_calls);
            count = (*lock).len();
            if (*lock).len() >= num_calls {
                return Ok((*lock).drain(..num_calls).collect());
            }
            drop(lock);
            sleep(Duration::from_millis(100)).await;
        }
        Err(format!(
            "Did not receive enough calls within the timeout period, received {}, expected {}.",
            count, num_calls
        ))
    }

    pub async fn wait_pop_transaction_query_calls(
        &self,
        num_calls: usize,
        timeout: Duration,
    ) -> Result<Vec<Signature>, String> {
        let now = Instant::now();
        let mut count = 0usize;
        while now.elapsed() < timeout {
            let mut lock = acquire_lock!(self.transaction_query_calls);
            count = (*lock).len();
            if (*lock).len() >= num_calls {
                return Ok((*lock).drain(..num_calls).collect());
            }
            drop(lock);
            sleep(Duration::from_millis(100)).await;
        }
        Err(format!(
            "Did not receive enough calls within the timeout period, received {}, expected {}.",
            count, num_calls
        ))
    }

    pub async fn wait_pop_transaction_batch_query_calls(
        &self,
        num_calls: usize,
        timeout: Duration,
    ) -> Result<Vec<Vec<Signature>>, String> {
        let now = Instant::now();
        let mut count = 0usize;
        while now.elapsed() < timeout {
            let mut lock = acquire_lock!(self.transaction_batch_query_calls);
            count = (*lock).len();
            if (*lock).len() >= num_calls {
                return Ok((*lock).drain(..num_calls).collect());
            }
            drop(lock);
            sleep(Duration::from_millis(100)).await;
        }
        Err(format!(
            "Did not receive enough calls within the timeout period, received {}, expected {}.",
            count, num_calls
        ))
    }

    pub async fn wait_pop_submit_transaction_calls(
        &self,
        num_calls: usize,
        timeout: Duration,
    ) -> Result<Vec<Transaction>, String> {
        let now = Instant::now();
        let mut count = 0usize;
        while now.elapsed() < timeout {
            let mut lock = acquire_lock!(self.submit_transaction_calls);
            count = (*lock).len();
            if (*lock).len() >= num_calls {
                return Ok((*lock).drain(..num_calls).collect());
            }
            drop(lock);
            sleep(Duration::from_millis(100)).await;
        }
        Err(format!(
            "Did not receive enough calls within the timeout period, received {}, expected {}.",
            count, num_calls
        ))
    }

    pub async fn wait_pop_fetch_utxos_calls(
        &self,
        num_calls: usize,
        timeout: Duration,
    ) -> Result<Vec<Vec<Vec<u8>>>, String> {
        let now = Instant::now();
        while now.elapsed() < timeout {
            let mut lock = acquire_lock!(self.fetch_utxos_calls);
            if (*lock).len() >= num_calls {
                return Ok((*lock).drain(..num_calls).collect());
            }
            drop(lock);
            sleep(Duration::from_millis(100)).await;
        }
        Err("Did not receive enough calls within the timeout period".to_string())
    }

    pub async fn wait_pop_query_deleted(
        &self,
        num_calls: usize,
        timeout: Duration,
    ) -> Result<Vec<QueryDeletedRequest>, String> {
        let now = Instant::now();
        while now.elapsed() < timeout {
            let mut lock = acquire_lock!(self.query_deleted_calls);
            if (*lock).len() >= num_calls {
                return Ok((*lock).drain(..num_calls).collect());
            }
            drop(lock);
            sleep(Duration::from_millis(100)).await;
        }
        Err("Did not receive enough calls within the timeout period".to_string())
    }
}

impl Default for BaseNodeWalletRpcMockState {
    fn default() -> Self {
        Self::new()
    }
}

pub struct BaseNodeWalletRpcMockService {
    state: BaseNodeWalletRpcMockState,
}

impl BaseNodeWalletRpcMockService {
    pub fn new() -> Self {
        Self {
            state: BaseNodeWalletRpcMockState::new(),
        }
    }

    pub fn get_state(&self) -> BaseNodeWalletRpcMockState {
        self.state.clone()
    }
}

impl Default for BaseNodeWalletRpcMockService {
    fn default() -> Self {
        Self::new()
    }
}

#[tari_comms::async_trait]
impl BaseNodeWalletService for BaseNodeWalletRpcMockService {
    async fn submit_transaction(
        &self,
        request: Request<TransactionProto>,
    ) -> Result<Response<TxSubmissionResponseProto>, RpcStatus> {
        let delay_lock = *acquire_lock!(self.state.response_delay);
        if let Some(delay) = delay_lock {
            sleep(delay).await;
        }

        let message = request.into_message();
        let transaction =
            Transaction::try_from(message).map_err(|_| RpcStatus::bad_request("Transaction was invalid"))?;
        log::info!("Submit Transaction call received: {}", transaction);

        let mut submit_transaction_calls_lock = acquire_lock!(self.state.submit_transaction_calls);
        (*submit_transaction_calls_lock).push(transaction);

        let status_lock = acquire_lock!(self.state.rpc_status_error);
        if let Some(status) = (*status_lock).clone() {
            return Err(status);
        }

        let submit_transaction_response_lock = acquire_lock!(self.state.submit_transaction_response);
        Ok(Response::new(submit_transaction_response_lock.clone().into()))
    }

    async fn transaction_query(
        &self,
        request: Request<SignatureProto>,
    ) -> Result<Response<TxQueryResponseProto>, RpcStatus> {
        let delay_lock = *acquire_lock!(self.state.response_delay);
        if let Some(delay) = delay_lock {
            sleep(delay).await;
        }

        let message = request.into_message();
        let signature = Signature::try_from(message).map_err(|_| RpcStatus::bad_request("Signature was invalid"))?;
        log::info!("Transaction Query call received: {:?}", signature);

        let mut transaction_query_calls_lock = acquire_lock!(self.state.transaction_query_calls);
        (*transaction_query_calls_lock).push(signature);

        let status_lock = acquire_lock!(self.state.rpc_status_error);
        if let Some(status) = (*status_lock).clone() {
            return Err(status);
        }

        let transaction_query_response_lock = acquire_lock!(self.state.transaction_query_response);

        Ok(Response::new(transaction_query_response_lock.clone().into()))
    }

    async fn transaction_batch_query(
        &self,
        request: Request<SignaturesProto>,
    ) -> Result<Response<TxQueryBatchResponsesProto>, RpcStatus> {
        let delay_lock = *acquire_lock!(self.state.response_delay);
        if let Some(delay) = delay_lock {
            sleep(delay).await;
        }

        let message = request.into_message();
        let mut signatures = Vec::new();
        for s in message.sigs {
            let signature = Signature::try_from(s).map_err(|_| RpcStatus::bad_request("Signature was invalid"))?;
            signatures.push(signature);
        }
        log::info!("Transaction Batch Query call received: {:?}", signatures);

        let mut transaction_query_calls_lock = acquire_lock!(self.state.transaction_batch_query_calls);
        (*transaction_query_calls_lock).push(signatures);

        let status_lock = acquire_lock!(self.state.rpc_status_error);
        if let Some(status) = (*status_lock).clone() {
            return Err(status);
        }

        let transaction_query_response_lock = acquire_lock!(self.state.transaction_query_batch_response);

        let mut response = transaction_query_response_lock.clone();

        let sync_lock = acquire_lock!(self.state.synced);
        response.is_synced = *sync_lock;
        Ok(Response::new(response))
    }

    async fn fetch_matching_utxos(
        &self,
        request: Request<FetchMatchingUtxos>,
    ) -> Result<Response<FetchUtxosResponse>, RpcStatus> {
        let delay_lock = *acquire_lock!(self.state.response_delay);
        if let Some(delay) = delay_lock {
            sleep(delay).await;
        }

        let message = request.into_message();

        let mut result = Vec::new();
        let utxo_lock = acquire_lock!(self.state.utxos);
        let utxos = (*utxo_lock).clone();

        let mut fetch_utxos_calls = acquire_lock!(self.state.fetch_utxos_calls);
        (*fetch_utxos_calls).push(message.output_hashes.clone());

        for hash in message.output_hashes.iter() {
            if let Some(output) = utxos.iter().find(|o| &o.hash() == hash) {
                result.push(TransactionOutputProto::from(output.clone()));
            }
        }

        let status_lock = acquire_lock!(self.state.rpc_status_error);
        if let Some(status) = (*status_lock).clone() {
            return Err(status);
        }

        let sync_lock = acquire_lock!(self.state.synced);
        Ok(Response::new(FetchUtxosResponse {
            outputs: result,
            is_synced: *sync_lock,
        }))
    }

    async fn get_tip_info(&self, _request: Request<()>) -> Result<Response<TipInfoResponse>, RpcStatus> {
        let delay_lock = *acquire_lock!(self.state.response_delay);
        if let Some(delay) = delay_lock {
            sleep(delay).await;
        }

        log::info!("Get tip info call received");

        let status_lock = acquire_lock!(self.state.rpc_status_error);
        if let Some(status) = (*status_lock).clone() {
            return Err(status);
        }

        let tip_info_response_lock = acquire_lock!(self.state.tip_info_response);

        Ok(Response::new(tip_info_response_lock.clone()))
    }

    async fn get_header(&self, _: Request<u64>) -> Result<Response<proto::core::BlockHeader>, RpcStatus> {
        let lock = acquire_lock!(self.state.get_header_response);
        let resp = lock
            .as_ref()
            .cloned()
            .ok_or_else(|| RpcStatus::not_found("get_header_response set to None"))?;
        Ok(Response::new(resp.into()))
    }

    async fn utxo_query(&self, request: Request<UtxoQueryRequest>) -> Result<Response<UtxoQueryResponses>, RpcStatus> {
        let message = request.into_message();

        let mut utxo_query_lock = acquire_lock!(self.state.utxo_query_calls);
        (*utxo_query_lock).push(message.output_hashes);

        let lock = acquire_lock!(self.state.utxo_query_response);
        Ok(Response::new(lock.clone()))
    }

    async fn query_deleted(
        &self,
        request: Request<QueryDeletedRequest>,
    ) -> Result<Response<QueryDeletedResponse>, RpcStatus> {
        let message = request.into_message();

        let mut query_deleted_lock = acquire_lock!(self.state.query_deleted_calls);
        (*query_deleted_lock).push(message);

        let lock = acquire_lock!(self.state.query_deleted_response);
        Ok(Response::new(lock.clone()))
    }

    async fn get_header_by_height(
        &self,
        request: Request<u64>,
    ) -> Result<Response<proto::core::BlockHeader>, RpcStatus> {
        let height = request.into_message();

        let mut header_by_height_lock = acquire_lock!(self.state.get_header_by_height_calls);
        (*header_by_height_lock).push(height);

        let block_lock = acquire_lock!(self.state.blocks);

        let header = (*block_lock).get(&height).cloned();

        if let Some(h) = header {
            Ok(Response::new(h.into()))
        } else {
            Err(RpcStatus::not_found("Header not found"))
        }
    }

    async fn get_height_at_time(&self, request: Request<u64>) -> Result<Response<u64>, RpcStatus> {
        let time = request.into_message();

        let mut height_at_time_lock = acquire_lock!(self.state.get_height_at_time_calls);
        (*height_at_time_lock).push(time);

        let block_lock = acquire_lock!(self.state.blocks);

        let mut headers = (*block_lock).values().cloned().collect::<Vec<BlockHeader>>();
        headers.sort_by(|a, b| b.height.cmp(&a.height));

        let mut found_height = 0;
        for h in headers.iter() {
            if h.timestamp.as_u64() < time {
                found_height = h.height;
                break;
            }
        }
        if found_height == 0 {
            found_height = headers[0].height;
        }
        Ok(Response::new(found_height))
    }

    async fn sync_utxos_by_block(
        &self,
        request: Request<SyncUtxosByBlockRequest>,
    ) -> Result<Streaming<SyncUtxosByBlockResponse>, RpcStatus> {
        let SyncUtxosByBlockRequest {
            start_header_hash,
            end_header_hash,
        } = request.into_message();

        let mut sync_utxo_by_block_lock = acquire_lock!(self.state.sync_utxo_by_block_calls);
        (*sync_utxo_by_block_lock).push((start_header_hash.clone(), end_header_hash.clone()));

        let block_lock = acquire_lock!(self.state.utxos_by_block);
        let mut blocks = (*block_lock).clone();
        blocks.sort_by(|a, b| a.height.cmp(&b.height));

        let start_index = blocks.iter().position(|b| b.header_hash == start_header_hash);
        let end_index = blocks.iter().position(|b| b.header_hash == end_header_hash);

        let mut channel_lock = acquire_lock!(self.state.sync_utxos_by_block_trigger_channel);
        let trigger_channel_option = (*channel_lock).take();

        if let (Some(start), Some(end)) = (start_index, end_index) {
            let (tx, rx) = mpsc::channel(200);
            let task = async move {
                if let Some(mut trigger_channel) = trigger_channel_option {
                    let mut current_block = start;
                    while let Some(trigger_block) = trigger_channel.recv().await {
                        if trigger_block < current_block {
                            // This is a testing harness so just panic if used incorrectly.
                            panic!("Trigger block cannot be before current starting block");
                        }
                        for b in blocks
                            .clone()
                            .into_iter()
                            .skip(current_block)
                            .take(min(trigger_block, end) - current_block + 1)
                        {
                            let item = SyncUtxosByBlockResponse {
                                outputs: b.utxos.clone().into_iter().map(|o| o.into()).collect(),
                                height: b.height,
                                header_hash: b.header_hash.clone(),
                            };
                            tx.send(Ok(item)).await.unwrap();
                        }
                        if trigger_block >= end {
                            break;
                        }
                        current_block = trigger_block + 1;
                    }
                } else {
                    for b in blocks.into_iter().skip(start).take(end - start + 1) {
                        let item = SyncUtxosByBlockResponse {
                            outputs: b.utxos.clone().into_iter().map(|o| o.into()).collect(),
                            height: b.height,
                            header_hash: b.header_hash.clone(),
                        };
                        tx.send(Ok(item)).await.unwrap();
                    }
                }
            };

            tokio::spawn(task);

            Ok(Streaming::new(rx))
        } else {
            Err(RpcStatus::not_found("Headers not found"))
        }
    }
}

#[derive(Clone, Debug)]
pub struct UtxosByBlock {
    pub height: u64,
    pub header_hash: Vec<u8>,
    pub utxos: Vec<TransactionOutput>,
}

#[cfg(test)]
mod test {
    use std::convert::{TryFrom, TryInto};

    use tari_common_types::types::BlindingFactor;
    use tari_comms::{
        peer_manager::PeerFeatures,
        protocol::rpc::{mock::MockRpcServer, NamedProtocolService},
        test_utils::node_identity::build_node_identity,
    };
    use tari_core::{
        base_node::{
            proto::wallet_rpc::{TxSubmissionRejectionReason, TxSubmissionResponse},
            rpc::{BaseNodeWalletRpcClient, BaseNodeWalletRpcServer},
        },
        proto::base_node::{ChainMetadata, TipInfoResponse},
        transactions::transaction_components::Transaction,
    };
    use tokio::time::Duration;

    use crate::support::comms_rpc::BaseNodeWalletRpcMockService;

    #[tokio::test]
    async fn test_wallet_rpc_mock() {
        let server_node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
        let client_node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);

        let service = BaseNodeWalletRpcMockService::new();
        let service_state = service.get_state();

        let server = BaseNodeWalletRpcServer::new(service);
        let protocol_name = server.as_protocol_name();

        let mut mock_server = MockRpcServer::new(server, server_node_identity.clone());

        mock_server.serve();

        let mut connection = mock_server
            .create_connection(client_node_identity.to_peer(), protocol_name.into())
            .await;

        let mut client = connection
            .connect_rpc_using_builder(BaseNodeWalletRpcClient::builder().with_deadline(Duration::from_secs(5)))
            .await
            .unwrap();

        assert!(service_state
            .wait_pop_submit_transaction_calls(1, Duration::from_millis(300))
            .await
            .is_err());

        service_state.set_submit_transaction_response(TxSubmissionResponse {
            accepted: false,
            rejection_reason: TxSubmissionRejectionReason::TimeLocked,
            is_synced: true,
        });

        let tx = Transaction::new(
            vec![],
            vec![],
            vec![],
            BlindingFactor::default(),
            BlindingFactor::default(),
        );

        let resp =
            TxSubmissionResponse::try_from(client.submit_transaction(tx.try_into().unwrap()).await.unwrap()).unwrap();
        assert_eq!(resp.rejection_reason, TxSubmissionRejectionReason::TimeLocked);

        let calls = service_state
            .wait_pop_submit_transaction_calls(1, Duration::from_millis(300))
            .await
            .unwrap();
        assert_eq!(calls.len(), 1);

        let chain_metadata = ChainMetadata {
            height_of_longest_chain: Some(444),
            best_block: Some(Vec::new()),
            accumulated_difficulty: Vec::new(),
            pruned_height: 0,
        };
        service_state.set_tip_info_response(TipInfoResponse {
            metadata: Some(chain_metadata),
            is_synced: false,
        });

        let resp = client.get_tip_info().await.unwrap();
        assert!(!resp.is_synced);
        assert_eq!(resp.metadata.unwrap().height_of_longest_chain(), 444);
    }
}
