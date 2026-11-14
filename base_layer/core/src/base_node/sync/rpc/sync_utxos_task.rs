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
use prost::Message;
#[cfg(test)]
use tari_comms::protocol::rpc::Request;
use tari_comms::{
    peer_manager::NodeId,
    protocol::rpc::{RpcStatus, RpcStatusResultExt},
    utils::mpsc::ChannelByteBudget,
};
use tari_node_components::blocks::BlockHeader;
use tari_utilities::{ByteArray, hex::Hex};
use tokio::{sync::mpsc, task};

#[cfg(feature = "metrics")]
use crate::base_node::metrics;
use crate::{
    base_node::RPC_STREAM_MAX_BUFFERED_BYTES,
    chain_storage::{BlockchainBackend, ChainStorageError, HorizonSyncBlockTxos, async_db::AsyncBlockchainDb},
    proto,
    proto::base_node::{SyncUtxosRequest, SyncUtxosResponse, sync_utxos_response::Txo},
};

const LOG_TARGET: &str = "c::base_node::sync_rpc::sync_utxo_task";

pub(crate) struct SyncUtxosTask<B> {
    db: AsyncBlockchainDb<B>,
    peer_node_id: Arc<NodeId>,
    max_buffered_bytes: usize,
    #[cfg(feature = "metrics")]
    active_sync_peers_metric: tari_metrics::IntGauge,
}

