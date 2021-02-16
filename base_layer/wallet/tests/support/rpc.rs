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
    convert::TryFrom,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tari_comms::protocol::rpc::{Request, Response, RpcStatus};
use tari_core::{
    base_node::{
        proto::wallet_response::{TxLocation, TxQueryResponse, TxSubmissionRejectionReason, TxSubmissionResponse},
        rpc::BaseNodeWalletService,
    },
    proto::{
        base_node::{
            Signatures as SignaturesProto,
            TxQueryBatchResponse as TxQueryBatchResponseProto,
            TxQueryBatchResponses as TxQueryBatchResponsesProto,
            TxQueryResponse as TxQueryResponseProto,
            TxSubmissionResponse as TxSubmissionResponseProto,
        },
        types::{Signature as SignatureProto, Transaction as TransactionProto},
    },
    transactions::{transaction::Transaction, types::Signature},
};
use tokio::time::delay_for;

/// This macro unlocks a Mutex or RwLock. If the lock is
/// poisoned (i.e. panic while unlocked) the last value
/// before the panic is used.
macro_rules! acquire_lock {
    ($e:expr, $m:ident) => {
        match $e.$m() {
            Ok(lock) => lock,
            Err(poisoned) => {
                log::warn!(target: "wallet", "Lock has been POISONED and will be silently recovered");
                poisoned.into_inner()
            },
        }
    };
    ($e:expr) => {
        acquire_lock!($e, lock)
    };
}

#[derive(Clone)]
pub struct BaseNodeWalletRpcMockState {
    submit_transaction_calls: Arc<Mutex<Vec<Transaction>>>,
    transaction_query_calls: Arc<Mutex<Vec<Signature>>>,
    transaction_batch_query_calls: Arc<Mutex<Vec<Vec<Signature>>>>,
    submit_transaction_response: Arc<Mutex<TxSubmissionResponse>>,
    transaction_query_response: Arc<Mutex<TxQueryResponse>>,
    response_delay: Arc<Mutex<Option<Duration>>>,
    rpc_status_error: Arc<Mutex<Option<RpcStatus>>>,
    synced: Arc<Mutex<bool>>,
}

