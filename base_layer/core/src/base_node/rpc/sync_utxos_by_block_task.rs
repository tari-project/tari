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

use std::{convert::TryInto, time::Instant};

use log::*;
use tari_p2p::proto::base_node::{SyncUtxosByBlockRequest, SyncUtxosByBlockResponse};
use tari_rpc_framework::{RpcStatus, RpcStatusResultExt};
use tari_utilities::hex::Hex;
use tokio::{sync::mpsc, task};

use crate::{
    blocks::BlockHeader,
    chain_storage::{async_db::AsyncBlockchainDb, BlockchainBackend},
};
use tari_p2p::proto;

const LOG_TARGET: &str = "c::base_node::sync_rpc::sync_utxo_by_block_task";

pub(crate) struct SyncUtxosByBlockTask<B> {
    db: AsyncBlockchainDb<B>,
}

impl<B> SyncUtxosByBlockTask<B>
where
    B: BlockchainBackend + 'static,
{
    pub(crate) fn new(db: AsyncBlockchainDb<B>) -> Self {
        Self { db }
    }

    pub(crate) async fn run(
        self,
        request: SyncUtxosByBlockRequest,
        mut tx: mpsc::Sender<Result<SyncUtxosByBlockResponse, RpcStatus>>,
    ) -> Result<(), RpcStatus> {
        let hash = request
            .start_header_hash
            .clone()
            .try_into()
            .rpc_status_internal_error(LOG_TARGET)?;
        let start_header = self
            .db
            .fetch_header_by_block_hash(hash)
            .await
            .rpc_status_internal_error(LOG_TARGET)?
            .ok_or_else(|| RpcStatus::not_found("Start header hash is was not found"))?;
        let hash = request
            .end_header_hash
            .clone()
            .try_into()
            .rpc_status_internal_error(LOG_TARGET)?;
        let end_header = self
            .db
            .fetch_header_by_block_hash(hash)
            .await
            .rpc_status_internal_error(LOG_TARGET)?
            .ok_or_else(|| RpcStatus::not_found("End header hash is was not found"))?;

        if start_header.height > end_header.height {
            return Err(RpcStatus::bad_request(&format!(
                "start header height {} cannot be greater than the end header height ({})",
                start_header.height, end_header.height
            )));
        }

        task::spawn(async move {
            if let Err(err) = self.start_streaming(&mut tx, start_header, end_header).await {
                let _result = tx.send(Err(err)).await;
            }
        });

        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    async fn start_streaming(
        &self,
        tx: &mut mpsc::Sender<Result<SyncUtxosByBlockResponse, RpcStatus>>,
        start_header: BlockHeader,
        end_header: BlockHeader,
    ) -> Result<(), RpcStatus> {
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

            let outputs_with_statuses = self
                .db
                .fetch_outputs_in_block_with_spend_state(current_header.hash(), None)
                .await
                .rpc_status_internal_error(LOG_TARGET)?;
            let outputs = outputs_with_statuses
                .into_iter()
                .map(|(output, _spent)| output.try_into())
                .collect::<Result<Vec<proto::types::TransactionOutput>, String>>()
                .map_err(|err| RpcStatus::general(&err))?;

            debug!(
                target: LOG_TARGET,
                "Streaming {} UTXO(s) for block #{} (Hash: {})",
                outputs.len(),
                current_header.height,
                current_header_hash.to_hex(),
            );

            for output_chunk in outputs.chunks(2000) {
                let output_block_response = SyncUtxosByBlockResponse {
                    outputs: output_chunk.to_vec(),
                    height: current_header.height,
                    header_hash: current_header_hash.to_vec(),
                    mined_timestamp: current_header.timestamp.as_u64(),
                };
                // Ensure task stops if the peer prematurely stops their RPC session
                if tx.send(Ok(output_block_response)).await.is_err() {
                    break;
                }
            }
            if outputs.is_empty() {
                // if its empty, we need to send an empty vec of outputs.
                let utxo_block_response = SyncUtxosByBlockResponse {
                    outputs: Vec::new(),
                    height: current_header.height,
                    header_hash: current_header_hash.to_vec(),
                    mined_timestamp: current_header.timestamp.as_u64(),
                };
                // Ensure task stops if the peer prematurely stops their RPC session
                if tx.send(Ok(utxo_block_response)).await.is_err() {
                    break;
                }
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
                .rpc_status_internal_error(LOG_TARGET)?
                .ok_or_else(|| {
                    RpcStatus::general(&format!(
                        "Potential data consistency issue: header {} not found",
                        current_header.height + 1
                    ))
                })?;
        }

        debug!(
            target: LOG_TARGET,
            "UTXO sync by block completed to UTXO {} (Header hash = {})",
            current_header.output_smt_size,
            current_header.hash().to_hex()
        );

        Ok(())
    }
}