impl<B> SyncUtxosTask<B>
where B: BlockchainBackend + 'static
{
    pub(crate) fn new(db: AsyncBlockchainDb<B>, peer_node_id: Arc<NodeId>) -> Self {
        Self {
            db,
            peer_node_id,
            max_buffered_bytes: RPC_STREAM_MAX_BUFFERED_BYTES,
            #[cfg(feature = "metrics")]
            active_sync_peers_metric: metrics::active_sync_peers().clone(),
        }
    }

    pub(crate) fn with_max_buffered_bytes(mut self, max_buffered_bytes: usize) -> Self {
        self.max_buffered_bytes = max_buffered_bytes;
        self
    }

    #[cfg(feature = "metrics")]
    pub(crate) fn with_active_sync_peers_metric(mut self, metric: tari_metrics::IntGauge) -> Self {
        self.active_sync_peers_metric = metric;
        self
    }

    /// Validates the requested header range before a sync session slot or metric is acquired.
    pub(crate) async fn validate(
        db: &AsyncBlockchainDb<B>,
        msg: &SyncUtxosRequest,
    ) -> Result<(BlockHeader, BlockHeader), RpcStatus> {
        let start_hash = msg
            .start_header_hash
            .clone()
            .try_into()
            .rpc_status_bad_request("Invalid header hash")?;

        let start_header = db
            .fetch_header_by_block_hash(start_hash)
            .await
            .rpc_status_internal_error(LOG_TARGET)?
            .ok_or_else(|| RpcStatus::not_found("Start header hash was not found"))?;

        let end_hash = msg
            .end_header_hash
            .clone()
            .try_into()
            .rpc_status_bad_request("Invalid header hash")?;

        let end_header = db
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

        Ok((start_header, end_header))
    }

    #[cfg(test)]
    pub(crate) async fn run(
        self,
        request: Request<SyncUtxosRequest>,
        tx: mpsc::Sender<Result<SyncUtxosResponse, RpcStatus>>,
    ) -> Result<(), RpcStatus> {
        let (start_header, end_header) = Self::validate(&self.db, request.message()).await?;
        self.run_validated(start_header, end_header, tx);
        Ok(())
    }

    /// Streams TXOs from `start_header` to `end_header` on a background task, holding `self.peer_node_id` until
    /// completion. `tx` must be the channel's only sender.
    pub(crate) fn run_validated(
        self,
        start_header: BlockHeader,
        end_header: BlockHeader,
        tx: mpsc::Sender<Result<SyncUtxosResponse, RpcStatus>>,
    ) {
        task::spawn(async move {
            debug!(
                target: LOG_TARGET,
                "Starting UTXO stream for peer '{}'", self.peer_node_id
            );
            if let Err(err) = self.start_streaming(&tx, start_header, end_header).await {
                debug!(
                    target: LOG_TARGET,
                    "UTXO stream errored for peer '{}': {err}", self.peer_node_id
                );
                let _result = tx.send(Err(err)).await;
            }
            debug!(
                target: LOG_TARGET,
                "UTXO stream completed for peer '{}'", self.peer_node_id
            );
            #[cfg(feature = "metrics")]
            self.active_sync_peers_metric.dec();
        });
    }

    #[allow(clippy::too_many_lines)]
    async fn start_streaming(
        &self,
        tx: &mpsc::Sender<Result<SyncUtxosResponse, RpcStatus>>,
        start_header: BlockHeader,
        end_header: BlockHeader,
    ) -> Result<(), RpcStatus> {
        let end_hash = end_header.hash();
        let end_height = end_header.height;
        debug!(
            target: LOG_TARGET,
            "Starting stream task with start_header: {}, end_header: {}",
            start_header.hash().to_hex(),
            end_hash.to_hex(),
        );

        // If this is a pruned node and outputs have been requested for an initial sync, we need to discover and send
        // the outputs from the genesis block that have been pruned as well
        let mut pruned_genesis_block_outputs = Vec::new();
        let metadata = self
            .db
            .get_chain_metadata()
            .await
            .rpc_status_internal_error(LOG_TARGET)?;
        if start_header.height == 1 && metadata.is_pruned_node() {
            let genesis_block = self.db.fetch_genesis_block();
            for output in genesis_block.block().body.outputs() {
                let output_hash = output.hash();
                // Only the genesis instance counts; the same output may have been mined again later.
                if !self
                    .db
                    .fetch_outputs(output_hash)
                    .await
                    .rpc_status_internal_error(LOG_TARGET)?
                    .iter()
                    .any(|o| o.header_hash == *genesis_block.hash())
                {
                    trace!(
                        target: LOG_TARGET,
                        "Spent genesis TXO (commitment '{}') to peer",
                        output.commitment.to_hex()
                    );
                    pruned_genesis_block_outputs.push(SyncUtxosResponse {
                        txo: Some(Txo::Commitment(output.commitment.as_bytes().to_vec())),
                        mined_header: start_header.hash().to_vec(),
                    });
                }
            }
        }

        let peer_node_id = &self.peer_node_id;
        let mut budget = ChannelByteBudget::new(self.max_buffered_bytes);
        for height in start_header.height..=end_height {
            let timer = Instant::now();
            if tx.is_closed() {
                debug!(target: LOG_TARGET, "Peer '{peer_node_id}' exited TXO sync session early");
                return Ok(());
            }
            if budget.wait_for_room(tx).await.is_err() {
                debug!(target: LOG_TARGET, "Peer '{peer_node_id}' exited TXO sync session early");
                return Ok(());
            }

            let expected_hash = (height == start_header.height).then(|| start_header.hash());
            let HorizonSyncBlockTxos {
                header: current_header,
                outputs_with_statuses,
                inputs_in_block,
                spent_mined_infos,
            } = match self
                .db
                .fetch_horizon_sync_block_txos(height, expected_hash, end_height, end_hash)
                .await
            {
                Ok(Some(block_txos)) => block_txos,
                Ok(None) => return Err(reorg_conflict(height)),
                Err(err) if err.is_value_not_found() || matches!(err, ChainStorageError::InvalidQuery(_)) => {
                    return Err(reorg_conflict(height));
                },
                Err(err) => return Err(RpcStatus::log_internal_error(LOG_TARGET)(err)),
            };

            let current_header_hash = current_header.hash();
            debug!(
                target: LOG_TARGET,
                "Streaming TXO(s) for block #{} ({})",
                current_header.height,
                current_header_hash.to_hex()
            );

            let mut outputs = Vec::with_capacity(outputs_with_statuses.len());
            for (output, spent) in outputs_with_statuses {
                if output.is_burned() {
                    continue;
                }
                if !spent {
                    let output_hash = output.hash();
                    let output_commitment = output.commitment.clone();
                    match proto::types::TransactionOutput::try_from(output) {
                        Ok(tx_output) => {
                            trace!(
                                target: LOG_TARGET,
                                "Unspent TXO (commitment '{}') to peer",
                                output_commitment.to_hex()
                            );
                            outputs.push(SyncUtxosResponse {
                                txo: Some(Txo::Output(tx_output)),
                                mined_header: current_header_hash.to_vec(),
                            });
                        },
                        Err(e) => {
                            return Err(RpcStatus::general(&format!(
                                "Output '{}' RPC conversion error ({})",
                                output_hash.to_hex(),
                                e
                            )));
                        },
                    }
                }
            }
            debug!(
                target: LOG_TARGET,
                "Adding {} outputs in response for block #{} '{}'",
                outputs.len(),
                current_header.height,
                current_header_hash
            );

            let mut inputs = Vec::with_capacity(inputs_in_block.len());
            for (input, mined_info) in inputs_in_block.iter().zip(spent_mined_infos) {
                if let Some(info) = &mined_info &&
                    info.mined_height >= start_header.height
                {
                    trace!(
                        target: LOG_TARGET,
                        "Spent TXO (hash '{}') not sent to peer",
                        input.output_hash().to_hex()
                    );
                    continue;
                }
                let input_commitment = match mined_info {
                    Some(o) => o.output.commitment,
                    None => {
                        return Err(RpcStatus::general(&format!(
                            "Mined info for input '{}' not found",
                            input.output_hash().to_hex()
                        )));
                    },
                };
                trace!(target: LOG_TARGET, "Spent TXO (commitment '{}') to peer", input_commitment.to_hex());
                inputs.push(SyncUtxosResponse {
                    txo: Some(Txo::Commitment(input_commitment.as_bytes().to_vec())),
                    mined_header: current_header_hash.to_vec(),
                });
            }
            debug!(
                target: LOG_TARGET,
                "Adding {} inputs in response for block #{} '{}'",
                inputs.len(),
                current_header.height,
                current_header_hash
            );

            let mut txos = Vec::with_capacity(outputs.len().saturating_add(inputs.len()));
            txos.append(&mut outputs);
            txos.append(&mut inputs);
            if height == start_header.height {
                debug!(
                    target: LOG_TARGET,
                    "Adding {} genesis block pruned inputs in response for block #{} '{}'",
                    pruned_genesis_block_outputs.len(),
                    current_header.height,
                    current_header_hash
                );
                txos.append(&mut pruned_genesis_block_outputs);
            }

            let txos_len = txos.len();
            for resp in txos {
                if budget.wait_for_room(tx).await.is_err() {
                    debug!(target: LOG_TARGET, "Peer '{peer_node_id}' exited TXO sync session early");
                    return Ok(());
                }
                let num_bytes = resp.encoded_len();
                if tx.send(Ok(resp)).await.is_err() {
                    debug!(target: LOG_TARGET, "Peer '{peer_node_id}' exited TXO sync session early");
                    return Ok(());
                }
                budget.record_sent(num_bytes);
            }

            debug!(
                target: LOG_TARGET,
                "Streamed {} TXOs in {:.2?} (including stream backpressure)",
                txos_len,
                timer.elapsed()
            );
        }

        // Always send a final terminator response tagged with the end header once the full requested range has been
        // walked. An early return on error or client disconnect never reaches this point, so a truncated stream is
        // never mistaken for a complete one by the consumer.
        if budget.wait_for_room(tx).await.is_err() {
            debug!(
                target: LOG_TARGET,
                "Peer '{peer_node_id}' exited TXO sync session before terminator could be sent"
            );
            return Ok(());
        }
        let terminator = SyncUtxosResponse {
            txo: None,
            mined_header: end_hash.to_vec(),
        };
        let num_bytes = terminator.encoded_len();
        if tx.send(Ok(terminator)).await.is_err() {
            debug!(
                target: LOG_TARGET,
                "Peer '{peer_node_id}' exited TXO sync session before terminator could be sent"
            );
            return Ok(());
        }
        budget.record_sent(num_bytes);

        debug!(
            target: LOG_TARGET,
            "TXO sync completed to Header hash = {}",
            end_hash.to_hex()
        );

        Ok(())
    }
}