impl BaseNodeWalletRpcMockState {
    pub fn new() -> Self {
        Self {
            submit_transaction_calls: Arc::new(Mutex::new(Vec::new())),
            transaction_query_calls: Arc::new(Mutex::new(Vec::new())),
            transaction_batch_query_calls: Arc::new(Mutex::new(Vec::new())),
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
            })),
            response_delay: Arc::new(Mutex::new(None)),
            rpc_status_error: Arc::new(Mutex::new(None)),
            synced: Arc::new(Mutex::new(true)),
        }
    }

    pub fn set_submit_transaction_response(&self, response: TxSubmissionResponse) {
        let mut lock = acquire_lock!(self.submit_transaction_response);
        *lock = response;
    }

    pub fn set_transaction_query_response(&self, response: TxQueryResponse) {
        let mut lock = acquire_lock!(self.transaction_query_response);
        *lock = response;
    }

    pub fn set_response_delay(&mut self, delay: Option<Duration>) {
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

    pub async fn wait_pop_transaction_query_calls(
        &self,
        num_calls: usize,
        timeout: Duration,
    ) -> Result<Vec<Signature>, String>
    {
        let now = Instant::now();
        while now.elapsed() < timeout {
            let mut lock = acquire_lock!(self.transaction_query_calls);
            if (*lock).len() >= num_calls {
                return Ok((*lock).drain(..num_calls).collect());
            }
            drop(lock);
            delay_for(Duration::from_millis(100)).await;
        }
        Err("Did not receive enough calls within the timeout period".to_string())
    }

    pub async fn wait_pop_transaction_batch_query_calls(
        &self,
        num_calls: usize,
        timeout: Duration,
    ) -> Result<Vec<Vec<Signature>>, String>
    {
        let now = Instant::now();
        while now.elapsed() < timeout {
            let mut lock = acquire_lock!(self.transaction_batch_query_calls);
            if (*lock).len() >= num_calls {
                return Ok((*lock).drain(..num_calls).collect());
            }
            drop(lock);
            delay_for(Duration::from_millis(100)).await;
        }
        Err("Did not receive enough calls within the timeout period".to_string())
    }

    pub async fn wait_pop_submit_transaction_calls(
        &self,
        num_calls: usize,
        timeout: Duration,
    ) -> Result<Vec<Transaction>, String>
    {
        let now = Instant::now();
        while now.elapsed() < timeout {
            let mut lock = acquire_lock!(self.submit_transaction_calls);
            if (*lock).len() >= num_calls {
                return Ok((*lock).drain(..num_calls).collect());
            }
            drop(lock);
            delay_for(Duration::from_millis(100)).await;
        }
        Err("Did not receive enough calls within the timeout period".to_string())
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

#[tari_comms::async_trait]
impl BaseNodeWalletService for BaseNodeWalletRpcMockService {
    async fn submit_transaction(
        &self,
        request: Request<TransactionProto>,
    ) -> Result<Response<TxSubmissionResponseProto>, RpcStatus>
    {
        let delay_lock = (*acquire_lock!(self.state.response_delay)).clone();
        if let Some(delay) = delay_lock {
            delay_for(delay).await;
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
    ) -> Result<Response<TxQueryResponseProto>, RpcStatus>
    {
        let delay_lock = (*acquire_lock!(self.state.response_delay)).clone();
        if let Some(delay) = delay_lock {
            delay_for(delay).await;
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
    ) -> Result<Response<TxQueryBatchResponsesProto>, RpcStatus>
    {
        let delay_lock = (*acquire_lock!(self.state.response_delay)).clone();
        if let Some(delay) = delay_lock {
            delay_for(delay).await;
        }

        let message = request.into_message();
        let mut signatures = Vec::new();
        for s in message.sigs {
            let signature = Signature::try_from(s).map_err(|_| RpcStatus::bad_request("Signature was invalid"))?;
            signatures.push(signature);
        }
        log::info!("Transaction Batch Query call received: {:?}", signatures);

        let mut transaction_query_calls_lock = acquire_lock!(self.state.transaction_batch_query_calls);
        (*transaction_query_calls_lock).push(signatures.clone());

        let status_lock = acquire_lock!(self.state.rpc_status_error);
        if let Some(status) = (*status_lock).clone() {
            return Err(status);
        }

        let transaction_query_response_lock = acquire_lock!(self.state.transaction_query_response);
        let transaction_query_response = TxQueryResponseProto::from(transaction_query_response_lock.clone());
        let mut responses = Vec::new();
        for sig in signatures.iter() {
            let response = TxQueryBatchResponseProto {
                signature: Some(sig.clone().into()),
                location: transaction_query_response.location,
                block_hash: transaction_query_response.block_hash.clone(),
                confirmations: transaction_query_response.confirmations,
            };
            responses.push(response);
        }
        let sync_lock = acquire_lock!(self.state.synced);
        Ok(Response::new(TxQueryBatchResponsesProto {
            responses,
            is_synced: *sync_lock,
        }))
    }
}

#[cfg(test)]
mod test {
    use crate::support::rpc::BaseNodeWalletRpcMockService;
    use tari_comms::{
        peer_manager::PeerFeatures,
        protocol::rpc::{mock::MockRpcServer, NamedProtocolService},
        test_utils::node_identity::build_node_identity,
    };

    use std::convert::TryFrom;
    use tari_core::{
        base_node::{
            proto::wallet_response::{TxSubmissionRejectionReason, TxSubmissionResponse},
            rpc::{BaseNodeWalletRpcClient, BaseNodeWalletRpcServer},
        },
        transactions::{transaction::Transaction, types::BlindingFactor},
    };
    use tokio::time::Duration;

    #[tokio_macros::test]
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

        let tx = Transaction::new(vec![], vec![], vec![], BlindingFactor::default());

        let resp = TxSubmissionResponse::try_from(client.submit_transaction(tx.into()).await.unwrap()).unwrap();
        assert_eq!(resp.rejection_reason, TxSubmissionRejectionReason::TimeLocked);

        let calls = service_state
            .wait_pop_submit_transaction_calls(1, Duration::from_millis(300))
            .await
            .unwrap();
        assert_eq!(calls.len(), 1);
    }
}
