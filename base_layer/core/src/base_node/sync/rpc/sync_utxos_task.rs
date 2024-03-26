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

use std::{
    convert::{TryFrom, TryInto},
    sync::Arc,
    time::Instant,
};

use log::*;
use tari_comms::{
    peer_manager::NodeId,
    protocol::rpc::{Request, RpcStatus, RpcStatusResultExt},
    utils,
};
use tari_utilities::{hex::Hex, ByteArray};
use tokio::{sync::mpsc, task};

#[cfg(feature = "metrics")]
use crate::base_node::metrics;
use crate::{
    blocks::BlockHeader,
    chain_storage::{async_db::AsyncBlockchainDb, BlockchainBackend},
    proto,
    proto::base_node::{sync_utxos_response::Txo, SyncUtxosRequest, SyncUtxosResponse},
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
            .ok_or_else(|| RpcStatus::not_found("Start header hash was not found"))?;

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
            .ok_or_else(|| RpcStatus::not_found("End header hash was not found"))?;
        if start_header.height > end_header.height {
            return Err(RpcStatus::bad_request(&format!(
                "Start header height({}) cannot be greater than the end header height({})",
                start_header.height, end_header.height
            )));
        }

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
            #[cfg(feature = "metrics")]
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
            "Starting stream task with current_header: {}, end_header: {}",
            current_header.hash().to_hex(),
            end_header.hash().to_hex(),
        );

        // If this is a pruned node and outputs have been requested for an initial sync, we need to discover and send
        // the outputs from the genesis block that have been pruned as well
        let mut pruned_genesis_block_outputs = Vec::new();
        let metadata = self
            .db
            .get_chain_metadata()
            .await
            .rpc_status_internal_error(LOG_TARGET)?;
        if current_header.height == 1 && metadata.is_pruned_node() {
            let genesis_block = self.db.fetch_genesis_block();
            for output in genesis_block.block().body.outputs() {
                let output_hash = output.hash();
                if self
                    .db
                    .fetch_output(output_hash)
                    .await
                    .rpc_status_internal_error(LOG_TARGET)?
                    .is_none()
                {
                    trace!(
                        target: LOG_TARGET,
                        "Spent genesis TXO (commitment '{}') to peer",
                        output.commitment.to_hex()
                    );
                    pruned_genesis_block_outputs.push(Ok(SyncUtxosResponse {
                        txo: Some(Txo::Commitment(output.commitment.as_bytes().to_vec())),
                        mined_header: current_header.hash().to_vec(),
                    }));
                }
            }
        }

        let start_header = current_header.clone();
        loop {
            let timer = Instant::now();
            let current_header_hash = current_header.hash();
            debug!(
                target: LOG_TARGET,
                "Streaming TXO(s) for block #{} ({})",
                current_header.height,
                current_header_hash.to_hex()
            );
            if tx.is_closed() {
                debug!(target: LOG_TARGET, "Peer '{}' exited TXO sync session early", self.peer_node_id);
                break;
            }

            let outputs_with_statuses = self
                .db
                .fetch_outputs_in_block_with_spend_state(current_header_hash, Some(end_header.hash()))
                .await
                .rpc_status_internal_error(LOG_TARGET)?;
            if tx.is_closed() {
                debug!(target: LOG_TARGET, "Peer '{}' exited TXO sync session early", self.peer_node_id);
                break;
            }

            let mut outputs = Vec::with_capacity(outputs_with_statuses.len());
            for (output, spent) in outputs_with_statuses {
                if output.is_burned() {
                    continue;
                }
                if !spent {
                    match proto::types::TransactionOutput::try_from(output.clone()) {
                        Ok(tx_ouput) => {
                            trace!(
                                target: LOG_TARGET,
                                "Unspent TXO (commitment '{}') to peer",
                                output.commitment.to_hex()
                            );
                            outputs.push(Ok(SyncUtxosResponse {
                                txo: Some(Txo::Output(tx_ouput)),
                                mined_header: current_header_hash.to_vec(),
                            }));
                        },
                        Err(e) => {
                            return Err(RpcStatus::general(&format!(
                                "Output '{}' RPC conversion error ({})",
                                output.hash().to_hex(),
                                e
                            )))
                        },
                    }
                }
            }
            debug!(
                target: LOG_TARGET,
                "Adding {} outputs in response for block #{} '{}'", outputs.len(),
                current_header.height,
                current_header_hash
            );

            let inputs_in_block = self
                .db
                .fetch_inputs_in_block(current_header_hash)
                .await
                .rpc_status_internal_error(LOG_TARGET)?;
            if tx.is_closed() {
                debug!(target: LOG_TARGET, "Peer '{}' exited TXO sync session early", self.peer_node_id);
                break;
            }

            let mut inputs = Vec::with_capacity(inputs_in_block.len());
            for input in inputs_in_block {
                let output_from_current_tranche = if let Some(mined_info) = self
                    .db
                    .fetch_output(input.output_hash())
                    .await
                    .rpc_status_internal_error(LOG_TARGET)?
                {
                    mined_info.mined_height >= start_header.height
                } else {
                    false
                };

                if output_from_current_tranche {
                    trace!(target: LOG_TARGET, "Spent TXO (hash '{}') not sent to peer", input.output_hash().to_hex());
                } else {
                    let input_commitment = match self.db.fetch_output(input.output_hash()).await {
                        Ok(Some(o)) => o.output.commitment,
                        Ok(None) => {
                            return Err(RpcStatus::general(&format!(
                                "Mined info for input '{}' not found",
                                input.output_hash().to_hex()
                            )))
                        },
                        Err(e) => {
                            return Err(RpcStatus::general(&format!(
                                "Input '{}' not found ({})",
                                input.output_hash().to_hex(),
                                e
                            )))
                        },
                    };
                    trace!(target: LOG_TARGET, "Spent TXO (commitment '{}') to peer", input_commitment.to_hex());
                    inputs.push(Ok(SyncUtxosResponse {
                        txo: Some(Txo::Commitment(input_commitment.as_bytes().to_vec())),
                        mined_header: current_header_hash.to_vec(),
                    }));
                }
            }
            debug!(
                target: LOG_TARGET,
                "Adding {} inputs in response for block #{} '{}'", inputs.len(),
                current_header.height,
                current_header_hash
            );

            let mut txos = Vec::with_capacity(outputs.len() + inputs.len());
            txos.append(&mut outputs);
            txos.append(&mut inputs);
            if start_header == current_header {
                debug!(
                    target: LOG_TARGET,
                    "Adding {} genesis block pruned inputs in response for block #{} '{}'", pruned_genesis_block_outputs.len(),
                    current_header.height,
                    current_header_hash
                );
                txos.append(&mut pruned_genesis_block_outputs);
            }
            let txos = txos.into_iter();

            // Ensure task stops if the peer prematurely stops their RPC session
            let txos_len = txos.len();
            if utils::mpsc::send_all(tx, txos).await.is_err() {
                break;
            }

            debug!(
                target: LOG_TARGET,
                "Streamed {} TXOs in {:.2?} (including stream backpressure)",
                txos_len,
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
            "TXO sync completed to Header hash = {}",
            current_header.hash().to_hex()
        );

        Ok(())
    }
}