fn reorg_conflict(height: u64) -> RpcStatus {
    warn!(
        target: LOG_TARGET,
        "Block #{height} is no longer an ancestor of the requested end block during horizon UTXO sync"
    );
    RpcStatus::conflict("Reorg detected: the requested end block is no longer on the chain")
}

#[cfg(test)]
mod tests {
    use tari_transaction_components::transaction_components::TransactionOutput;

    use super::*;

    #[tokio::test]
    async fn duplicate_output_stream_matches_spend_to_pre_tranche_instance() {
        let (chain, ..) = crate::validation::block_body::test::duplicate_output_chain();
        let spent = chain.get_block_and_smt_by_name("3").unwrap();
        let remined = chain.get_block_and_smt_by_name("4").unwrap();
        let hash = spent.block().body.inputs().first().unwrap().output_hash();
        let output = chain.db().fetch_outputs(hash).unwrap().pop().unwrap().output;
        let task = SyncUtxosTask::new(chain.db().clone().into(), Arc::new(NodeId::new()));
        // The tranche includes s1 and m2, but m1 belongs to the previous tranche.
        let (tx, mut rx) = mpsc::channel(100);
        task.start_streaming(&tx, spent.header().clone(), remined.header().clone())
            .await
            .unwrap();
        drop(tx);
        let mut lifecycle = Vec::new();
        while let Some(response) = rx.recv().await {
            let response = response.unwrap();
            match response.txo {
                Some(Txo::Commitment(c)) if c == output.commitment.as_bytes() => {
                    lifecycle.push((false, response.mined_header));
                },
                Some(Txo::Output(o)) if TransactionOutput::try_from(o.clone()).unwrap().hash() == hash => {
                    lifecycle.push((true, response.mined_header));
                },
                _ => {},
            }
        }
        assert_eq!(lifecycle, vec![
            (false, spent.hash().to_vec()),
            (true, remined.hash().to_vec())
        ]);
    }
}
