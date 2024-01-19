//  Copyright 2020, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that
// the  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the
// following  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED
// WARRANTIES,  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A
// PARTICULAR PURPOSE ARE  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY
// DIRECT, INDIRECT, INCIDENTAL,  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO,
// PROCUREMENT OF SUBSTITUTE GOODS OR  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
// CAUSED AND ON ANY THEORY OF LIABILITY,  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR
// OTHERWISE) ARISING IN ANY WAY OUT OF THE  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH
// DAMAGE.

use std::convert::{TryFrom, TryInto};

use log::*;
use tari_common_types::types::{FixedHash, Signature};
use tari_comms::protocol::rpc::{Request, Response, RpcStatus, RpcStatusResultExt, Streaming};
use tari_utilities::hex::Hex;
use tokio::sync::mpsc;

use crate::{
    base_node::{
        rpc::{sync_utxos_by_block_task::SyncUtxosByBlockTask, BaseNodeWalletService},
        state_machine_service::states::StateInfo,
        StateMachineHandle,
    },
    chain_storage::{async_db::AsyncBlockchainDb, BlockchainBackend},
    mempool::{service::MempoolHandle, TxStorageResponse},
    proto,
    proto::{
        base_node::{
            FetchMatchingUtxos,
            FetchUtxosResponse,
            GetMempoolFeePerGramStatsRequest,
            GetMempoolFeePerGramStatsResponse,
            QueryDeletedData,
            QueryDeletedRequest,
            QueryDeletedResponse,
            Signatures as SignaturesProto,
            SyncUtxosByBlockRequest,
            SyncUtxosByBlockResponse,
            TipInfoResponse,
            TxLocation,
            TxQueryBatchResponse,
            TxQueryBatchResponses,
            TxQueryResponse,
            TxSubmissionRejectionReason,
            TxSubmissionResponse,
            UtxoQueryRequest,
            UtxoQueryResponse,
            UtxoQueryResponses,
        },
        types::{Signature as SignatureProto, Transaction as TransactionProto},
    },
    transactions::transaction_components::Transaction,
};

