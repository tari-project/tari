// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::cmp;

use log::trace;
use serde_valid::{validation, Validate};
use tari_common_types::{types, types::FixedHashSizeError};
use tari_utilities::{hex::Hex, ByteArray, ByteArrayError};
use thiserror::Error;

use crate::{
    base_node::{
        rpc::{
            models::{
                self,
                BlockUtxoInfo,
                GetUtxosByBlockRequest,
                GetUtxosByBlockResponse,
                MinimalUtxoSyncInfo,
                SyncUtxosByBlockRequest,
                SyncUtxosByBlockResponse,
                TipInfoResponse,
                TxLocation,
                TxQueryResponse,
            },
            BaseNodeWalletQueryService,
        },
        state_machine_service::states::StateInfo,
        StateMachineHandle,
    },
    chain_storage::{async_db::AsyncBlockchainDb, BlockchainBackend, ChainStorageError},
    mempool::{service::MempoolHandle, MempoolServiceError, TxStorageResponse},
    transactions::transaction_components::TransactionOutput,
};

const LOG_TARGET: &str = "c::bn::rpc::query_service";

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
}

pub struct Service<B> {
    db: AsyncBlockchainDb<B>,
    state_machine: StateMachineHandle,
    mempool: MempoolHandle,
}

impl<B: BlockchainBackend + 'static> Service<B> {
    pub fn new(db: AsyncBlockchainDb<B>, state_machine: StateMachineHandle, mempool: MempoolHandle) -> Self {
        Self {
            db,
            state_machine,
            mempool,
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

    async fn fetch_kernel(&self, signature: types::Signature) -> Result<TxQueryResponse, Error> {
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
    async fn fetch_utxos(&self, request: SyncUtxosByBlockRequest) -> Result<SyncUtxosByBlockResponse, Error> {
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
        let increase = ((start_header.height + 100) / 100) * 100;
        let end_height = cmp::min(tip_header.header().height, increase);

        // pagination
        let start_header_height = start_header.height + (request.page * request.limit);
        let start_header = self
            .db
            .fetch_header(start_header_height)
            .await?
            .ok_or_else(|| Error::HeaderNotFound {
                height: start_header_height,
            })?;

        if start_header.height > tip_header.header().height {
            return Err(Error::HeaderHeightMismatch {
                start_height: start_header.height,
                end_height: tip_header.header().height,
            });
        }

        // fetch utxos
        let mut utxos = vec![];
        let mut current_header = start_header;
        let mut fetched_utxos = 0;
        let next_header_to_request;
        loop {
            let current_header_hash = current_header.hash();

            trace!(
                target: LOG_TARGET,
                "current header = {} ({})",
                current_header.height,
                current_header_hash.to_hex()
            );

            let outputs_with_statuses = self
                .db
                .fetch_outputs_in_block_with_spend_state(current_header.hash(), None)
                .await?;
            let mut inputs = self
                .db
                .fetch_inputs_in_block(current_header.hash())
                .await?
                .into_iter()
                .map(|input| input.output_hash().to_vec())
                .collect::<Vec<Vec<u8>>>();

            let outputs = outputs_with_statuses
                .into_iter()
                .map(|(output, _spent)| output)
                .collect::<Vec<TransactionOutput>>();

            for output_chunk in outputs.chunks(2000) {
                let inputs_to_send = if inputs.is_empty() {
                    Vec::new()
                } else {
                    let num_to_drain = inputs.len().min(2000);
                    inputs.drain(..num_to_drain).collect()
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
            }
            // We might still have inputs left to send if they are more than the outputs
            for input_chunk in inputs.chunks(2000) {
                let output_block_response = BlockUtxoInfo {
                    outputs: Vec::new(),
                    inputs: input_chunk.to_vec(),
                    height: current_header.height,
                    header_hash: current_header_hash.to_vec(),
                    mined_timestamp: current_header.timestamp.as_u64(),
                };
                utxos.push(output_block_response);
            }

            fetched_utxos += 1;

            if current_header.height >= tip_header.header().height {
                next_header_to_request = vec![];
                break;
            }
            if fetched_utxos >= request.limit {
                next_header_to_request = current_header.hash().to_vec();
                break;
            }

            current_header =
                self.db
                    .fetch_header(current_header.height + 1)
                    .await?
                    .ok_or_else(|| Error::HeaderNotFound {
                        height: current_header.height + 1,
                    })?;
            if current_header.height == end_height {
                next_header_to_request = current_header.hash().to_vec();
                break; // Stop if we reach the end height}
            }
        }

        let has_next_page = (end_height.saturating_sub(current_header.height)) > 0;

        Ok(SyncUtxosByBlockResponse {
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
                    "requested_epoch_time: {}, selected height: {}",
                    epoch_time, right_height
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

    async fn sync_utxos_by_block(
        &self,
        request: SyncUtxosByBlockRequest,
    ) -> Result<SyncUtxosByBlockResponse, Self::Error> {
        self.fetch_utxos(request).await
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
}
