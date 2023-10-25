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

use std::{convert::TryInto, sync::Arc, time::Instant};

use log::*;
use tari_comms::{
    peer_manager::NodeId,
    protocol::rpc::{Request, RpcStatus, RpcStatusResultExt},
    utils,
};
use tari_utilities::hex::Hex;
use tokio::{sync::mpsc, task};

use crate::{
    base_node::metrics,
    blocks::BlockHeader,
    chain_storage::{async_db::AsyncBlockchainDb, BlockchainBackend},
    proto::base_node::{SyncUtxosRequest, SyncUtxosResponse},
};

const LOG_TARGET: &str = "c::base_node::sync_rpc::sync_utxo_task";

pub(crate) struct SyncUtxosTask<B> {
    db: AsyncBlockchainDb<B>,
    peer_node_id: Arc<NodeId>,
}

impl<B> SyncUtxosTask<B>
where B: BlockchainBackend + 'static
{
    pub(crate) fn new(db: AsyncBlockchainDb<B>, peer_node_id: Arc<NodeId>) -> Self {
        Self { db, peer_node_id }
    }

    pub(crate) async fn run(
        self,
        request: Request<SyncUtxosRequest>,
        mut tx: mpsc::Sender<Result<SyncUtxosResponse, RpcStatus>>,
    ) -> Result<(), RpcStatus> {
        let msg = request.into_message();
        let start_hash = msg
            .start_header_hash
            .clone()
            .try_into()
            .rpc_status_bad_request("Invalid header hash")?;

        let start_header = self
            .db
            .fetch_header_by_block_hash(start_hash)
            .await
            .rpc_status_internal_error(LOG_TARGET)?
            .ok_or_else(|| RpcStatus::not_found("Start header hash is was not found"))?;

        let end_hash = msg
            .end_header_hash
            .clone()
            .try_into()
            .rpc_status_bad_request("Invalid header hash")?;

        let end_header = self
            .db
            .fetch_header_by_block_hash(end_hash)
            .await
            .rpc_status_internal_error(LOG_TARGET)?
            .ok_or_else(|| RpcStatus::not_found("End header hash is was not found"))?;

        task::spawn(async move {
            debug!(
                target: LOG_TARGET,
                "Starting UTXO stream for peer '{}'", self.peer_node_id
            );
            if let Err(err) = self.start_streaming(&mut tx, start_header, end_header).await {
                debug!(
                    target: LOG_TARGET,
                    "UTXO stream errored for peer '{}': {}", self.peer_node_id, err
                );
                let _result = tx.send(Err(err)).await;
            }
            debug!(
                target: LOG_TARGET,
                "UTXO stream completed for peer '{}'", self.peer_node_id
            );
            metrics::active_sync_peers().dec();
        });

        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    async fn start_streaming(
        &self,
        tx: &mut mpsc::Sender<Result<SyncUtxosResponse, RpcStatus>>,
        mut current_header: BlockHeader,
        end_header: BlockHeader,
    ) -> Result<(), RpcStatus> {
        debug!(
            target: LOG_TARGET,
            "Starting stream task with current_header: {}, end_header: {},",
            current_header.hash().to_hex(),
            end_header.hash().to_hex(),
        );
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
                debug!(
                    target: LOG_TARGET,
                    "Peer '{}' exited UTXO sync session early", self.peer_node_id
                );
                break;
            }

            let utxos = self
                .db
                .fetch_utxos_in_block(current_header.hash(), Some(end_header.hash()))
                .await
                .rpc_status_internal_error(LOG_TARGET)?;
            debug!(
                target: LOG_TARGET,
                "Streaming UTXO(s) for block #{}.",
                current_header.height,
            );
            if tx.is_closed() {
                debug!(
                    target: LOG_TARGET,
                    "Peer '{}' exited UTXO sync session early", self.peer_node_id
                );
                break;
            }

            let utxos = utxos
                .into_iter()
                .filter_map(|(utxo, spent)| {
                    // We only send unspent utxos
                    if spent {
                        None
                    } else {
                        match utxo.try_into() {
                            Ok(tx_ouput) => Some(Ok(SyncUtxosResponse {
                                output: Some(tx_ouput),
                                mined_header: current_header.hash().to_vec(),
                            })),
                            Err(err) => Some(Err(err)),
                        }
                    }
                })
                .collect::<Result<Vec<SyncUtxosResponse>, String>>()
                .map_err(|err| RpcStatus::bad_request(&err))?
                .into_iter()
                .map(Ok);

            // Ensure task stops if the peer prematurely stops their RPC session
            if utils::mpsc::send_all(tx, utxos).await.is_err() {
                break;
            }

            debug!(
                target: LOG_TARGET,
                "Streamed utxos in {:.2?} (including stream backpressure)",
                timer.elapsed()
            );

            if current_header.height + 1 > end_header.height {
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
            "UTXO sync completed to Header hash = {}",
            current_header.hash().to_hex()
        );

        Ok(())
    }
}