const LOG_TARGET: &str = "c::base_node::rpc";
const MAX_QUERY_DELETED_HASHES: usize = 1000;

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
        let chain_metadata = db.get_chain_metadata().await.rpc_status_internal_error(LOG_TARGET)?;
        let state_machine = self.state_machine();

        // Determine if we are synced
        let status_watch = state_machine.get_status_info_watch();
        let is_synced = match (status_watch.borrow()).state_info {
            StateInfo::Listening(li) => li.is_synced(),
            _ => false,
        };
        match db
            .fetch_kernel_by_excess_sig(signature.clone())
            .await
            .rpc_status_internal_error(LOG_TARGET)?
        {
            None => (),
            Some((_, block_hash)) => {
                match db
                    .fetch_header_by_block_hash(block_hash)
                    .await
                    .rpc_status_internal_error(LOG_TARGET)?
                {
                    None => (),
                    Some(header) => {
                        let confirmations = chain_metadata.height_of_longest_chain().saturating_sub(header.height);
                        let response = TxQueryResponse {
                            location: TxLocation::Mined as i32,
                            block_hash: block_hash.to_vec(),
                            confirmations,
                            is_synced,
                            height_of_longest_chain: chain_metadata.height_of_longest_chain(),
                            mined_timestamp: header.timestamp.as_u64(),
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
            .rpc_status_internal_error(LOG_TARGET)?
        {
            TxStorageResponse::UnconfirmedPool => TxQueryResponse {
                location: TxLocation::InMempool as i32,
                block_hash: vec![],
                confirmations: 0,
                is_synced,
                height_of_longest_chain: chain_metadata.height_of_longest_chain(),
                mined_timestamp: 0,
            },
            TxStorageResponse::ReorgPool |
            TxStorageResponse::NotStoredOrphan |
            TxStorageResponse::NotStoredTimeLocked |
            TxStorageResponse::NotStoredAlreadySpent |
            TxStorageResponse::NotStoredConsensus |
            TxStorageResponse::NotStored |
            TxStorageResponse::NotStoredFeeTooLow |
            TxStorageResponse::NotStoredAlreadyMined => TxQueryResponse {
                location: TxLocation::NotStored as i32,
                block_hash: vec![],
                confirmations: 0,
                is_synced,
                height_of_longest_chain: chain_metadata.height_of_longest_chain(),
                mined_timestamp: 0,
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
    ) -> Result<Response<TxSubmissionResponse>, RpcStatus> {
        let message = request.into_message();
        let transaction =
            Transaction::try_from(message).map_err(|_| RpcStatus::bad_request("Transaction was invalid"))?;
        let mut mempool = self.mempool();
        let state_machine = self.state_machine();

        // Determine if we are synced
        let status_watch = state_machine.get_status_info_watch();
        let is_synced = match (status_watch.borrow()).state_info {
            StateInfo::Listening(li) => li.is_synced(),
            _ => false,
        };

        let response = match mempool
            .submit_transaction(transaction.clone())
            .await
            .rpc_status_internal_error(LOG_TARGET)?
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
            TxStorageResponse::NotStoredFeeTooLow => TxSubmissionResponse {
                accepted: false,
                rejection_reason: TxSubmissionRejectionReason::FeeTooLow.into(),
                is_synced,
            },
            TxStorageResponse::NotStoredTimeLocked => TxSubmissionResponse {
                accepted: false,
                rejection_reason: TxSubmissionRejectionReason::TimeLocked.into(),
                is_synced,
            },
            TxStorageResponse::NotStoredConsensus | TxStorageResponse::NotStored => TxSubmissionResponse {
                accepted: false,
                rejection_reason: TxSubmissionRejectionReason::ValidationFailed.into(),
                is_synced,
            },
            TxStorageResponse::NotStoredAlreadySpent |
            TxStorageResponse::ReorgPool |
            TxStorageResponse::NotStoredAlreadyMined => {
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
                            .rpc_status_internal_error(LOG_TARGET)?
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
    ) -> Result<Response<TxQueryResponse>, RpcStatus> {
        let state_machine = self.state_machine();

        // Determine if we are synced
        let status_watch = state_machine.get_status_info_watch();
        let is_synced = match status_watch.borrow().state_info {
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
    ) -> Result<Response<TxQueryBatchResponses>, RpcStatus> {
        let state_machine = self.state_machine();

        // Determine if we are synced
        let status_watch = state_machine.get_status_info_watch();
        let is_synced = match (status_watch.borrow()).state_info {
            StateInfo::Listening(li) => li.is_synced(),
            _ => false,
        };

        let message = request.into_message();

        let mut responses: Vec<TxQueryBatchResponse> = Vec::new();

        let metadata = self
            .db
            .get_chain_metadata()
            .await
            .rpc_status_internal_error(LOG_TARGET)?;

        for sig in message.sigs {
            let signature = Signature::try_from(sig).map_err(|_| RpcStatus::bad_request("Signature was invalid"))?;
            let response: TxQueryResponse = self.fetch_kernel(signature.clone()).await?;
            responses.push(TxQueryBatchResponse {
                signature: Some(SignatureProto::from(signature)),
                location: response.location,
                block_hash: response.block_hash,
                confirmations: response.confirmations,
                block_height: response.height_of_longest_chain.saturating_sub(response.confirmations),
                mined_timestamp: response.mined_timestamp,
            });
        }
        Ok(Response::new(TxQueryBatchResponses {
            responses,
            is_synced,
            tip_hash: metadata.best_block().to_vec(),
            height_of_longest_chain: metadata.height_of_longest_chain(),
            tip_mined_timestamp: metadata.timestamp(),
        }))
    }

    async fn fetch_matching_utxos(
        &self,
        request: Request<FetchMatchingUtxos>,
    ) -> Result<Response<FetchUtxosResponse>, RpcStatus> {
        let message = request.into_message();

        let state_machine = self.state_machine();
        // Determine if we are synced
        let status_watch = state_machine.get_status_info_watch();
        let is_synced = match (status_watch.borrow()).state_info {
            StateInfo::Listening(li) => li.is_synced(),
            _ => false,
        };

        let db = self.db();
        let mut res = Vec::with_capacity(message.output_hashes.len());
        let hashes: Vec<FixedHash> = message
            .output_hashes
            .into_iter()
            .map(|hash| hash.try_into().map_err(|_| "Malformed pruned hash".to_string()))
            .collect::<Result<_, _>>()
            .map_err(|_| RpcStatus::bad_request(&"Malformed block hash received".to_string()))?;
        let utxos = db
            .fetch_outputs_with_spend_status_at_tip(hashes)
            .await
            .rpc_status_internal_error(LOG_TARGET)?
            .into_iter()
            .flatten();
        for (output, spent) in utxos {
            if !spent {
                res.push(output);
            }
        }

        Ok(Response::new(FetchUtxosResponse {
            outputs: res
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>, String>>()
                .map_err(|err| RpcStatus::bad_request(&err))?,
            is_synced,
        }))
    }

    async fn utxo_query(&self, request: Request<UtxoQueryRequest>) -> Result<Response<UtxoQueryResponses>, RpcStatus> {
        let message = request.into_message();
        if message.output_hashes.is_empty() {
            return Err(RpcStatus::bad_request("Empty output hashes"));
        }
        const MAX_ALLOWED_QUERY_SIZE: usize = 512;
        if message.output_hashes.len() > MAX_ALLOWED_QUERY_SIZE {
            return Err(RpcStatus::bad_request(&format!(
                "Exceeded maximum allowed query hashes. Max: {}",
                MAX_ALLOWED_QUERY_SIZE
            )));
        }

        let db = self.db();

        debug!(
            target: LOG_TARGET,
            "Querying {} UTXO(s) for mined state",
            message.output_hashes.len(),
        );
        let hashes: Vec<FixedHash> = message
            .output_hashes
            .into_iter()
            .map(|hash| hash.try_into().map_err(|_| "Malformed pruned hash".to_string()))
            .collect::<Result<_, _>>()
            .map_err(|_| RpcStatus::bad_request(&"Malformed block hash received".to_string()))?;
        trace!(
            target: LOG_TARGET,
            "UTXO hashes queried from wallet: {:?}",
            hashes.iter().map(|h| h.to_hex()).collect::<Vec<String>>()
        );

        let mined_info_resp = db
            .fetch_outputs_mined_info(hashes)
            .await
            .rpc_status_internal_error(LOG_TARGET)?;

        let num_mined = mined_info_resp.iter().filter(|opt| opt.is_some()).count();
        debug!(
            target: LOG_TARGET,
            "Found {} mined and {} unmined UTXO(s)",
            num_mined,
            mined_info_resp.len() - num_mined
        );
        let metadata = self
            .db
            .get_chain_metadata()
            .await
            .rpc_status_internal_error(LOG_TARGET)?;

        Ok(Response::new(UtxoQueryResponses {
            best_block_height: metadata.height_of_longest_chain(),
            best_block_hash: metadata.best_block().to_vec(),
            responses: mined_info_resp
                .into_iter()
                .flatten()
                .map(|utxo| {
                    Ok(UtxoQueryResponse {
                        mined_at_height: utxo.mined_height,
                        mined_in_block: utxo.header_hash.to_vec(),
                        output_hash: utxo.output.hash().to_vec(),
                        output: match utxo.output.try_into() {
                            Ok(output) => Some(output),
                            Err(err) => {
                                return Err(err);
                            },
                        },
                        mined_timestamp: utxo.mined_timestamp,
                    })
                })
                .collect::<Result<Vec<_>, String>>()
                .map_err(|err| RpcStatus::bad_request(&err))?,
        }))
    }

    async fn query_deleted(
        &self,
        request: Request<QueryDeletedRequest>,
    ) -> Result<Response<QueryDeletedResponse>, RpcStatus> {
        let message = request.into_message();
        if message.hashes.len() > MAX_QUERY_DELETED_HASHES {
            return Err(RpcStatus::bad_request(
                &"Received more hashes than we allow".to_string(),
            ));
        }
        let chain_include_header = message.chain_must_include_header;
        if !chain_include_header.is_empty() {
            let hash = chain_include_header
                .try_into()
                .map_err(|_| RpcStatus::bad_request(&"Malformed block hash received".to_string()))?;
            if self
                .db
                .fetch_header_by_block_hash(hash)
                .await
                .rpc_status_internal_error(LOG_TARGET)?
                .is_none()
            {
                return Err(RpcStatus::not_found(
                    "Chain does not include header. It might have been reorged out",
                ));
            }
        }
        let hashes: Vec<FixedHash> = message
            .hashes
            .into_iter()
            .map(|hash| hash.try_into())
            .collect::<Result<_, _>>()
            .map_err(|_| RpcStatus::bad_request(&"Malformed utxo hash received".to_string()))?;
        let mut return_data = Vec::with_capacity(hashes.len());
        let utxos = self
            .db
            .fetch_outputs_mined_info(hashes.clone())
            .await
            .rpc_status_internal_error(LOG_TARGET)?;
        let txos = self
            .db
            .fetch_inputs_mined_info(hashes)
            .await
            .rpc_status_internal_error(LOG_TARGET)?;
        if utxos.len() != txos.len() {
            return Err(RpcStatus::general("database returned different inputs vs outputs"));
        }
        for (utxo, txo) in utxos.iter().zip(txos.iter()) {
            let mut data = match utxo {
                None => QueryDeletedData {
                    mined_at_height: 0,
                    block_mined_in: Vec::new(),
                    height_deleted_at: 0,
                    block_deleted_in: Vec::new(),
                },
                Some(u) => QueryDeletedData {
                    mined_at_height: u.mined_height,
                    block_mined_in: u.header_hash.to_vec(),
                    height_deleted_at: 0,
                    block_deleted_in: Vec::new(),
                },
            };
            if let Some(input) = txo {
                data.height_deleted_at = input.spent_height;
                data.block_deleted_in = input.header_hash.to_vec();
            };
            return_data.push(data);
        }
        let metadata = self
            .db
            .get_chain_metadata()
            .await
            .rpc_status_internal_error(LOG_TARGET)?;

        Ok(Response::new(QueryDeletedResponse {
            best_block_height: metadata.height_of_longest_chain(),
            best_block_hash: metadata.best_block().to_vec(),
            data: return_data,
        }))
    }

    async fn get_tip_info(&self, _request: Request<()>) -> Result<Response<TipInfoResponse>, RpcStatus> {
        let state_machine = self.state_machine();
        let status_watch = state_machine.get_status_info_watch();
        let is_synced = match status_watch.borrow().state_info {
            StateInfo::Listening(li) => li.is_synced(),
            _ => false,
        };

        let metadata = self
            .db
            .get_chain_metadata()
            .await
            .rpc_status_internal_error(LOG_TARGET)?;

        Ok(Response::new(TipInfoResponse {
            metadata: Some(metadata.into()),
            is_synced,
        }))
    }

    async fn get_header(&self, request: Request<u64>) -> Result<Response<proto::core::BlockHeader>, RpcStatus> {
        let height = request.into_message();
        let header = self
            .db()
            .fetch_header(height)
            .await
            .rpc_status_internal_error(LOG_TARGET)?
            .ok_or_else(|| RpcStatus::not_found(&format!("Header not found at height {}", height)))?;

        Ok(Response::new(header.into()))
    }

    async fn get_header_by_height(
        &self,
        request: Request<u64>,
    ) -> Result<Response<proto::core::BlockHeader>, RpcStatus> {
        let height = request.into_message();
        let header = self
            .db()
            .fetch_header(height)
            .await
            .rpc_status_internal_error(LOG_TARGET)?
            .ok_or_else(|| RpcStatus::not_found(&format!("Header not found at height {}", height)))?;

        Ok(Response::new(header.into()))
    }

    async fn get_height_at_time(&self, request: Request<u64>) -> Result<Response<u64>, RpcStatus> {
        let requested_epoch_time: u64 = request.into_message();

        let tip_header = self
            .db()
            .fetch_tip_header()
            .await
            .rpc_status_internal_error(LOG_TARGET)?;

        let mut left_height = 0u64;
        let mut right_height = tip_header.height();

        while left_height <= right_height {
            let mut mid_height = (left_height + right_height) / 2;

            if mid_height == 0 {
                return Ok(Response::new(0u64));
            }
            // If the two bounds are adjacent then perform the test between the right and left sides
            if left_height == mid_height {
                mid_height = right_height;
            }

            let mid_header = self
                .db()
                .fetch_header(mid_height)
                .await
                .rpc_status_internal_error(LOG_TARGET)?
                .ok_or_else(|| {
                    RpcStatus::not_found(&format!("Header not found during search at height {}", mid_height))
                })?;
            let before_mid_header = self
                .db()
                .fetch_header(mid_height - 1)
                .await
                .rpc_status_internal_error(LOG_TARGET)?
                .ok_or_else(|| {
                    RpcStatus::not_found(&format!("Header not found during search at height {}", mid_height - 1))
                })?;
            if requested_epoch_time < mid_header.timestamp.as_u64() &&
                requested_epoch_time >= before_mid_header.timestamp.as_u64()
            {
                return Ok(Response::new(before_mid_header.height));
            } else if mid_height == right_height {
                return Ok(Response::new(right_height));
            } else if requested_epoch_time <= mid_header.timestamp.as_u64() {
                right_height = mid_height;
            } else {
                left_height = mid_height;
            }
        }

        Ok(Response::new(0u64))
    }

    async fn sync_utxos_by_block(
        &self,
        request: Request<SyncUtxosByBlockRequest>,
    ) -> Result<Streaming<SyncUtxosByBlockResponse>, RpcStatus> {
        let req = request.message();
        let peer = request.context().peer_node_id();
        debug!(
            target: LOG_TARGET,
            "Received sync_utxos_by_block request from {} from header {} to {} ",
            peer,
            req.start_header_hash.to_hex(),
            req.end_header_hash.to_hex(),
        );

        // Number of blocks to load and push to the stream before loading the next batch. Most blocks have 1 output but
        // full blocks will have 500
        const BATCH_SIZE: usize = 5;
        let (tx, rx) = mpsc::channel(BATCH_SIZE);
        let task = SyncUtxosByBlockTask::new(self.db());
        task.run(request.into_message(), tx).await?;

        Ok(Streaming::new(rx))
    }

    async fn get_mempool_fee_per_gram_stats(
        &self,
        request: Request<GetMempoolFeePerGramStatsRequest>,
    ) -> Result<Response<GetMempoolFeePerGramStatsResponse>, RpcStatus> {
        let req = request.into_message();
        let count =
            usize::try_from(req.count).map_err(|_| RpcStatus::bad_request("count must be less than or equal to 20"))?;

        if count > 20 {
            return Err(RpcStatus::bad_request("count must be less than or equal to 20"));
        }

        let metadata = self
            .db
            .get_chain_metadata()
            .await
            .rpc_status_internal_error(LOG_TARGET)?;
        let stats = self
            .mempool()
            .get_fee_per_gram_stats(count, metadata.height_of_longest_chain())
            .await
            .rpc_status_internal_error(LOG_TARGET)?;

        Ok(Response::new(stats.into()))
    }
}
