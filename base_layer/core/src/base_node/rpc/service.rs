// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::convert::{TryFrom, TryInto};

use log::*;
use tari_common_types::types::{CompressedSignature, FixedHash};
use tari_comms::protocol::rpc::{Request, Response, RpcStatus, RpcStatusResultExt, Streaming};
use tari_transaction_components::transaction_components::Transaction;
use tari_utilities::hex::Hex;
use tokio::sync::mpsc;
use url::Url;

use crate::{
    base_node::{
        StateMachineHandle,
        rpc::{BaseNodeWalletService, MAX_ALLOWED_QUERY_SIZE, sync_utxos_by_block_task::SyncUtxosByBlockTask},
        state_machine_service::states::StateInfo,
    },
    chain_storage::{BlockchainBackend, async_db::AsyncBlockchainDb},
    mempool::{TxStorageResponse, service::MempoolHandle},
    proto,
    proto::{
        base_node::{
            FetchMatchingUtxos,
            FetchUtxosResponse,
            GetMempoolFeePerGramStatsRequest,
            GetMempoolFeePerGramStatsResponse,
            GetWalletQueryHttpServiceAddressResponse,
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
};

const LOG_TARGET: &str = "c::base_node::rpc";

pub struct BaseNodeWalletRpcService<B> {
    db: AsyncBlockchainDb<B>,
    mempool: MempoolHandle,
    state_machine: StateMachineHandle,
    wallet_query_service_address: Option<Url>,
}

impl<B: BlockchainBackend + 'static> BaseNodeWalletRpcService<B> {
    pub fn new(
        db: AsyncBlockchainDb<B>,
        mempool: MempoolHandle,
        state_machine: StateMachineHandle,
        wallet_query_service_address: Option<Url>,
    ) -> Self {
        Self {
            db,
            mempool,
            state_machine,
            wallet_query_service_address,
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

    async fn fetch_kernel(&self, signature: CompressedSignature) -> Result<TxQueryResponse, RpcStatus> {
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
                        let confirmations = chain_metadata.best_block_height().saturating_sub(header.height);
                        let response = TxQueryResponse {
                            location: TxLocation::Mined as i32,
                            best_block_hash: block_hash.to_vec(),
                            confirmations,
                            is_synced,
                            best_block_height: chain_metadata.best_block_height(),
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
                best_block_hash: vec![],
                confirmations: 0,
                is_synced,
                best_block_height: chain_metadata.best_block_height(),
                mined_timestamp: 0,
            },
            TxStorageResponse::ReorgPool |
            TxStorageResponse::NotStoredOrphan |
            TxStorageResponse::NotStoredTimeLocked |
            TxStorageResponse::NotStoredAlreadySpent |
            TxStorageResponse::NotStoredConsensus(_) |
            TxStorageResponse::NotStored(_) |
            TxStorageResponse::NotStoredFeeTooLow |
            TxStorageResponse::NotStoredValidatorNodeSlotTaken |
            TxStorageResponse::NotStoredAlreadyMined => TxQueryResponse {
                location: TxLocation::NotStored as i32,
                best_block_hash: vec![],
                confirmations: 0,
                is_synced,
                best_block_height: chain_metadata.best_block_height(),
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
            TxStorageResponse::NotStoredValidatorNodeSlotTaken => TxSubmissionResponse {
                accepted: false,
                rejection_reason: TxSubmissionRejectionReason::ValidatorNodeSlotTaken.into(),
                is_synced,
            },
            TxStorageResponse::NotStoredTimeLocked => TxSubmissionResponse {
                accepted: false,
                rejection_reason: TxSubmissionRejectionReason::TimeLocked.into(),
                is_synced,
            },
            TxStorageResponse::NotStoredConsensus(_) | TxStorageResponse::NotStored(_) => TxSubmissionResponse {
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
        let signature =
            CompressedSignature::try_from(message).map_err(|_| RpcStatus::bad_request("Signature was invalid"))?;

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
        if message.sigs.is_empty() {
            return Err(RpcStatus::bad_request("Empty signatures"));
        }
        if message.sigs.len() > MAX_ALLOWED_QUERY_SIZE {
            return Err(RpcStatus::bad_request(&format!(
                "Exceeded maximum allowed query signatures. Max: {MAX_ALLOWED_QUERY_SIZE}"
            )));
        }

        let mut responses: Vec<TxQueryBatchResponse> = Vec::with_capacity(message.sigs.len());

        let metadata = self
            .db
            .get_chain_metadata()
            .await
            .rpc_status_internal_error(LOG_TARGET)?;

        for sig in message.sigs {
            let signature =
                CompressedSignature::try_from(sig).map_err(|_| RpcStatus::bad_request("Signature was invalid"))?;
            let response: TxQueryResponse = self.fetch_kernel(signature.clone()).await?;
            responses.push(TxQueryBatchResponse {
                signature: Some(SignatureProto::from(&signature)),
                location: response.location,
                best_block_hash: response.best_block_hash,
                confirmations: response.confirmations,
                best_block_height: response.best_block_height.saturating_sub(response.confirmations),
                mined_timestamp: response.mined_timestamp,
            });
        }
        Ok(Response::new(TxQueryBatchResponses {
            responses,
            is_synced,
            best_block_hash: metadata.best_block_hash().to_vec(),
            best_block_height: metadata.best_block_height(),
            tip_mined_timestamp: metadata.timestamp(),
        }))
    }

    async fn fetch_matching_utxos(
        &self,
        request: Request<FetchMatchingUtxos>,
    ) -> Result<Response<FetchUtxosResponse>, RpcStatus> {
        let message = request.into_message();
        if message.output_hashes.is_empty() {
            return Err(RpcStatus::bad_request("Empty output hashes"));
        }
        if message.output_hashes.len() > MAX_ALLOWED_QUERY_SIZE {
            return Err(RpcStatus::bad_request(&format!(
                "Exceeded maximum allowed query hashes. Max: {MAX_ALLOWED_QUERY_SIZE}"
            )));
        }

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
        if message.output_hashes.len() > MAX_ALLOWED_QUERY_SIZE {
            return Err(RpcStatus::bad_request(&format!(
                "Exceeded maximum allowed query hashes. Max: {MAX_ALLOWED_QUERY_SIZE}"
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
            mined_info_resp.len().saturating_sub(num_mined)
        );
        let metadata = self
            .db
            .get_chain_metadata()
            .await
            .rpc_status_internal_error(LOG_TARGET)?;

        Ok(Response::new(UtxoQueryResponses {
            best_block_height: metadata.best_block_height(),
            best_block_hash: metadata.best_block_hash().to_vec(),
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
        if message.hashes.is_empty() {
            return Err(RpcStatus::bad_request("Empty utxo hashes"));
        }
        if message.hashes.len() > MAX_ALLOWED_QUERY_SIZE {
            return Err(RpcStatus::bad_request(&format!(
                "Exceeded maximum allowed query hashes. Max: {MAX_ALLOWED_QUERY_SIZE}"
            )));
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
        // One read for all hashes; each entry's mined and spent halves describe the same lifecycle.
        let infos = self
            .db
            .fetch_latest_mined_infos(hashes)
            .await
            .rpc_status_internal_error(LOG_TARGET)?;
        for info in infos {
            let mut data = match &info.output {
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
            if let Some(input) = &info.input {
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
            best_block_height: metadata.best_block_height(),
            best_block_hash: metadata.best_block_hash().to_vec(),
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
            .ok_or_else(|| RpcStatus::not_found(&format!("Header not found at height {height}")))?;

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
            .ok_or_else(|| RpcStatus::not_found(&format!("Header not found at height {height}")))?;

        Ok(Response::new(header.into()))
    }

    async fn get_height_at_time(&self, request: Request<u64>) -> Result<Response<u64>, RpcStatus> {
        let requested_epoch_time: u64 = request.into_message();
        trace!(target: LOG_TARGET, "requested_epoch_time: {requested_epoch_time}");
        let tip_header = self
            .db()
            .fetch_tip_header()
            .await
            .rpc_status_internal_error(LOG_TARGET)?;

        let mut left_height = 0u64;
        let mut right_height = tip_header.height();
        trace!(
            target: LOG_TARGET,
            "requested_epoch_time: {}, left: {}, right: {}",
            requested_epoch_time,
            left_height,
            right_height
        );

        while left_height <= right_height {
            let mut mid_height = left_height.saturating_add(right_height) / 2;

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
                    RpcStatus::not_found(&format!("Header not found during search at height {mid_height}"))
                })?;
            let before_mid_header = self
                .db()
                .fetch_header(mid_height.saturating_sub(1))
                .await
                .rpc_status_internal_error(LOG_TARGET)?
                .ok_or_else(|| {
                    RpcStatus::not_found(&format!(
                        "Header not found during search at height {}",
                        mid_height.saturating_sub(1)
                    ))
                })?;
            trace!(
                target: LOG_TARGET,
                "requested_epoch_time: {}, left: {}, mid: {}/{} ({}/{}), right: {}",
                requested_epoch_time,
                left_height,
                mid_height,
                mid_height.saturating_sub(1),
                mid_header.timestamp.as_u64(),
                before_mid_header.timestamp.as_u64(),
                right_height
            );
            if requested_epoch_time < mid_header.timestamp.as_u64() &&
                requested_epoch_time >= before_mid_header.timestamp.as_u64()
            {
                trace!(
                    target: LOG_TARGET,
                    "requested_epoch_time: {}, selected height: {}",
                    requested_epoch_time, before_mid_header.height
                );
                return Ok(Response::new(before_mid_header.height));
            } else if mid_height == right_height {
                trace!(
                    target: LOG_TARGET,
                    "requested_epoch_time: {requested_epoch_time}, selected height: {right_height}"
                );
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
            .get_fee_per_gram_stats(count, metadata.best_block_height())
            .await
            .rpc_status_internal_error(LOG_TARGET)?;

        Ok(Response::new(stats.into()))
    }

    async fn get_wallet_query_http_service_address(
        &self,
        _request: Request<()>,
    ) -> Result<Response<GetWalletQueryHttpServiceAddressResponse>, RpcStatus> {
        Ok(Response::new(GetWalletQueryHttpServiceAddressResponse {
            http_address: self
                .wallet_query_service_address
                .clone()
                .map(|url| url.to_string())
                .unwrap_or_default(),
        }))
    }
}

#[cfg(test)]
mod test {
    use tari_common::configuration::Network;
    use tari_comms::protocol::rpc::RpcStatusCode;
    use tari_shutdown::Shutdown;
    use tokio::sync::{broadcast, watch};

    use super::*;
    use crate::{
        base_node::state_machine_service::states::StatusInfo,
        mempool::test_utils::{
            covenant_over_token_limit,
            mock::create_mempool_service_mock,
            transaction_proto_with_covenant,
        },
        test_helpers::blockchain::create_new_blockchain_with_network,
    };

    /// The service has no connectivity handle, so it cannot ban: a rejection here is only ever a bad request
    #[tokio::test]
    async fn submit_transaction_rejects_a_covenant_over_the_token_limit() {
        let db = AsyncBlockchainDb::from(create_new_blockchain_with_network(Network::LocalNet));
        let (mempool, mempool_state) = create_mempool_service_mock();
        let (state_tx, _state_rx) = broadcast::channel(10);
        let (_status_tx, status_rx) = watch::channel(StatusInfo::new());
        let shutdown = Shutdown::new();
        let state_machine = StateMachineHandle::new(state_tx, status_rx, shutdown.to_signal());
        let service = BaseNodeWalletRpcService::new(db, mempool, state_machine, None);

        let err = service
            .submit_transaction(Request::new(
                0.into(),
                transaction_proto_with_covenant(&covenant_over_token_limit()),
            ))
            .await
            .unwrap_err();
        assert_eq!(err.as_status_code(), RpcStatusCode::BadRequest);
        assert!(mempool_state.submitted_transactions().await.is_empty());

        // The same transaction with a covenant at the limit reaches the mempool
        service
            .submit_transaction(Request::new(0.into(), transaction_proto_with_covenant(&[0x20; 128])))
            .await
            .unwrap();
        assert_eq!(mempool_state.submitted_transactions().await.len(), 1);
    }

    #[tokio::test]
    async fn duplicate_output_query_deleted_ignores_previous_lifecycle() {
        let (chain, ..) = crate::validation::block_body::test::duplicate_output_chain();
        let spent = chain.get_block_and_smt_by_name("3").unwrap();
        let remined = chain.get_block_and_smt_by_name("4").unwrap();
        let hash = spent.block().body.inputs().first().unwrap().output_hash();
        let (mempool, _) = create_mempool_service_mock();
        let (state_tx, _state_rx) = broadcast::channel(10);
        let (_status_tx, status_rx) = watch::channel(StatusInfo::new());
        let shutdown = Shutdown::new();
        let state_machine = StateMachineHandle::new(state_tx, status_rx, shutdown.to_signal());
        let service = BaseNodeWalletRpcService::new(chain.db().clone().into(), mempool, state_machine, None);
        let response = service
            .query_deleted(Request::new(0.into(), QueryDeletedRequest {
                hashes: vec![hash.to_vec()],
                chain_must_include_header: remined.hash().to_vec(),
            }))
            .await
            .unwrap()
            .into_message();
        let data = response.data.first().unwrap();
        assert_eq!(data.mined_at_height, remined.height());
        assert_eq!(data.height_deleted_at, 0);
        assert!(data.block_deleted_in.is_empty());
    }

    #[tokio::test]
    async fn sync_utxos_by_block_enforces_range_limit_and_main_chain_headers() {
        use tari_common_types::{
            chain_metadata::ChainMetadata,
            epoch::VnEpoch,
            types::{BadBlock, CompressedCommitment, CompressedPublicKey, CompressedSignature, HashOutput},
        };
        use tari_node_components::blocks::{Block, BlockHeader, BlockHeaderAccumulatedData, ChainBlock, ChainHeader};
        use tari_transaction_components::transaction_components::{
            TransactionInput,
            TransactionKernel,
            TransactionOutput,
        };

        use crate::{
            blocks::BlockAccumulatedData,
            chain_storage::{
                AccumulatedDataRebuildStatus,
                BlockchainCheckRequest,
                BlockchainCheckStatus,
                BlockchainDatabase,
                BlockchainDatabaseConfig,
                BurnCommitmentRebuildStatus,
                ChainStorageError,
                DbBasicStats,
                DbKey,
                DbTotalSizeStats,
                DbTransaction,
                DbValue,
                HorizonData,
                HorizonSyncOutputCheckpoint,
                InputMinedInfo,
                MinedInfo,
                MmrTree,
                OutputMinedInfo,
                OwnedLmdbTreeReader,
                PayrefRebuildStatus,
                Reorg,
                ValidatorNodeEntry,
                ValidatorNodeRegistrationInfo,
                Validators,
            },
            test_helpers::{blockchain::TempDatabase, create_consensus_rules},
            validation::{DifficultyCalculator, mocks::MockValidator},
        };

        struct ForkHeaderBackend {
            inner: TempDatabase,
            fork_header: BlockHeader,
        }

        impl BlockchainBackend for ForkHeaderBackend {
            fn write(&mut self, tx: DbTransaction) -> Result<(), ChainStorageError> {
                self.inner.write(tx)
            }

            fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, ChainStorageError> {
                if key == &DbKey::HeaderHash(self.fork_header.hash()) {
                    return Ok(Some(DbValue::HeaderHash(Box::new(self.fork_header.clone()))));
                }
                self.inner.fetch(key)
            }

            fn contains(&self, key: &DbKey) -> Result<bool, ChainStorageError> {
                self.inner.contains(key)
            }

            fn fetch_chain_header_by_height(&self, height: u64) -> Result<ChainHeader, ChainStorageError> {
                self.inner.fetch_chain_header_by_height(height)
            }

            fn fetch_header_accumulated_data(
                &self,
                hash: &HashOutput,
            ) -> Result<Option<BlockHeaderAccumulatedData>, ChainStorageError> {
                self.inner.fetch_header_accumulated_data(hash)
            }

            fn fetch_chain_header_in_all_chains(&self, hash: &HashOutput) -> Result<ChainHeader, ChainStorageError> {
                self.inner.fetch_chain_header_in_all_chains(hash)
            }

            fn fetch_header_containing_kernel_mmr(&self, mmr_position: u64) -> Result<ChainHeader, ChainStorageError> {
                self.inner.fetch_header_containing_kernel_mmr(mmr_position)
            }

            fn is_empty(&self) -> Result<bool, ChainStorageError> {
                self.inner.is_empty()
            }

            fn fetch_block_accumulated_data(
                &self,
                header_hash: &HashOutput,
            ) -> Result<Option<BlockAccumulatedData>, ChainStorageError> {
                self.inner.fetch_block_accumulated_data(header_hash)
            }

            fn fetch_block_accumulated_data_by_height(
                &self,
                height: u64,
            ) -> Result<Option<BlockAccumulatedData>, ChainStorageError> {
                self.inner.fetch_block_accumulated_data_by_height(height)
            }

            fn fetch_kernels_in_block(
                &self,
                header_hash: &HashOutput,
            ) -> Result<Vec<TransactionKernel>, ChainStorageError> {
                self.inner.fetch_kernels_in_block(header_hash)
            }

            fn fetch_bad_blocks(&self) -> Result<Vec<BadBlock>, ChainStorageError> {
                self.inner.fetch_bad_blocks()
            }

            fn clear_all_bad_blocks(&mut self) -> Result<(), ChainStorageError> {
                self.inner.clear_all_bad_blocks()
            }

            fn fetch_kernel_by_excess_sig(
                &self,
                excess_sig: &CompressedSignature,
            ) -> Result<Option<(TransactionKernel, HashOutput)>, ChainStorageError> {
                self.inner.fetch_kernel_by_excess_sig(excess_sig)
            }

            fn fetch_kernel_by_excess(
                &self,
                excess: &CompressedCommitment,
            ) -> Result<Option<(TransactionKernel, HashOutput)>, ChainStorageError> {
                self.inner.fetch_kernel_by_excess(excess)
            }

            fn fetch_kernel_by_burn_commitment(
                &self,
                burn_commitment: &CompressedCommitment,
            ) -> Result<Option<(TransactionKernel, HashOutput)>, ChainStorageError> {
                self.inner.fetch_kernel_by_burn_commitment(burn_commitment)
            }

            fn fetch_outputs_in_block_with_spend_state(
                &self,
                header_hash: &HashOutput,
                spend_status_at_header: Option<&HashOutput>,
            ) -> Result<Vec<(TransactionOutput, bool)>, ChainStorageError> {
                self.inner
                    .fetch_outputs_in_block_with_spend_state(header_hash, spend_status_at_header)
            }

            fn fetch_outputs(&self, output_hash: &HashOutput) -> Result<Vec<OutputMinedInfo>, ChainStorageError> {
                self.inner.fetch_outputs(output_hash)
            }

            fn contains_output_hash(&self, output_hash: &HashOutput) -> Result<bool, ChainStorageError> {
                self.inner.contains_output_hash(output_hash)
            }

            fn fetch_inputs(&self, output_hash: &HashOutput) -> Result<Vec<InputMinedInfo>, ChainStorageError> {
                self.inner.fetch_inputs(output_hash)
            }

            fn fetch_unspent_output_hash_by_commitment(
                &self,
                commitment: &CompressedCommitment,
            ) -> Result<Option<HashOutput>, ChainStorageError> {
                self.inner.fetch_unspent_output_hash_by_commitment(commitment)
            }

            fn fetch_mined_info_by_payref(&self, payref: &FixedHash) -> Result<MinedInfo, ChainStorageError> {
                self.inner.fetch_mined_info_by_payref(payref)
            }

            fn fetch_mined_info_by_output_hash(
                &self,
                output_hash: &HashOutput,
            ) -> Result<Vec<MinedInfo>, ChainStorageError> {
                self.inner.fetch_mined_info_by_output_hash(output_hash)
            }

            fn fetch_outputs_in_block(
                &self,
                header_hash: &HashOutput,
            ) -> Result<Vec<TransactionOutput>, ChainStorageError> {
                self.inner.fetch_outputs_in_block(header_hash)
            }

            fn fetch_inputs_in_block(
                &self,
                header_hash: &HashOutput,
            ) -> Result<Vec<TransactionInput>, ChainStorageError> {
                self.inner.fetch_inputs_in_block(header_hash)
            }

            fn fetch_mmr_size(&self, tree: MmrTree) -> Result<u64, ChainStorageError> {
                self.inner.fetch_mmr_size(tree)
            }

            fn orphan_count(&self) -> Result<usize, ChainStorageError> {
                self.inner.orphan_count()
            }

            fn fetch_last_header(&self) -> Result<BlockHeader, ChainStorageError> {
                self.inner.fetch_last_header()
            }

            fn clear_all_pending_headers(&self) -> Result<usize, ChainStorageError> {
                self.inner.clear_all_pending_headers()
            }

            fn fetch_last_chain_header(&self) -> Result<ChainHeader, ChainStorageError> {
                self.inner.fetch_last_chain_header()
            }

            fn fetch_tip_header(&self) -> Result<ChainHeader, ChainStorageError> {
                self.inner.fetch_tip_header()
            }

            fn fetch_chain_metadata(&self) -> Result<ChainMetadata, ChainStorageError> {
                self.inner.fetch_chain_metadata()
            }

            fn fetch_payref_rebuild_status(&self) -> Result<PayrefRebuildStatus, ChainStorageError> {
                self.inner.fetch_payref_rebuild_status()
            }

            fn fetch_accumulated_data_rebuild_status(&self) -> Result<AccumulatedDataRebuildStatus, ChainStorageError> {
                self.inner.fetch_accumulated_data_rebuild_status()
            }

            fn set_accumulated_data_rebuild_status(
                &self,
                status: AccumulatedDataRebuildStatus,
            ) -> Result<(), ChainStorageError> {
                self.inner.set_accumulated_data_rebuild_status(status)
            }

            fn fetch_burn_commitment_rebuild_status(&self) -> Result<BurnCommitmentRebuildStatus, ChainStorageError> {
                self.inner.fetch_burn_commitment_rebuild_status()
            }

            fn update_accumulated_data_check_status(
                &self,
                request: BlockchainCheckRequest,
            ) -> Result<BlockchainCheckStatus, ChainStorageError> {
                self.inner.update_accumulated_data_check_status(request)
            }

            fn update_blockchain_consistency_check_status(
                &self,
                request: BlockchainCheckRequest,
            ) -> Result<BlockchainCheckStatus, ChainStorageError> {
                self.inner.update_blockchain_consistency_check_status(request)
            }

            fn fetch_accumulated_data_check_status(
                &self,
            ) -> Result<Option<BlockchainCheckStatus>, ChainStorageError> {
                self.inner.fetch_accumulated_data_check_status()
            }

            fn fetch_blockchain_consistency_check_status(
                &self,
            ) -> Result<Option<BlockchainCheckStatus>, ChainStorageError> {
                self.inner.fetch_blockchain_consistency_check_status()
            }

            fn build_payref_indexes_for_height(
                &self,
                height: u64,
                metadata_at_start: ChainMetadata,
                initialize_stats: Option<u64>,
                finalize: bool,
            ) -> Result<PayrefRebuildStatus, ChainStorageError> {
                self.inner
                    .build_payref_indexes_for_height(height, metadata_at_start, initialize_stats, finalize)
            }

            fn build_burn_commitment_index_for_height(
                &self,
                height: u64,
                finalize: bool,
            ) -> Result<BurnCommitmentRebuildStatus, ChainStorageError> {
                self.inner.build_burn_commitment_index_for_height(height, finalize)
            }

            fn update_accumulated_difficulty(
                &self,
                height: u64,
                header_accum_data: BlockHeaderAccumulatedData,
                last_chain_header: ChainHeader,
                update_meta_data_db: bool,
            ) -> Result<AccumulatedDataRebuildStatus, ChainStorageError> {
                self.inner.update_accumulated_difficulty(
                    height,
                    header_accum_data,
                    last_chain_header,
                    update_meta_data_db,
                )
            }

            fn utxo_count(&self) -> Result<usize, ChainStorageError> {
                self.inner.utxo_count()
            }

            fn kernel_count(&self) -> Result<usize, ChainStorageError> {
                self.inner.kernel_count()
            }

            fn fetch_orphan_chain_tip_by_hash(
                &self,
                hash: &HashOutput,
            ) -> Result<Option<ChainHeader>, ChainStorageError> {
                self.inner.fetch_orphan_chain_tip_by_hash(hash)
            }

            fn fetch_strongest_orphan_chain_tips(&self) -> Result<Vec<ChainHeader>, ChainStorageError> {
                self.inner.fetch_strongest_orphan_chain_tips()
            }

            fn fetch_orphan_children_of(&self, hash: HashOutput) -> Result<Vec<Block>, ChainStorageError> {
                self.inner.fetch_orphan_children_of(hash)
            }

            fn fetch_orphan_hashes_at_or_above(&self, height: u64) -> Result<Vec<HashOutput>, ChainStorageError> {
                self.inner.fetch_orphan_hashes_at_or_above(height)
            }

            fn fetch_orphan_chain_block(&self, hash: HashOutput) -> Result<Option<ChainBlock>, ChainStorageError> {
                self.inner.fetch_orphan_chain_block(hash)
            }

            fn delete_oldest_orphans(
                &mut self,
                horizon_height: u64,
                orphan_storage_capacity: usize,
            ) -> Result<(), ChainStorageError> {
                self.inner
                    .delete_oldest_orphans(horizon_height, orphan_storage_capacity)
            }

            fn fetch_monero_seed_first_seen_height(&self, seed: &[u8]) -> Result<u64, ChainStorageError> {
                self.inner.fetch_monero_seed_first_seen_height(seed)
            }

            fn fetch_horizon_data(&self) -> Result<Option<HorizonData>, ChainStorageError> {
                self.inner.fetch_horizon_data()
            }

            fn fetch_horizon_sync_output_checkpoint(
                &self,
            ) -> Result<Option<HorizonSyncOutputCheckpoint>, ChainStorageError> {
                self.inner.fetch_horizon_sync_output_checkpoint()
            }

            fn verify_horizon_sync_output_root(&self, expected_root: HashOutput) -> Result<(), ChainStorageError> {
                self.inner.verify_horizon_sync_output_root(expected_root)
            }

            fn get_stats(&self) -> Result<DbBasicStats, ChainStorageError> {
                self.inner.get_stats()
            }

            fn fetch_total_size_stats(&self) -> Result<DbTotalSizeStats, ChainStorageError> {
                self.inner.fetch_total_size_stats()
            }

            fn bad_block_exists(&self, block_hash: HashOutput) -> Result<(bool, String), ChainStorageError> {
                self.inner.bad_block_exists(block_hash)
            }

            fn fetch_all_reorgs(&self) -> Result<Vec<Reorg>, ChainStorageError> {
                self.inner.fetch_all_reorgs()
            }

            fn fetch_all_active_validator_nodes(
                &self,
                height: u64,
            ) -> Result<Vec<ValidatorNodeRegistrationInfo>, ChainStorageError> {
                self.inner.fetch_all_active_validator_nodes(height)
            }

            fn fetch_active_validator_nodes(
                &self,
                sidechain_pk: Option<&CompressedPublicKey>,
                height: u64,
            ) -> Result<Vec<ValidatorNodeRegistrationInfo>, ChainStorageError> {
                self.inner.fetch_active_validator_nodes(sidechain_pk, height)
            }

            fn fetch_validators_activating_in_epoch(
                &self,
                sidechain_pk: Option<&CompressedPublicKey>,
                epoch: VnEpoch,
            ) -> Result<Vec<ValidatorNodeRegistrationInfo>, ChainStorageError> {
                self.inner.fetch_validators_activating_in_epoch(sidechain_pk, epoch)
            }

            fn fetch_validators_exiting_in_epoch(
                &self,
                sidechain_pk: Option<&CompressedPublicKey>,
                epoch: VnEpoch,
            ) -> Result<Vec<ValidatorNodeRegistrationInfo>, ChainStorageError> {
                self.inner.fetch_validators_exiting_in_epoch(sidechain_pk, epoch)
            }

            fn validator_node_exists(
                &self,
                sidechain_pk: Option<&CompressedPublicKey>,
                end_epoch: VnEpoch,
                validator_node_pk: &CompressedPublicKey,
            ) -> Result<bool, ChainStorageError> {
                self.inner
                    .validator_node_exists(sidechain_pk, end_epoch, validator_node_pk)
            }

            fn validator_node_is_active(
                &self,
                sidechain_pk: Option<&CompressedPublicKey>,
                end_epoch: VnEpoch,
                validator_node_pk: &CompressedPublicKey,
            ) -> Result<bool, ChainStorageError> {
                self.inner
                    .validator_node_is_active(sidechain_pk, end_epoch, validator_node_pk)
            }

            fn fetch_validator_node_entry(
                &self,
                sidechain_pk: Option<&CompressedPublicKey>,
                public_key: &CompressedPublicKey,
            ) -> Result<Option<ValidatorNodeEntry>, ChainStorageError> {
                self.inner.fetch_validator_node_entry(sidechain_pk, public_key)
            }

            fn validator_node_has_pending_exit(
                &self,
                sidechain_pk: Option<&CompressedPublicKey>,
                public_key: &CompressedPublicKey,
                commitment: &CompressedCommitment,
                epoch: VnEpoch,
            ) -> Result<bool, ChainStorageError> {
                self.inner
                    .validator_node_has_pending_exit(sidechain_pk, public_key, commitment, epoch)
            }

            fn get_validator_node(
                &self,
                sidechain_pk: Option<&CompressedPublicKey>,
                public_key: CompressedPublicKey,
            ) -> Result<Option<ValidatorNodeRegistrationInfo>, ChainStorageError> {
                self.inner.get_validator_node(sidechain_pk, public_key)
            }

            fn create_smt_reader(&self) -> Result<(OwnedLmdbTreeReader<'_>, u64), ChainStorageError> {
                self.inner.create_smt_reader()
            }

            fn fetch_all_orphans(&self) -> Result<Vec<ChainHeader>, ChainStorageError> {
                self.inner.fetch_all_orphans()
            }

            fn set_stats_total_height(&self, total: u64) {
                self.inner.set_stats_total_height(total)
            }

            fn update_stats_progress(&self, current: u64) {
                self.inner.update_stats_progress(current)
            }
        }

        let mut fork_header = BlockHeader::from_previous(
            &create_new_blockchain_with_network(Network::LocalNet)
                .fetch_chain_header(0)
                .unwrap()
                .into_header(),
        );
        fork_header.nonce = 999_999;
        let fork_hash = fork_header.hash();

        let rules = create_consensus_rules();
        let validators = Validators::new(
            MockValidator::new(true),
            MockValidator::new(true),
            MockValidator::new(true),
        );
        let db = BlockchainDatabase::start_new(
            ForkHeaderBackend {
                inner: TempDatabase::new(),
                fork_header,
            },
            rules.clone(),
            validators,
            BlockchainDatabaseConfig::default(),
            DifficultyCalculator::new(rules, Default::default()),
        )
        .unwrap();
        let genesis = db.fetch_chain_header(0).unwrap();

        let mut headers = Vec::with_capacity(MAX_ALLOWED_QUERY_SIZE);
        let mut prev_header = genesis.header().clone();
        for _ in 0..MAX_ALLOWED_QUERY_SIZE {
            let mut next_header = BlockHeader::from_previous(&prev_header);
            next_header.kernel_mmr_size += 1;
            let mut accum = genesis.accumulated_data().clone();
            accum.hash = next_header.hash();
            let chain_header = ChainHeader::try_construct(next_header.clone(), accum).unwrap();
            prev_header = next_header;
            headers.push(chain_header);
        }
        let max_valid_end_hash = *headers[MAX_ALLOWED_QUERY_SIZE - 2].hash();
        let far_hash = *headers.last().unwrap().hash();
        db.insert_valid_headers(headers).unwrap();

        let async_db = AsyncBlockchainDb::from(db);

        // 1. Span of MAX_ALLOWED_QUERY_SIZE + 1 blocks (0..=MAX_ALLOWED_QUERY_SIZE) is rejected with BadRequest.
        let (tx, _rx) = mpsc::channel(5);
        let err = SyncUtxosByBlockTask::new(async_db.clone())
            .run(
                SyncUtxosByBlockRequest {
                    start_header_hash: genesis.hash().to_vec(),
                    end_header_hash: far_hash.to_vec(),
                },
                tx,
            )
            .await
            .unwrap_err();
        assert_eq!(err.as_status_code(), RpcStatusCode::BadRequest);

        // 2. Largest allowed span (0..=(MAX_ALLOWED_QUERY_SIZE - 1), i.e. MAX_ALLOWED_QUERY_SIZE blocks) succeeds.
        let (tx, mut rx) = mpsc::channel(5);
        SyncUtxosByBlockTask::new(async_db.clone())
            .run(
                SyncUtxosByBlockRequest {
                    start_header_hash: genesis.hash().to_vec(),
                    end_header_hash: max_valid_end_hash.to_vec(),
                },
                tx,
            )
            .await
            .unwrap();
        let first_msg = rx.recv().await.unwrap().unwrap();
        assert_eq!(first_msg.height, 0);

        // 3. Off-main-chain start_header_hash is rejected with NotFound.
        let (tx, _rx) = mpsc::channel(5);
        let err = SyncUtxosByBlockTask::new(async_db.clone())
            .run(
                SyncUtxosByBlockRequest {
                    start_header_hash: fork_hash.to_vec(),
                    end_header_hash: max_valid_end_hash.to_vec(),
                },
                tx,
            )
            .await
            .unwrap_err();
        assert_eq!(err.as_status_code(), RpcStatusCode::NotFound);

        // 4. Off-main-chain end_header_hash is rejected with NotFound.
        let (tx, _rx) = mpsc::channel(5);
        let err = SyncUtxosByBlockTask::new(async_db)
            .run(
                SyncUtxosByBlockRequest {
                    start_header_hash: genesis.hash().to_vec(),
                    end_header_hash: fork_hash.to_vec(),
                },
                tx,
            )
            .await
            .unwrap_err();
        assert_eq!(err.as_status_code(), RpcStatusCode::NotFound);
    }
}
