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

use std::{cmp, sync::Arc, time::Instant};

use log::*;
use tari_comms::{protocol::rpc::RpcStatus, utils};
use tari_crypto::tari_utilities::{hex::Hex, Hashable};
use tokio::sync::mpsc;

use crate::{
    chain_storage::{async_db::AsyncBlockchainDb, BlockchainBackend},
    proto,
    proto::base_node::{SyncUtxo, SyncUtxosRequest, SyncUtxosResponse},
};

const LOG_TARGET: &str = "c::base_node::sync_rpc::sync_utxo_task";

pub(crate) struct SyncUtxosTask<B> {
    db: AsyncBlockchainDb<B>,
    request: SyncUtxosRequest,
}

impl<B> SyncUtxosTask<B>
where B: BlockchainBackend + 'static
{
    pub(crate) fn new(db: AsyncBlockchainDb<B>, request: SyncUtxosRequest) -> Self {
        Self { db, request }
    }

    pub(crate) async fn run(self, mut tx: mpsc::Sender<Result<SyncUtxosResponse, RpcStatus>>) {
        if let Err(err) = self.start_streaming(&mut tx).await {
            let _ = tx.send(Err(err)).await;
        }
    }

    async fn start_streaming(
        &self,
        tx: &mut mpsc::Sender<Result<SyncUtxosResponse, RpcStatus>>,
    ) -> Result<(), RpcStatus> {
        let end_header = self
            .db
            .fetch_header_by_block_hash(self.request.end_header_hash.clone())
            .await
            .map_err(RpcStatus::log_internal_error(LOG_TARGET))?
            .ok_or_else(|| {
                RpcStatus::not_found(format!(
                    "End header hash {} is was not found",
                    self.request.end_header_hash.to_hex()
                ))
            })?;

        if self.request.start > end_header.output_mmr_size - 1 {
            return Err(RpcStatus::bad_request(format!(
                "start index {} cannot be greater than the end header's output MMR size ({})",
                self.request.start, end_header.output_mmr_size
            )));
        }

        let prev_header = self
            .db
            .fetch_header_containing_utxo_mmr(self.request.start)
            .await
            .map_err(RpcStatus::log_internal_error(LOG_TARGET))?;
        let (mut prev_header, _) = prev_header.into_parts();

        if prev_header.height > end_header.height {
            return Err(RpcStatus::bad_request("start index is greater than end index"));
        }
        // we need to construct a temp bitmap for the height the client requested
        let bitmap = self
            .db
            .fetch_complete_deleted_bitmap_at(end_header.hash())
            .await
            .map_err(|_| RpcStatus::not_found("Could not get tip deleted bitmap"))?
            .into_bitmap();

        let bitmap = Arc::new(bitmap);
        loop {
            let timer = Instant::now();
            if prev_header.height == end_header.height {
                break;
            }

            let current_header = self
                .db
                .fetch_header(prev_header.height + 1)
                .await
                .map_err(RpcStatus::log_internal_error(LOG_TARGET))?
                .ok_or_else(|| {
                    RpcStatus::general(format!(
                        "Potential data consistency issue: header {} not found",
                        prev_header.height + 1
                    ))
                })?;

            debug!(
                target: LOG_TARGET,
                "previous header = {} ({}) current header = {} ({})",
                prev_header.height,
                prev_header.hash().to_hex(),
                current_header.height,
                current_header.hash().to_hex()
            );

            let start = cmp::max(self.request.start, prev_header.output_mmr_size);
            let end = current_header.output_mmr_size - 1;

            if tx.is_closed() {
                debug!(target: LOG_TARGET, "Exiting sync_utxos early because client has gone",);
                break;
            }

            debug!(
                target: LOG_TARGET,
                "Streaming UTXOs {}-{} ({}) for block #{}",
                start,
                end,
                end.saturating_sub(start).saturating_add(1),
                current_header.height
            );
            let (utxos, deleted_diff) = self
                .db
                .fetch_utxos_by_mmr_position(start, end, bitmap.clone())
                .await
                .map_err(RpcStatus::log_internal_error(LOG_TARGET))?;
            trace!(
                target: LOG_TARGET,
                "Loaded {} UTXO(s) and |deleted_diff| = {}",
                utxos.len(),
                deleted_diff.cardinality(),
            );
            let utxos = utxos
                    .into_iter()
                    .enumerate()
                    // Only include pruned UTXOs if include_pruned_utxos is true
                    .filter(|(_, utxo)| self.request.include_pruned_utxos || !utxo.is_pruned())
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

            if self.request.include_deleted_bitmaps {
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

            prev_header = current_header;
        }

        debug!(
            target: LOG_TARGET,
            "UTXO sync completed to UTXO {} (Header hash = {})",
            prev_header.output_mmr_size,
            prev_header.hash().to_hex()
        );

        Ok(())
    }
}
