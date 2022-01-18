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

use std::{sync::Arc, time::Instant};

use log::*;
use tari_comms::protocol::rpc::RpcStatus;
use tari_crypto::tari_utilities::{hex::Hex, Hashable};
use tokio::{sync::mpsc, task};

use crate::{
    blocks::BlockHeader,
    chain_storage::{async_db::AsyncBlockchainDb, BlockchainBackend, PrunedOutput},
    proto,
    proto::base_node::{SyncUtxosByBlockRequest, SyncUtxosByBlockResponse},
};

const LOG_TARGET: &str = "c::base_node::sync_rpc::sync_utxo_by_block_task";

pub(crate) struct SyncUtxosByBlockTask<B> {
    db: AsyncBlockchainDb<B>,
}

impl<B> SyncUtxosByBlockTask<B>
where B: BlockchainBackend + 'static
{
    pub(crate) fn new(db: AsyncBlockchainDb<B>) -> Self {
        Self { db }
    }

    pub(crate) async fn run(
        self,
        request: SyncUtxosByBlockRequest,
        mut tx: mpsc::Sender<Result<SyncUtxosByBlockResponse, RpcStatus>>,
    ) -> Result<(), RpcStatus> {
        let start_header = self
            .db
            .fetch_header_by_block_hash(request.start_header_hash.clone())
            .await
            .map_err(RpcStatus::log_internal_error(LOG_TARGET))?
            .ok_or_else(|| RpcStatus::not_found("Start header hash is was not found"))?;

        let end_header = self
            .db
            .fetch_header_by_block_hash(request.end_header_hash.clone())
            .await
            .map_err(RpcStatus::log_internal_error(LOG_TARGET))?
            .ok_or_else(|| RpcStatus::not_found("End header hash is was not found"))?;

        if start_header.height > end_header.height {
            return Err(RpcStatus::bad_request(format!(
                "start header height {} cannot be greater than the end header height ({})",
                start_header.height, end_header.height
            )));
        }

        task::spawn(async move {
            if let Err(err) = self.start_streaming(&mut tx, start_header, end_header).await {
                let _ = tx.send(Err(err)).await;
            }
        });

        Ok(())
    }

    async fn start_streaming(
        &self,
        tx: &mut mpsc::Sender<Result<SyncUtxosByBlockResponse, RpcStatus>>,
        start_header: BlockHeader,
        end_header: BlockHeader,
    ) -> Result<(), RpcStatus> {
        let bitmap = self
            .db
            .fetch_complete_deleted_bitmap_at(end_header.hash())
            .await
            .map_err(|err| {
                error!(target: LOG_TARGET, "Failed to get deleted bitmap: {}", err);
                RpcStatus::general(format!(
                    "Could not get deleted bitmap at hash {}",
                    end_header.hash().to_hex()
                ))
            })?
            .into_bitmap();
        let bitmap = Arc::new(bitmap);

        debug!(
            target: LOG_TARGET,
            "Starting stream task with start_header: {} and end_header: {}",
            start_header.hash().to_hex(),
            end_header.hash().to_hex(),
        );

        let mut current_header = start_header;

        loop {
            let timer = Instant::now();
            let current_header_hash = current_header.hash();

            debug!(
                target: LOG_TARGET,
                "current header = {} ({})",
                current_header.height,
                current_header_hash.to_hex()
            );

            if tx.is_closed() {
                debug!(target: LOG_TARGET, "Exiting sync_utxos early because client has gone",);
                break;
            }

            let (utxos, _deleted_diff) = self
                .db
                .fetch_utxos_in_block(current_header.hash(), Some(bitmap.clone()))
                .await
                .map_err(RpcStatus::log_internal_error(LOG_TARGET))?;

            let utxos: Vec<proto::types::TransactionOutput> = utxos
                    .into_iter()
                    .enumerate()
                    // Don't include pruned UTXOs
                    .filter_map(|(_, utxo)| match utxo {
                        PrunedOutput::Pruned{output_hash: _,witness_hash:_} => None,
                        PrunedOutput::NotPruned{output} => Some(output.into()),
                    }).collect();

            debug!(
                target: LOG_TARGET,
                "Streaming {} UTXO(s) for block #{} (Hash: {})",
                utxos.len(),
                current_header.height,
                current_header_hash.to_hex(),
            );

            let utxo_block_response = SyncUtxosByBlockResponse {
                outputs: utxos,
                height: current_header.height,
                header_hash: current_header_hash,
            };
            // Ensure task stops if the peer prematurely stops their RPC session
            if tx.send(Ok(utxo_block_response)).await.is_err() {
                break;
            }

            debug!(
                target: LOG_TARGET,
                "Streamed utxos in {:.2?} (including stream backpressure)",
                timer.elapsed()
            );

            if current_header.height >= end_header.height {
                break;
            }

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
            "UTXO sync by block completed to UTXO {} (Header hash = {})",
            current_header.output_mmr_size,
            current_header.hash().to_hex()
        );

        Ok(())
    }
}
