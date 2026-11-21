// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{convert::TryInto, sync::Arc, time::Instant};

use log::*;
use prost::Message;
use tari_comms::{
    peer_manager::NodeId,
    protocol::rpc::{RpcStatus, RpcStatusResultExt},
    utils::mpsc::ChannelByteBudget,
};
use tari_node_components::blocks::BlockHeader;
use tari_utilities::hex::Hex;
use tokio::{sync::mpsc, task};

use crate::{
    chain_storage::{BlockchainBackend, ChainStorageError, NotServed, async_db::AsyncBlockchainDb},
    proto,
    proto::base_node::{SyncUtxosByBlockRequest, SyncUtxosByBlockResponse},
};

const LOG_TARGET: &str = "c::base_node::sync_rpc::sync_utxo_by_block_task";
const REORG_DETECTED: &str = "Reorg detected: the requested end block is no longer on the chain";
pub(super) const END_BODY_NOT_STORED: &str = "The requested end block's body is not stored on this node";
pub(super) const OUTPUT_REBUILD_CONFLICT_MSG: &str =
    "Committed output state is unavailable while horizon sync rebuilds outputs";

pub(crate) struct SyncUtxosByBlockTask<B> {
    db: AsyncBlockchainDb<B>,
    max_buffered_bytes: usize,
}

