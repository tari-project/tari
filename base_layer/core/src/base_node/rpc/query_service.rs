// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::cmp;

use log::trace;
use serde_valid::{Validate, validation};
use tari_common_types::{
    types,
    types::{FixedHash, FixedHashSizeError},
};
use tari_transaction_components::{
    rpc::{
        models,
        models::{
            BlockUtxoInfo,
            GenerateKernelMerkleProofResponse,
            GetUtxosByBlockRequest,
            GetUtxosByBlockResponse,
            MinimalUtxoSyncInfo,
            SyncUtxosByBlockRequest,
            SyncUtxosByBlockResponseV0,
            SyncUtxosByBlockResponseV1,
            TipInfoResponse,
            TxLocation,
            TxQueryResponse,
        },
    },
    transaction_components::TransactionOutput,
};
use tari_utilities::{ByteArray, ByteArrayError, hex::Hex};
use thiserror::Error;

use crate::{
    base_node::{StateMachineHandle, rpc::BaseNodeWalletQueryService, state_machine_service::states::StateInfo},
    chain_storage::{BlockchainBackend, ChainStorageError, async_db::AsyncBlockchainDb},
    mempool::{MempoolServiceError, TxStorageResponse, service::MempoolHandle},
};

const LOG_TARGET: &str = "c::bn::rpc::query_service";
const SYNC_UTXOS_SPEND_TIP_SAFETY_LIMIT: u64 = 1000;
const WALLET_MAX_BLOCKS_PER_REQUEST: u64 = 100;
const MAX_UTXO_CHUNK_SIZE: usize = 2000;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Failed to get chain metadata: {0}")]
    FailedToGetChainMetadata(#[from] ChainStorageError),
    #[error("Header not found at height: {height}")]
    HeaderNotFound { height: u64 },
    #[error("Signature conversion error: {0}")]
    SignatureConversion(ByteArrayError),
    #[error("Mempool service error: {0}")]
    MempoolService(#[from] MempoolServiceError),
    #[error("Serde validation error: {0}")]
    SerdeValidation(#[from] validation::Errors),
    #[error("Hash conversion error: {0}")]
    HashConversion(#[from] FixedHashSizeError),
    #[error("Start header hash not found")]
    StartHeaderHashNotFound,
    #[error("End header hash not found")]
    EndHeaderHashNotFound,
    #[error("Header hash not found")]
    HeaderHashNotFound,
    #[error("Start header height {start_height} cannot be greater than the end header height {end_height}")]
    HeaderHeightMismatch { start_height: u64, end_height: u64 },
    #[error("Output not found")]
    OutputNotFound,
    #[error("A general error occurred: {0}")]
    General(anyhow::Error),
}

impl Error {
    fn general(err: impl Into<anyhow::Error>) -> Self {
        Error::General(err.into())
    }
}

pub struct Service<B> {
    db: AsyncBlockchainDb<B>,
    state_machine: StateMachineHandle,
    mempool: MempoolHandle,
    max_utxo_chunk_size: usize,
}

impl<B: BlockchainBackend + 'static> Service<B> {
    pub fn new(db: AsyncBlockchainDb<B>, state_machine: StateMachineHandle, mempool: MempoolHandle) -> Self {
        Self {
            db,
            state_machine,
            mempool,
            max_utxo_chunk_size: MAX_UTXO_CHUNK_SIZE,
        }
    }

    fn state_machine(&self) -> StateMachineHandle {
        self.state_machine.clone()
    }

    fn db(&self) -> &AsyncBlockchainDb<B> {
        &self.db
    }

    fn mempool(&self) -> MempoolHandle {
        self.mempool.clone()
    }

    async fn fetch_kernel(&self, signature: types::CompressedSignature) -> Result<TxQueryResponse, Error> {
        let db = self.db();

        match db.fetch_kernel_by_excess_sig(signature.clone()).await? {
            None => (),
            Some((_, block_hash)) => match db.fetch_header_by_block_hash(block_hash).await? {
                None => (),
                Some(header) => {
                    let response = TxQueryResponse {
                        location: TxLocation::Mined,
                        mined_header_hash: Some(block_hash.to_vec()),
                        mined_height: Some(header.height),
                        mined_timestamp: Some(header.timestamp.as_u64()),
                    };
                    return Ok(response);
                },
            },
        };

        // If not in a block then check the mempool
        let mut mempool = self.mempool();
        let mempool_response = match mempool.get_tx_state_by_excess_sig(signature.clone()).await? {
            TxStorageResponse::UnconfirmedPool => TxQueryResponse {
                location: TxLocation::InMempool,
                mined_header_hash: None,
                mined_height: None,
                mined_timestamp: None,
            },
            TxStorageResponse::ReorgPool |
            TxStorageResponse::NotStoredOrphan |
            TxStorageResponse::NotStoredTimeLocked |
            TxStorageResponse::NotStoredAlreadySpent |
            TxStorageResponse::NotStoredConsensus |
            TxStorageResponse::NotStored |
            TxStorageResponse::NotStoredFeeTooLow |
            TxStorageResponse::NotStoredAlreadyMined => TxQueryResponse {
                location: TxLocation::NotStored,
                mined_timestamp: None,
                mined_height: None,
                mined_header_hash: None,
            },
        };

        Ok(mempool_response)
    }

    async fn fetch_utxos_by_block(&self, request: GetUtxosByBlockRequest) -> Result<GetUtxosByBlockResponse, Error> {
        request.validate()?;

        let hash = request.header_hash.clone().try_into()?;

        let header = self
            .db()
            .fetch_header_by_block_hash(hash)
            .await?
            .ok_or_else(|| Error::HeaderHashNotFound)?;

        // fetch utxos
        let outputs_with_statuses = self.db.fetch_outputs_in_block_with_spend_state(hash, None).await?;

        let outputs = outputs_with_statuses
            .into_iter()
            .map(|(output, _spent)| output)
            .collect::<Vec<TransactionOutput>>();

        // if its empty, we need to send an empty vec of outputs.
        let utxo_block_response = GetUtxosByBlockResponse {
            outputs,
            height: header.height,
            header_hash: hash.to_vec(),
            mined_timestamp: header.timestamp.as_u64(),
        };

        Ok(utxo_block_response)
    }

    #[allow(clippy::too_many_lines)]
    async fn fetch_utxos(&self, request: SyncUtxosByBlockRequest) -> Result<SyncUtxosByBlockResponseV0, Error> {
        // validate and fetch inputs
        request.validate()?;

        let hash = request.start_header_hash.clone().try_into()?;
        let start_header = self
            .db()
            .fetch_header_by_block_hash(hash)
            .await?
            .ok_or_else(|| Error::StartHeaderHashNotFound)?;

        let tip_header = self.db.fetch_tip_header().await?;
        // we only allow wallets to ask for a max of 100 blocks at a time and we want to cache the queries to ensure
        // they are in batch of 100 and we want to ensure they request goes to the nearest 100 block height so
        // we can cache all wallet's queries
        let increase = ((start_header.height + WALLET_MAX_BLOCKS_PER_REQUEST) / WALLET_MAX_BLOCKS_PER_REQUEST) *
            WALLET_MAX_BLOCKS_PER_REQUEST;
        let end_height = cmp::min(tip_header.header().height, increase);
        // pagination
        let start_header_height = start_header.height + (request.page * request.limit);
        if start_header_height > tip_header.header().height {
            return Err(Error::HeaderHeightMismatch {
                start_height: start_header.height,
                end_height: tip_header.header().height,
            });
        }
        let start_header = self
            .db
            .fetch_header(start_header_height)
            .await?
            .ok_or_else(|| Error::HeaderNotFound {
                height: start_header_height,
            })?;
        // fetch utxos
        let mut utxos = vec![];
        let next_page_start_height = start_header.height.saturating_add(request.limit);
        let mut current_header = start_header;
        let mut fetched_chunks = 0;
        let spending_end_header_hash = self
            .db
            .fetch_header(
                tip_header
                    .header()
                    .height
                    .saturating_sub(SYNC_UTXOS_SPEND_TIP_SAFETY_LIMIT),
            )
            .await?
            .ok_or_else(|| Error::HeaderNotFound {
                height: tip_header
                    .header()
                    .height
                    .saturating_sub(SYNC_UTXOS_SPEND_TIP_SAFETY_LIMIT),
            })?
            .hash();
        let next_header_to_request;
        let mut has_next_page = true;
        loop {
            let current_header_hash = current_header.hash();
            trace!(
                target: LOG_TARGET,
                "current header = {} ({})",
                current_header.height,
                current_header_hash.to_hex()
            );
            let outputs = if request.exclude_spent {
                self.db
                    .fetch_outputs_in_block_with_spend_state(current_header_hash, Some(spending_end_header_hash))
                    .await?
                    .into_iter()
                    .filter(|(_, spent)| !spent)
                    .map(|(output, _spent)| output)
                    .collect::<Vec<TransactionOutput>>()
            } else {
                self.db
                    .fetch_outputs_in_block_with_spend_state(current_header_hash, None)
                    .await?
                    .into_iter()
                    .map(|(output, _spent)| output)
                    .collect::<Vec<TransactionOutput>>()
            };
            let mut inputs = if request.exclude_inputs {
                Vec::new()
            } else {
                self.db
                    .fetch_inputs_in_block(current_header_hash)
                    .await?
                    .into_iter()
                    .map(|input| input.output_hash())
                    .collect::<Vec<FixedHash>>()
            };
            if outputs.is_empty() && inputs.is_empty() {
                // No outputs or inputs in this block, put empty placeholder here so wallet knows this height has been
                // scanned This can happen if all the outputs are spent and exclude_spent is true
                let block_response = BlockUtxoInfo {
                    outputs: Vec::new(),
                    inputs: Vec::new(),
                    height: current_header.height,
                    header_hash: current_header_hash.to_vec(),
                    mined_timestamp: current_header.timestamp.as_u64(),
                };
                utxos.push(block_response);
            }
            for output_chunk in outputs.chunks(self.max_utxo_chunk_size) {
                let inputs_to_send = if inputs.is_empty() {
                    Vec::new()
                } else {
                    let num_to_drain = inputs.len().min(self.max_utxo_chunk_size);
                    inputs.drain(..num_to_drain).map(|h| h.to_vec()).collect()
                };

                let output_block_response = BlockUtxoInfo {
                    outputs: output_chunk
                        .iter()
                        .map(|output| MinimalUtxoSyncInfo {
                            output_hash: output.hash().to_vec(),
                            commitment: output.commitment().to_vec(),
                            encrypted_data: output.encrypted_data().as_bytes().to_vec(),
                            sender_offset_public_key: output.sender_offset_public_key.to_vec(),
                        })
                        .collect(),
                    inputs: inputs_to_send,
                    height: current_header.height,
                    header_hash: current_header_hash.to_vec(),
                    mined_timestamp: current_header.timestamp.as_u64(),
                };
                utxos.push(output_block_response);
                fetched_chunks += 1;
            }
            // We might still have inputs left to send if they are more than the outputs
            for input_chunk in inputs.chunks(self.max_utxo_chunk_size) {
                let output_block_response = BlockUtxoInfo {
                    outputs: Vec::new(),
                    inputs: input_chunk.iter().map(|h| h.to_vec()).collect::<Vec<_>>().to_vec(),
                    height: current_header.height,
                    header_hash: current_header_hash.to_vec(),
                    mined_timestamp: current_header.timestamp.as_u64(),
                };
                utxos.push(output_block_response);
                fetched_chunks += 1;
            }

            if current_header.height >= tip_header.header().height {
                next_header_to_request = vec![];
                has_next_page = (end_height.saturating_sub(current_header.height)) > 0;
                break;
            }
            if fetched_chunks > request.limit {
                next_header_to_request = current_header_hash.to_vec();
                // This is a special edge case, our request has reached the page limit, but we are also not done with
                // the block. We also dont want to split up the block over two requests. So we need to ensure that we
                // remove the partial block we added so that it can be requested fully in the next request. We also dont
                // want to get in a loop where the block cannot fit into the page limit, so if the block is the same as
                // the first one, we just send it as is, partial. If not we remove it and let it be sent in the next
                // request.
                if utxos.first().ok_or(Error::General(anyhow::anyhow!("No utxos founds")))? // should never happen as we always add at least one block
                    .header_hash ==
                    current_header_hash.to_vec()
                {
                    // special edge case where the first block is also the last block we can send, so we just send it as
                    // is, partial
                    break;
                }
                while !utxos.is_empty() &&
                    utxos.last().ok_or(Error::General(anyhow::anyhow!("No utxos found")))? // should never happen as we always add at least one block
                    .header_hash ==
                        current_header_hash.to_vec()
                {
                    utxos.pop();
                }
                has_next_page = false;
                break;
            }
            if current_header.height + 1 > end_height {
                next_header_to_request = current_header.hash().to_vec();
                has_next_page = (end_height.saturating_sub(current_header.height)) > 0;
                break; // Stop if we reach the end height
            }
            current_header =
                self.db
                    .fetch_header(current_header.height + 1)
                    .await?
                    .ok_or_else(|| Error::HeaderNotFound {
                        height: current_header.height + 1,
                    })?;

            if current_header.height == next_page_start_height {
                // we are on the limit, stop here
                next_header_to_request = current_header.hash().to_vec();
                has_next_page = (end_height.saturating_sub(current_header.height)) > 0;
                break;
            }
        }
        Ok(SyncUtxosByBlockResponseV0 {
            blocks: utxos,
            has_next_page,
            next_header_to_scan: next_header_to_request,
        })
    }
}

#[async_trait::async_trait]
impl<B: BlockchainBackend + 'static> BaseNodeWalletQueryService for Service<B> {
    type Error = Error;

    async fn get_tip_info(&self) -> Result<TipInfoResponse, Self::Error> {
        let state_machine = self.state_machine();
        let status_watch = state_machine.get_status_info_watch();
        let is_synced = match status_watch.borrow().state_info {
            StateInfo::Listening(li) => li.is_synced(),
            _ => false,
        };

        let metadata = self.db.get_chain_metadata().await?;

        Ok(TipInfoResponse {
            metadata: Some(metadata),
            is_synced,
        })
    }

    async fn get_header_by_height(&self, height: u64) -> Result<models::BlockHeader, Self::Error> {
        let result = self
            .db
            .fetch_header(height)
            .await?
            .ok_or(Error::HeaderNotFound { height })?
            .into();
        Ok(result)
    }

    async fn get_height_at_time(&self, epoch_time: u64) -> Result<u64, Self::Error> {
        trace!(target: LOG_TARGET, "requested_epoch_time: {}", epoch_time);
        let tip_header = self.db.fetch_tip_header().await?;

        let mut left_height = 0u64;
        let mut right_height = tip_header.height();

        while left_height <= right_height {
            let mut mid_height = (left_height + right_height) / 2;

            if mid_height == 0 {
                return Ok(0u64);
            }
            // If the two bounds are adjacent then perform the test between the right and left sides
            if left_height == mid_height {
                mid_height = right_height;
            }

            let mid_header = self
                .db
                .fetch_header(mid_height)
                .await?
                .ok_or_else(|| Error::HeaderNotFound { height: mid_height })?;
            let before_mid_header = self
                .db
                .fetch_header(mid_height - 1)
                .await?
                .ok_or_else(|| Error::HeaderNotFound { height: mid_height - 1 })?;
            trace!(
                target: LOG_TARGET,
                "requested_epoch_time: {}, left: {}, mid: {}/{} ({}/{}), right: {}",
                epoch_time,
                left_height,
                mid_height,
                mid_height-1,
                mid_header.timestamp.as_u64(),
                before_mid_header.timestamp.as_u64(),
                right_height
            );
            if epoch_time < mid_header.timestamp.as_u64() && epoch_time >= before_mid_header.timestamp.as_u64() {
                trace!(
                    target: LOG_TARGET,
                    "requested_epoch_time: {}, selected height: {}",
                    epoch_time, before_mid_header.height
                );
                return Ok(before_mid_header.height);
            } else if mid_height == right_height {
                trace!(
                    target: LOG_TARGET,
                    "requested_epoch_time: {epoch_time}, selected height: {right_height}"
                );
                return Ok(right_height);
            } else if epoch_time <= mid_header.timestamp.as_u64() {
                right_height = mid_height;
            } else {
                left_height = mid_height;
            }
        }

        Ok(0u64)
    }

    async fn transaction_query(
        &self,
        signature: crate::base_node::rpc::models::Signature,
    ) -> Result<TxQueryResponse, Self::Error> {
        let signature = signature.try_into().map_err(Error::SignatureConversion)?;

        let response = self.fetch_kernel(signature).await?;

        Ok(response)
    }

    async fn sync_utxos_by_block_v0(
        &self,
        request: SyncUtxosByBlockRequest,
    ) -> Result<SyncUtxosByBlockResponseV0, Self::Error> {
        self.fetch_utxos(request).await
    }

    async fn sync_utxos_by_block_v1(
        &self,
        request: SyncUtxosByBlockRequest,
    ) -> Result<SyncUtxosByBlockResponseV1, Self::Error> {
        let v1 = self.fetch_utxos(request).await?;
        Ok(v1.into())
    }

    async fn get_utxos_by_block(
        &self,
        request: GetUtxosByBlockRequest,
    ) -> Result<GetUtxosByBlockResponse, Self::Error> {
        self.fetch_utxos_by_block(request).await
    }

    async fn get_utxos_mined_info(
        &self,
        request: models::GetUtxosMinedInfoRequest,
    ) -> Result<models::GetUtxosMinedInfoResponse, Self::Error> {
        request.validate()?;

        let mut utxos = vec![];

        let tip_header = self.db().fetch_tip_header().await?;
        for hash in request.hashes {
            let hash = hash.try_into()?;
            let output = self.db().fetch_output(hash).await?;
            if let Some(output) = output {
                utxos.push(models::MinedUtxoInfo {
                    utxo_hash: hash.to_vec(),
                    mined_in_hash: output.header_hash.to_vec(),
                    mined_in_height: output.mined_height,
                    mined_in_timestamp: output.mined_timestamp,
                });
            }
        }

        Ok(models::GetUtxosMinedInfoResponse {
            utxos,
            best_block_hash: tip_header.hash().to_vec(),
            best_block_height: tip_header.height(),
        })
    }

    async fn get_utxos_deleted_info(
        &self,
        request: models::GetUtxosDeletedInfoRequest,
    ) -> Result<models::GetUtxosDeletedInfoResponse, Self::Error> {
        request.validate()?;

        let mut utxos = vec![];

        let must_include_header = request.must_include_header.clone().try_into()?;
        if self
            .db()
            .fetch_header_by_block_hash(must_include_header)
            .await?
            .is_none()
        {
            return Err(Error::HeaderHashNotFound);
        }

        let tip_header = self.db().fetch_tip_header().await?;
        for hash in request.hashes {
            let hash = hash.try_into()?;
            let output = self.db().fetch_output(hash).await?;

            if let Some(output) = output {
                // is it still unspent?
                let input = self.db().fetch_input(hash).await?;
                if let Some(i) = input {
                    utxos.push(models::DeletedUtxoInfo {
                        utxo_hash: hash.to_vec(),
                        found_in_header: Some((output.mined_height, output.header_hash.to_vec())),
                        spent_in_header: Some((i.spent_height, i.header_hash.to_vec())),
                    });
                } else {
                    utxos.push(models::DeletedUtxoInfo {
                        utxo_hash: hash.to_vec(),
                        found_in_header: Some((output.mined_height, output.header_hash.to_vec())),
                        spent_in_header: None,
                    });
                }
            } else {
                utxos.push(models::DeletedUtxoInfo {
                    utxo_hash: hash.to_vec(),
                    found_in_header: None,
                    spent_in_header: None,
                });
            }
        }

        Ok(models::GetUtxosDeletedInfoResponse {
            utxos,
            best_block_hash: tip_header.hash().to_vec(),
            best_block_height: tip_header.height(),
        })
    }

    async fn generate_kernel_merkle_proof(
        &self,
        excess_sig: types::CompressedSignature,
    ) -> Result<GenerateKernelMerkleProofResponse, Self::Error> {
        let proof = self.db().generate_kernel_merkle_proof(excess_sig).await?;
        Ok(GenerateKernelMerkleProofResponse {
            encoded_merkle_proof: bincode::serialize(&proof.merkle_proof).map_err(Error::general)?,
            block_hash: proof.block_hash,
            leaf_index: proof.leaf_index.value() as u64,
        })
    }

    async fn get_utxo(&self, request: models::GetUtxoRequest) -> Result<Option<TransactionOutput>, Self::Error> {
        let hash: FixedHash = request.output_hash.try_into().map_err(Error::general)?;
        let outputs = self.db().fetch_outputs_with_spend_status_at_tip(vec![hash]).await?;
        let output = match outputs.first() {
            Some(Some((output, _spent))) => Some(output.clone()),
            _ => return Err(Error::OutputNotFound),
        };
        Ok(output)
    }

    async fn get_mempool_fee_per_gram_stats(&self, count: usize) -> Result<Vec<models::FeePerGramStat>, Self::Error> {
        if count > 20 {
            return Err(Error::general(anyhow::anyhow!(
                "count must be less than or equal to 20"
            )));
        }

        let metadata = self.db.get_chain_metadata().await?;
        let stats = self
            .mempool()
            .get_fee_per_gram_stats(count, metadata.best_block_height())
            .await
            .map_err(Error::general)?;

        Ok(stats)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)]
    use tari_common::configuration::Network;
    use tari_shutdown::Shutdown;

    use super::*;
    use crate::test_helpers::blockchain::create_new_blockchain_with_network;
    fn make_state_machine_handle() -> StateMachineHandle {
        use tokio::sync::{broadcast, watch};
        let (state_tx, _state_rx) = broadcast::channel(10);
        let (_status_tx, status_rx) =
            watch::channel(crate::base_node::state_machine_service::states::StatusInfo::new());
        let shutdown = Shutdown::new();
        StateMachineHandle::new(state_tx, status_rx, shutdown.to_signal())
    }

    fn make_mempool_handle() -> MempoolHandle {
        use crate::mempool::test_utils::mock::create_mempool_service_mock;
        let (handle, _state) = create_mempool_service_mock();
        handle
    }

    async fn make_service() -> Service<crate::test_helpers::blockchain::TempDatabase> {
        let db = create_new_blockchain_with_network(Network::LocalNet);
        let adb = AsyncBlockchainDb::from(db);
        let state_machine = make_state_machine_handle();
        let mempool = make_mempool_handle();
        Service::new(adb, state_machine, mempool)
    }

    #[tokio::test]
    async fn fetch_utxos_start_header_not_found() {
        let service = make_service().await;
        let req = SyncUtxosByBlockRequest {
            start_header_hash: vec![0xAB; 32],
            limit: 4,
            page: 0,
            exclude_spent: false,
            exclude_inputs: false,
            version: 0,
        };
        let err = service.fetch_utxos(req).await.unwrap_err();
        match err {
            Error::StartHeaderHashNotFound => {},
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn fetch_utxos_header_height_mismatch() {
        let service = make_service().await;
        let genesis = service.db().fetch_header(0).await.unwrap().unwrap();
        // page * limit moves start height beyond tip (0)
        let req = SyncUtxosByBlockRequest {
            start_header_hash: genesis.hash().to_vec(),
            limit: 1,
            page: 1,
            exclude_spent: false,
            exclude_inputs: false,
            version: 0,
        };
        let err = service.fetch_utxos(req).await.unwrap_err();
        match err {
            Error::HeaderHeightMismatch { .. } => {},
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn fetch_utxos_paginates_results() {
        use crate::test_helpers::blockchain::create_main_chain;

        // Build a small chain: GB -> A -> B -> C
        let db = create_new_blockchain_with_network(Network::LocalNet);
        let (_names, chain) = create_main_chain(&db, block_specs!(["A->GB"], ["B->A"], ["C->B"], ["D->C"]));

        // Construct the service over this DB
        let adb = AsyncBlockchainDb::from(db);
        let state_machine = make_state_machine_handle();
        let mempool = make_mempool_handle();
        let service = Service::new(adb, state_machine, mempool);

        // Use genesis as the start header hash
        let genesis = service.db().fetch_header(0).await.unwrap().unwrap();
        let g_hash = genesis.hash().to_vec();

        // Page 0, limit 1: should return only A (height 1) and next header should be B
        // genesis block in local net has 0 inputs and outputs
        let resp0 = service
            .fetch_utxos(SyncUtxosByBlockRequest {
                start_header_hash: g_hash.clone(),
                limit: 1,
                page: 0,
                exclude_spent: false,
                exclude_inputs: false,
                version: 0,
            })
            .await
            .expect("fetch_utxos page 0 should succeed");

        assert_eq!(resp0.blocks.len(), 1, "expected exactly one block in first page");
        assert_eq!(resp0.blocks[0].height, 0, "first page should start at genesis height");

        // Expect next_header_to_scan to be the hash of block A (height 1)
        let a_hash = chain.get("A").unwrap().hash().to_vec();
        assert_eq!(resp0.next_header_to_scan, a_hash, "next header should point to A");

        // Page 1, limit 1: should return only block A (height 1) and next header should be B
        let resp1 = service
            .fetch_utxos(SyncUtxosByBlockRequest {
                start_header_hash: g_hash.clone(),
                limit: 1,
                page: 1,
                exclude_spent: false,
                exclude_inputs: false,
                version: 0,
            })
            .await
            .expect("fetch_utxos page 1 should succeed");

        assert_eq!(resp1.blocks.len(), 1, "expected exactly one block in second page");
        assert_eq!(resp1.blocks[0].height, 1, "second page should start at height 1 (A)");

        // Expect next_header_to_scan to be the hash of block B (height 2)
        let b_hash = chain.get("B").unwrap().hash().to_vec();
        assert_eq!(resp1.next_header_to_scan, b_hash, "next header should point to B");

        // Page 2, limit 1: should return only block B (height 2) and next header should be C
        let resp2 = service
            .fetch_utxos(SyncUtxosByBlockRequest {
                start_header_hash: g_hash.clone(),
                limit: 1,
                page: 2,
                exclude_spent: false,
                exclude_inputs: false,
                version: 0,
            })
            .await
            .expect("fetch_utxos page 2 should succeed");

        assert_eq!(resp2.blocks.len(), 1, "expected exactly one block in third page");
        assert_eq!(resp2.blocks[0].height, 2, "third page should start at height 2 (B)");
        let c_hash = chain.get("C").unwrap().hash().to_vec();
        assert_eq!(resp2.next_header_to_scan, c_hash, "next header should point to C");

        // Page 3, limit 1: should return only block C (height 3) and next header should be empty (tip reached)
        let resp3 = service
            .fetch_utxos(SyncUtxosByBlockRequest {
                start_header_hash: g_hash.clone(),
                limit: 1,
                page: 3,
                exclude_spent: false,
                exclude_inputs: false,
                version: 0,
            })
            .await
            .expect("fetch_utxos page 3 should succeed");

        assert_eq!(resp3.blocks.len(), 1, "expected exactly one block in fourth page");
        assert_eq!(resp3.blocks[0].height, 3, "fourth page should start at height 3 (C)");
        let d_hash = chain.get("D").unwrap().hash().to_vec();
        assert_eq!(resp3.next_header_to_scan, d_hash, "next header should point to D");

        // Page 4, limit 1: should return only block C (height 3) and next header should be empty (tip reached)
        let resp4 = service
            .fetch_utxos(SyncUtxosByBlockRequest {
                start_header_hash: g_hash.clone(),
                limit: 1,
                page: 4,
                exclude_spent: false,
                exclude_inputs: false,
                version: 0,
            })
            .await
            .expect("fetch_utxos page 3 should succeed");

        assert_eq!(resp4.blocks.len(), 1, "expected exactly one block in fourth page");
        assert_eq!(resp4.blocks[0].height, 4, "fourth page should start at height 4 (D)");
        assert!(resp4.next_header_to_scan.is_empty(), "no next header at tip");
        let resp5 = service
            .fetch_utxos(SyncUtxosByBlockRequest {
                start_header_hash: g_hash,
                limit: 2,
                page: 0,
                exclude_spent: false,
                exclude_inputs: false,
                version: 0,
            })
            .await
            .expect("fetch_utxos should succeed");

        assert_eq!(resp5.blocks.len(), 2, "expected 2 blocks");
        assert_eq!(resp5.blocks[0].height, 0, "Should be block (GB)");
        assert_eq!(resp5.blocks[1].height, 1, "Should be block (A)");
        let b_hash = chain.get("B").unwrap().hash().to_vec();
        assert_eq!(resp5.next_header_to_scan, b_hash, "next header should point to B");
        assert!(resp5.has_next_page, "Should have more pages");
    }

    #[tokio::test]
    async fn large_fetch_utxo_paginates_results() {
        use crate::test_helpers::blockchain::create_main_chain;

        // Build a small chain: GB -> A -> B -> C
        let db = create_new_blockchain_with_network(Network::LocalNet);
        let (_names, chain) = create_main_chain(
            &db,
            block_specs!(
                ["1->GB"],
                ["2->1"],
                ["3->2"],
                ["4->3"],
                ["5->4"],
                ["6->5"],
                ["7->6"],
                ["8->7"],
                ["9->8"],
                ["10->9"],
                ["11->10"],
                ["12->11"]
            ),
        );

        // Construct the service over this DB
        let adb = AsyncBlockchainDb::from(db);
        let state_machine = make_state_machine_handle();
        let mempool = make_mempool_handle();
        let service = Service::new(adb, state_machine, mempool);

        // Use genesis as the start header hash
        let genesis = service.db().fetch_header(0).await.unwrap().unwrap();
        let g_hash = genesis.hash().to_vec();

        let resp = service
            .fetch_utxos(SyncUtxosByBlockRequest {
                start_header_hash: g_hash.clone(),
                limit: 10,
                page: 0,
                exclude_spent: false,
                exclude_inputs: false,
                version: 0,
            })
            .await
            .expect("fetch_utxos should succeed");

        assert_eq!(resp.blocks.len(), 10, "expected 10 blocks");
        assert_eq!(resp.blocks[0].height, 0, "Should be block (0)");
        assert_eq!(resp.blocks[1].height, 1, "Should be block (1)");
        assert_eq!(resp.blocks[2].height, 2, "Should be block (2)");
        assert_eq!(resp.blocks[3].height, 3, "Should be block (3)");
        assert_eq!(resp.blocks[4].height, 4, "Should be block (4)");
        assert_eq!(resp.blocks[5].height, 5, "Should be block (5)");
        assert_eq!(resp.blocks[6].height, 6, "Should be block (6)");
        assert_eq!(resp.blocks[7].height, 7, "Should be block (7)");
        assert_eq!(resp.blocks[8].height, 8, "Should be block (8)");
        assert_eq!(resp.blocks[9].height, 9, "Should be block (9)");
        let next_hash = chain.get("10").unwrap().hash().to_vec();
        assert_eq!(resp.next_header_to_scan, next_hash, "next header should point to 10");
        assert!(resp.has_next_page, "Should have more pages");
        let resp = service
            .fetch_utxos(SyncUtxosByBlockRequest {
                start_header_hash: g_hash,
                limit: 10,
                page: 1,
                exclude_spent: false,
                exclude_inputs: false,
                version: 0,
            })
            .await
            .expect("fetch_utxos should succeed");
        assert_eq!(resp.blocks.len(), 3, "expected 3 blocks");
        assert_eq!(resp.blocks[0].height, 10, "Should be block (10)");
        assert_eq!(resp.blocks[1].height, 11, "Should be block (11)");
        assert_eq!(resp.blocks[2].height, 12, "Should be block (12)");
        assert!(resp.next_header_to_scan.is_empty(), "Should be empty");
        assert!(!resp.has_next_page, "Should not have more pages");
    }

    // this will only run and work in esmeralda
    #[cfg(tari_target_network_testnet)]
    #[tokio::test]
    async fn large_utxo_handled_correctly() {
        use crate::test_helpers::blockchain::create_main_chain;

        // Build a small chain: GB -> A -> B -> C
        let db = create_new_blockchain_with_network(Network::Esmeralda);
        let (_names, _chain) = create_main_chain(
            &db,
            block_specs!(
                ["1->GB"],
                ["2->1"],
                ["3->2"],
                ["4->3"],
                ["5->4"],
                ["6->5"],
                ["7->6"],
                ["8->7"],
                ["9->8"],
                ["10->9"],
            ),
        );

        // Construct the service over this DB
        let adb = AsyncBlockchainDb::from(db);
        let state_machine = make_state_machine_handle();
        let mempool = make_mempool_handle();
        let mut service = Service::new(adb, state_machine, mempool);
        service.max_utxo_chunk_size = 500; // set small chunk size for testing

        // Use genesis as the start header hash
        let genesis = service.db().fetch_header(0).await.unwrap().unwrap();
        let g_hash = genesis.hash().to_vec();

        let resp = service
            .fetch_utxos(SyncUtxosByBlockRequest {
                start_header_hash: g_hash.clone(),
                limit: 5,
                page: 0,
                exclude_spent: false,
                exclude_inputs: false,
                version: 0,
            })
            .await
            .expect("fetch_utxos should succeed");

        assert_eq!(resp.blocks.len(), 5, "expected 5 blocks");
        assert_eq!(resp.blocks[0].height, 0, "Should be block (0)");
        assert_eq!(resp.blocks[1].height, 0, "Should be block (0)");
        assert_eq!(resp.blocks[2].height, 1, "Should be block (1)");
        assert_eq!(resp.blocks[3].height, 2, "Should be block (2)");
        assert_eq!(resp.blocks[4].height, 3, "Should be block (3)");
        let header_4 = service.db().fetch_header(4).await.unwrap().unwrap();
        assert_eq!(
            header_4.hash().to_vec(),
            resp.next_header_to_scan,
            "next header should point to 4"
        );
        assert!(!resp.has_next_page, "Should have no more pages");
    }
}
