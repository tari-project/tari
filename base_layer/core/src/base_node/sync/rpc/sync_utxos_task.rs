// Copyright 2021. The Tari Project
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

use crate::{
    blocks::BlockHeader,
    chain_storage::{async_db::AsyncBlockchainDb, BlockchainBackend},
    proto,
    proto::base_node::{SyncUtxo, SyncUtxosRequest, SyncUtxosResponse},
};
use croaring::Bitmap;
use log::*;
use std::{cmp, sync::Arc, time::Instant};
use tari_comms::{protocol::rpc::RpcStatus, utils};
use tari_crypto::tari_utilities::{hex::Hex, Hashable};
use tokio::{sync::mpsc, task};

const LOG_TARGET: &str = "c::base_node::sync_rpc::sync_utxo_task";

pub(crate) struct SyncUtxosTask<B> {
    db: AsyncBlockchainDb<B>,
}

impl<B> SyncUtxosTask<B>
where B: BlockchainBackend + 'static
{
    pub(crate) fn new(db: AsyncBlockchainDb<B>) -> Self {
        Self { db }
    }

    pub(crate) async fn run(
        self,
        request: SyncUtxosRequest,
        mut tx: mpsc::Sender<Result<SyncUtxosResponse, RpcStatus>>,
    ) -> Result<(), RpcStatus> {
        let start_header = self
            .db
            .fetch_header_containing_utxo_mmr(request.start + 1)
            .await
            .map_err(RpcStatus::log_internal_error(LOG_TARGET))?;

        let end_header = self
            .db
            .fetch_header_by_block_hash(request.end_header_hash.clone())
            .await
            .map_err(RpcStatus::log_internal_error(LOG_TARGET))?
            .ok_or_else(|| RpcStatus::not_found("End header hash is was not found"))?;

        if start_header.height() > end_header.height {
            return Err(RpcStatus::bad_request(format!(
                "start header height {} cannot be greater than the end header height ({})",
                start_header.height(),
                end_header.height
            )));
        }

        let (skip_outputs, prev_utxo_mmr_size) = if start_header.height() == 0 {
            (request.start, 0)
        } else {
            let prev_header = self
                .db
                .fetch_header_by_block_hash(start_header.header().prev_hash.clone())
                .await
                .map_err(RpcStatus::log_internal_error(LOG_TARGET))?
                .ok_or_else(|| RpcStatus::not_found("Previous start header hash is was not found"))?;

            let skip = request.start.checked_sub(prev_header.output_mmr_size)
                // This is a data inconsistency because fetch_header_containing_utxo_mmr returned the header we are basing this on
                .ok_or_else(|| RpcStatus::general(format!("Data inconsistency: output mmr size of header at {} was more than the start index {}", prev_header.height, request.start)))?;
            (skip, prev_header.output_mmr_size)
        };

        // we need to fetch the spent bitmap for the height the client requested
        let bitmap = self
            .db
            .fetch_complete_deleted_bitmap_at(end_header.hash())
            .await
            .map_err(|_| {
                RpcStatus::general(format!(
                    "Could not get tip deleted bitmap at hash {}",
                    end_header.hash().to_hex()
                ))
            })?
            .into_bitmap();
        let bitmap = Arc::new(bitmap);

        let include_pruned_utxos = request.include_pruned_utxos;
        let include_deleted_bitmaps = request.include_deleted_bitmaps;
        task::spawn(async move {
            if let Err(err) = self
                .start_streaming(
                    &mut tx,
                    start_header.into_header(),
                    skip_outputs,
                    prev_utxo_mmr_size,
                    end_header,
                    bitmap,
                    include_pruned_utxos,
                    include_deleted_bitmaps,
                )
                .await
            {
                let _ = tx.send(Err(err)).await;
            }
        });

        Ok(())
    }

    async fn start_streaming(
        &self,
        tx: &mut mpsc::Sender<Result<SyncUtxosResponse, RpcStatus>>,
        mut current_header: BlockHeader,
        mut skip_outputs: u64,
        mut prev_utxo_mmr_size: u64,
        end_header: BlockHeader,
        bitmap: Arc<Bitmap>,
        include_pruned_utxos: bool,
        include_deleted_bitmaps: bool,
    ) -> Result<(), RpcStatus> {
        debug!(
            target: LOG_TARGET,
            "Starting stream task with current_header: {}, skip_outputs: {}, prev_utxo_mmr_size: {}, end_header: {}, \
             include_pruned_utxos: {:?}, include_deleted_bitmaps: {:?}",
            current_header.hash().to_hex(),
            skip_outputs,
            prev_utxo_mmr_size,
            end_header.hash().to_hex(),
            include_pruned_utxos,
            include_deleted_bitmaps
        );
        while current_header.height <= end_header.height {
            let timer = Instant::now();
            let current_header_hash = current_header.hash();

            debug!(
                target: LOG_TARGET,
                "current header = {} ({})",
                current_header.height,
                current_header_hash.to_hex()
            );

            let start = prev_utxo_mmr_size + skip_outputs;
            let end = current_header.output_mmr_size;

            if tx.is_closed() {
                debug!(target: LOG_TARGET, "Exiting sync_utxos early because client has gone",);
                break;
            }

            let (utxos, deleted_diff) = self
                .db
                .fetch_utxos_in_block(current_header.hash(), bitmap.clone())
                .await
                .map_err(RpcStatus::log_internal_error(LOG_TARGET))?;
            debug!(
                target: LOG_TARGET,
                "Streaming UTXO(s) {}-{} ({}) for block #{}. Deleted diff len = {}",
                start,
                end,
                utxos.len(),
                current_header.height,
                deleted_diff.cardinality(),
            );
            let utxos = utxos
                    .into_iter()
                    .enumerate()
                    .skip(skip_outputs as usize)
                    // Only include pruned UTXOs if include_pruned_utxos is true
                    .filter(|(_, utxo)| include_pruned_utxos || !utxo.is_pruned())
                    .map(|(i, utxo)| {
                        SyncUtxosResponse {
                            utxo_or_deleted: Some(proto::base_node::sync_utxos_response::UtxoOrDeleted::Utxo(
                                SyncUtxo::from(utxo)
                            )),
                            mmr_index: start + i  as u64,
                        }
                    })
                    .map(Ok);

            // Ensure task stops if the peer prematurely stops their RPC session
            if utils::mpsc::send_all(tx, utxos).await.is_err() {
                break;
            }

            // We only want to skip the first block UTXOs
            skip_outputs = 0;

            if include_deleted_bitmaps {
                let bitmaps = SyncUtxosResponse {
                    utxo_or_deleted: Some(proto::base_node::sync_utxos_response::UtxoOrDeleted::DeletedDiff(
                        deleted_diff.serialize(),
                    )),
                    mmr_index: 0,
                };

                if tx.send(Ok(bitmaps)).await.is_err() {
                    break;
                }
            }
            debug!(
                target: LOG_TARGET,
                "Streamed utxos {} to {} in {:.2?} (including stream backpressure)",
                start,
                end,
                timer.elapsed()
            );

            prev_utxo_mmr_size = current_header.output_mmr_size;
            current_header = self
                .db
                .fetch_header(current_header.height + 1)
                .await
                .map_err(RpcStatus::log_internal_error(LOG_TARGET))?
                .ok_or_else(|| {
                    RpcStatus::general(format!(
                        "Potential data consistency issue: header {} not found",
                        current_header.height + 1
                    ))
                })?;
        }

        debug!(
            target: LOG_TARGET,
            "UTXO sync completed to UTXO {} (Header hash = {})",
            current_header.output_mmr_size,
            current_header.hash().to_hex()
        );

        Ok(())
    }
}
