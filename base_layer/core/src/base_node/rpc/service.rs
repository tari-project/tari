//  Copyright 2020, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use crate::{
    base_node::{rpc::BaseNodeWalletService, state_machine_service::states::StateInfo, StateMachineHandle},
    chain_storage::{async_db::AsyncBlockchainDb, BlockchainBackend},
    mempool::{service::MempoolHandle, TxStorageResponse},
    proto::{
        base_node::{
            ChainMetadata,
            FetchMatchingUtxos,
            FetchUtxosResponse,
            ReorgInfo,
            Signatures as SignaturesProto,
            TipInfoResponse,
            TxLocation,
            TxQueryBatchResponse,
            TxQueryBatchResponses,
            TxQueryResponse,
            TxSubmissionRejectionReason,
            TxSubmissionResponse,
        },
        types::{Signature as SignatureProto, Transaction as TransactionProto},
    },
    transactions::{transaction::Transaction, types::Signature},
};
use std::convert::TryFrom;
use tari_comms::protocol::rpc::{Request, Response, RpcStatus};

const LOG_TARGET: &str = "c::base_node::rpc";

pub struct BaseNodeWalletRpcService<B> {
    db: AsyncBlockchainDb<B>,
    mempool: MempoolHandle,
    state_machine: StateMachineHandle,
}

impl<B: BlockchainBackend + 'static> BaseNodeWalletRpcService<B> {
    pub fn new(db: AsyncBlockchainDb<B>, mempool: MempoolHandle, state_machine: StateMachineHandle) -> Self {
        Self {
            db,
            mempool,
            state_machine,
        }
    }

    #[inline]
    fn db(&self) -> AsyncBlockchainDb<B> {
        self.db.clone()
    }

    #[inline]
    pub fn mempool(&self) -> MempoolHandle {
        self.mempool.clone()
    }

    #[inline]
    pub fn state_machine(&self) -> StateMachineHandle {
        self.state_machine.clone()
    }

    async fn fetch_kernel(&self, signature: Signature) -> Result<TxQueryResponse, RpcStatus> {
        let db = self.db();
        let chain_metadata = db
            .get_chain_metadata()
            .await
            .map_err(RpcStatus::log_internal_error(LOG_TARGET))?;
        match db
            .fetch_kernel_by_excess_sig(signature.clone())
            .await
            .map_err(RpcStatus::log_internal_error(LOG_TARGET))?
        {
            None => (),
            Some((_, block_hash)) => {
                match db
                    .fetch_header_by_block_hash(block_hash.clone())
                    .await
                    .map_err(RpcStatus::log_internal_error(LOG_TARGET))?
                {
                    None => (),
                    Some(header) => {
                        let confirmations = chain_metadata.height_of_longest_chain().saturating_sub(header.height);
                        let response = TxQueryResponse {
                            location: TxLocation::Mined as i32,
                            block_hash: Some(block_hash),
                            confirmations,
                            is_synced: true,
                            height_of_longest_chain: chain_metadata.height_of_longest_chain(),
                        };
                        return Ok(response);
                    },
                }
            },
        };

        // If not in a block then check the mempool
        let mut mempool = self.mempool();
        let mempool_response = match mempool
            .get_tx_state_by_excess_sig(signature.clone())
            .await
            .map_err(RpcStatus::log_internal_error(LOG_TARGET))?
        {
            TxStorageResponse::UnconfirmedPool => TxQueryResponse {
                location: TxLocation::InMempool as i32,
                block_hash: None,
                confirmations: 0,
                is_synced: true,
                height_of_longest_chain: chain_metadata.height_of_longest_chain(),
            },
            TxStorageResponse::ReorgPool |
            TxStorageResponse::NotStoredOrphan |
            TxStorageResponse::NotStoredTimeLocked |
            TxStorageResponse::NotStoredAlreadySpent |
            TxStorageResponse::NotStored => TxQueryResponse {
                location: TxLocation::NotStored as i32,
                block_hash: None,
                confirmations: 0,
                is_synced: true,
                height_of_longest_chain: chain_metadata.height_of_longest_chain(),
            },
        };
        Ok(mempool_response)
    }
}