impl<B> SyncUtxosByBlockTask<B>
where B: BlockchainBackend + 'static
{
    pub(crate) fn new(db: AsyncBlockchainDb<B>, max_buffered_bytes: usize) -> Self {
        Self { db, max_buffered_bytes }
    }

    /// Returns the start and end headers of a valid request
    pub(crate) async fn validate(
        &self,
        request: &SyncUtxosByBlockRequest,
    ) -> Result<(BlockHeader, BlockHeader), RpcStatus> {
        let start_hash = request
            .start_header_hash
            .clone()
            .try_into()
            .map_err(|_| RpcStatus::bad_request("Malformed start header hash received"))?;
        let end_hash = request
            .end_header_hash
            .clone()
            .try_into()
            .map_err(|_| RpcStatus::bad_request("Malformed end header hash received"))?;

        let (start_header, end_header) = match self
            .db
            .validate_sync_utxos_by_block_range(start_hash, end_hash)
            .await
            .rpc_status_internal_error(LOG_TARGET)?
        {
            Ok(pair) => pair,
            Err(NotServed::BodyNotStored) => return Err(RpcStatus::conflict(END_BODY_NOT_STORED)),
            Err(NotServed::OutputRebuild) => return Err(RpcStatus::conflict(OUTPUT_REBUILD_CONFLICT_MSG)),
            Err(NotServed::NotOnChain) => return Err(RpcStatus::conflict(REORG_DETECTED)),
        };

        let start_header = start_header.ok_or_else(|| RpcStatus::not_found("Start header hash was not found"))?;
        let end_header = end_header.ok_or_else(|| RpcStatus::not_found("End header hash was not found"))?;

        if start_header.height > end_header.height {
            return Err(RpcStatus::bad_request(&format!(
                "start header height {} cannot be greater than the end header height ({})",
                start_header.height, end_header.height
            )));
        }
        Ok((start_header, end_header))
    }

    /// Streams the outputs of the blocks from `start_header` to `end_header` on a new task, which holds
    /// `session_token` until it ends. `tx` must be the channel's only sender.
    pub(crate) fn run(
        self,
        start_header: BlockHeader,
        end_header: BlockHeader,
        tx: mpsc::Sender<Result<SyncUtxosByBlockResponse, RpcStatus>>,
        session_token: Arc<NodeId>,
    ) {
        task::spawn(async move {
            if let Err(err) = self.start_streaming(&tx, start_header, end_header).await {
                let _result = tx.send(Err(err)).await;
            }
            debug!(target: LOG_TARGET, "UTXO sync by block session for peer '{session_token}' ended");
        });
    }

    async fn start_streaming(
        &self,
        tx: &mpsc::Sender<Result<SyncUtxosByBlockResponse, RpcStatus>>,
        start_header: BlockHeader,
        end_header: BlockHeader,
    ) -> Result<(), RpcStatus> {
        let end_hash = end_header.hash();
        trace!(
            target: LOG_TARGET,
            "Starting stream task with start_header: {} and end_header: {}",
            start_header.hash().to_hex(),
            end_hash.to_hex(),
        );

        let mut budget = ChannelByteBudget::new(self.max_buffered_bytes);
        for height in start_header.height..=end_header.height {
            let timer = Instant::now();
            if tx.is_closed() {
                debug!(target: LOG_TARGET, "Exiting sync_utxos early because client has gone",);
                return Ok(());
            }

            let header = if height == start_header.height {
                start_header.clone()
            } else {
                match self.db.fetch_header(height).await {
                    Ok(Some(header)) => header,
                    Ok(None) => return Err(reorg_conflict(height)),
                    Err(err) if err.is_value_not_found() || matches!(err, ChainStorageError::InvalidQuery(_)) => {
                        return Err(reorg_conflict(height));
                    },
                    Err(err) => return Err(RpcStatus::log_internal_error(LOG_TARGET)(err)),
                }
            };
            let header_hash = header.hash();
            let outputs = match self
                .db
                .fetch_outputs_in_block_on_chain(height, header_hash, end_header.height, end_hash)
                .await
            {
                Ok(Ok(outputs)) => outputs,
                Ok(Err(NotServed::NotOnChain)) => return Err(reorg_conflict(height)),
                Ok(Err(NotServed::BodyNotStored)) => {
                    debug!(
                        target: LOG_TARGET,
                        "Ending UTXO sync by block at #{height}: requested end block body is not stored"
                    );
                    return Err(RpcStatus::conflict(END_BODY_NOT_STORED));
                },
                Ok(Err(NotServed::OutputRebuild)) => {
                    warn!(
                        target: LOG_TARGET,
                        "Aborting UTXO sync by block at #{height}: committed output state is being rebuilt by horizon sync"
                    );
                    return Err(RpcStatus::conflict(OUTPUT_REBUILD_CONFLICT_MSG));
                },
                Err(err) if err.is_value_not_found() || matches!(err, ChainStorageError::InvalidQuery(_)) => {
                    return Err(reorg_conflict(height));
                },
                Err(err) => return Err(RpcStatus::log_internal_error(LOG_TARGET)(err)),
            };
            let outputs = outputs
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<proto::types::TransactionOutput>, String>>()
                .map_err(|err| RpcStatus::general(&err))?;

            trace!(
                target: LOG_TARGET,
                "Streaming {} UTXO(s) for block #{} (Hash: {})",
                outputs.len(),
                height,
                header_hash.to_hex(),
            );

            // Each chunk is moved into its response only once the budget has room for it. An empty block still gets
            // one response, with no outputs.
            let mut outputs = outputs.into_iter();
            loop {
                // Ensure task stops if the peer prematurely stops their RPC session
                if budget.wait_for_room(tx).await.is_err() {
                    return Ok(());
                }
                let response = SyncUtxosByBlockResponse {
                    outputs: outputs.by_ref().take(2000).collect(),
                    height,
                    header_hash: header_hash.to_vec(),
                    mined_timestamp: header.timestamp.as_u64(),
                };
                let num_bytes = response.encoded_len();
                if tx.send(Ok(response)).await.is_err() {
                    return Ok(());
                }
                budget.record_sent(num_bytes);
                if outputs.len() == 0 {
                    break;
                }
            }

            trace!(
                target: LOG_TARGET,
                "Streamed utxos in {:.2?} (including stream backpressure)",
                timer.elapsed()
            );
        }

        trace!(
            target: LOG_TARGET,
            "UTXO sync by block completed to header hash {}",
            end_hash.to_hex()
        );

        Ok(())
    }
}

fn reorg_conflict(height: u64) -> RpcStatus {
    warn!(
        target: LOG_TARGET,
        "Block #{height} is no longer an ancestor of the requested end block during UTXO sync by block"
    );
    RpcStatus::conflict(REORG_DETECTED)
}