#[tari_comms::async_trait]
impl<B: BlockchainBackend + 'static> BaseNodeWalletService for BaseNodeWalletRpcService<B> {
    async fn submit_transaction(
        &self,
        request: Request<TransactionProto>,
    ) -> Result<Response<TxSubmissionResponse>, RpcStatus>
    {
        let message = request.into_message();
        let transaction =
            Transaction::try_from(message).map_err(|_| RpcStatus::bad_request("Transaction was invalid"))?;
        let mut mempool = self.mempool();
        let state_machine = self.state_machine();

        // Determine if we are synced
        let status_watch = state_machine.get_status_info_watch();
        let is_synced = match (*status_watch.borrow()).state_info {
            StateInfo::Listening(li) => li.is_synced(),
            _ => false,
        };

        let response = match mempool
            .submit_transaction(transaction.clone())
            .await
            .map_err(RpcStatus::log_internal_error(LOG_TARGET))?
        {
            TxStorageResponse::UnconfirmedPool => TxSubmissionResponse {
                accepted: true,
                rejection_reason: TxSubmissionRejectionReason::None.into(),
                is_synced,
            },

            TxStorageResponse::NotStoredOrphan => TxSubmissionResponse {
                accepted: false,
                rejection_reason: TxSubmissionRejectionReason::Orphan.into(),
                is_synced,
            },
            TxStorageResponse::NotStoredTimeLocked => TxSubmissionResponse {
                accepted: false,
                rejection_reason: TxSubmissionRejectionReason::TimeLocked.into(),
                is_synced,
            },

            TxStorageResponse::NotStored => TxSubmissionResponse {
                accepted: false,
                rejection_reason: TxSubmissionRejectionReason::ValidationFailed.into(),
                is_synced,
            },
            TxStorageResponse::NotStoredAlreadySpent | TxStorageResponse::ReorgPool => {
                // Is this transaction a double spend or has this transaction been mined?
                match transaction.first_kernel_excess_sig() {
                    None => TxSubmissionResponse {
                        accepted: false,
                        rejection_reason: TxSubmissionRejectionReason::DoubleSpend.into(),
                        is_synced,
                    },
                    Some(s) => {
                        // Check to see if the kernel exists in the blockchain db in which case this exact transaction
                        // already exists in the chain, otherwise it is a double spend
                        let db = self.db();
                        match db
                            .fetch_kernel_by_excess_sig(s.clone())
                            .await
                            .map_err(RpcStatus::log_internal_error(LOG_TARGET))?
                        {
                            None => TxSubmissionResponse {
                                accepted: false,
                                rejection_reason: TxSubmissionRejectionReason::DoubleSpend.into(),
                                is_synced,
                            },
                            Some(_) => TxSubmissionResponse {
                                accepted: false,
                                rejection_reason: TxSubmissionRejectionReason::AlreadyMined.into(),
                                is_synced,
                            },
                        }
                    },
                }
            },
        };
        Ok(Response::new(response))
    }

    async fn transaction_query(
        &self,
        request: Request<SignatureProto>,
    ) -> Result<Response<TxQueryResponse>, RpcStatus>
    {
        let state_machine = self.state_machine();

        // Determine if we are synced
        let status_watch = state_machine.get_status_info_watch();
        let is_synced = match (*status_watch.borrow()).state_info {
            StateInfo::Listening(li) => li.is_synced(),
            _ => false,
        };

        let message = request.into_message();
        let signature = Signature::try_from(message).map_err(|_| RpcStatus::bad_request("Signature was invalid"))?;

        let mut response = self.fetch_kernel(signature).await?;
        response.is_synced = is_synced;
        Ok(Response::new(response))
    }

    async fn transaction_batch_query(
        &self,
        request: Request<SignaturesProto>,
    ) -> Result<Response<TxQueryBatchResponses>, RpcStatus>
    {
        let state_machine = self.state_machine();

        // Determine if we are synced
        let status_watch = state_machine.get_status_info_watch();
        let is_synced = match (*status_watch.borrow()).state_info {
            StateInfo::Listening(li) => li.is_synced(),
            _ => false,
        };

        let message = request.into_message();

        let mut responses: Vec<TxQueryBatchResponse> = Vec::new();

        for sig in message.sigs {
            let signature = Signature::try_from(sig).map_err(|_| RpcStatus::bad_request("Signature was invalid"))?;
            let response: TxQueryResponse = self.fetch_kernel(signature.clone()).await?;
            responses.push(TxQueryBatchResponse {
                signature: Some(SignatureProto::from(signature)),
                location: response.location,
                block_hash: response.block_hash,
                confirmations: response.confirmations,
            });
        }
        Ok(Response::new(TxQueryBatchResponses { responses, is_synced }))
    }

    async fn fetch_matching_utxos(
        &self,
        request: Request<FetchMatchingUtxos>,
    ) -> Result<Response<FetchUtxosResponse>, RpcStatus>
    {
        let message = request.into_message();

        let state_machine = self.state_machine();
        // Determine if we are synced
        let status_watch = state_machine.get_status_info_watch();
        let is_synced = match (*status_watch.borrow()).state_info {
            StateInfo::Listening(li) => li.is_synced(),
            _ => false,
        };

        let db = self.db();
        let mut res = Vec::with_capacity(message.output_hashes.len());
        for item in db
            .fetch_utxos(message.output_hashes, None)
            .await
            .map_err(RpcStatus::log_internal_error(LOG_TARGET))?
        {
            if let Some((output, spent)) = item {
                if !spent {
                    res.push(output);
                }
            }
        }

        Ok(Response::new(FetchUtxosResponse {
            outputs: res.into_iter().map(Into::into).collect(),
            is_synced,
        }))
    }

    async fn get_tip_info(&self, _request: Request<()>) -> Result<Response<TipInfoResponse>, RpcStatus> {
        let state_machine = self.state_machine();
        let status_watch = state_machine.get_status_info_watch();
        let is_synced = match (*status_watch.borrow()).state_info {
            StateInfo::Listening(li) => li.is_synced(),
            _ => false,
        };

        let metadata = self
            .db
            .get_chain_metadata()
            .await
            .map_err(RpcStatus::log_internal_error(LOG_TARGET))?;

        let reorg_info = match self
            .db
            .fetch_reorg_info()
            .await
            .map_err(RpcStatus::log_internal_error(LOG_TARGET))?
        {
            None => None,
            Some(val) => Some(ReorgInfo {
                last_reorg_best_block: val.last_reorg_best_block.clone(),
                num_blocks_reorged: val.num_blocks_reorged,
                tip_height: val.tip_height,
            }),
        };

        Ok(Response::new(TipInfoResponse {
            metadata: Some(ChainMetadata {
                height_of_longest_chain: Some(metadata.height_of_longest_chain()),
                best_block: Some(metadata.best_block().to_vec()),
                pruning_horizon: metadata.pruning_horizon(),
                accumulated_difficulty: metadata.accumulated_difficulty().to_be_bytes().to_vec(),
                effective_pruned_height: metadata.pruned_height(),
            }),
            is_synced,
            reorg_info,
        }))
    }
}
