// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{convert::TryInto, sync::Arc, time::Instant};

use log::*;
use prost::Message;
use tari_common_types::types::FixedHash;
use tari_comms::{
    peer_manager::NodeId,
    protocol::rpc::{RpcStatus, RpcStatusResultExt},
    utils::mpsc::ChannelByteBudget,
};
use tari_node_components::blocks::BlockHeader;
use tari_transaction_components::transaction_components::TransactionOutput;
use tari_utilities::hex::Hex;
use tokio::{sync::mpsc, task};

use crate::{
    base_node::sync::rpc::is_on_requested_chain,
    chain_storage::{BlockchainBackend, ChainStorageError, async_db::AsyncBlockchainDb},
    proto,
    proto::base_node::{SyncUtxosByBlockRequest, SyncUtxosByBlockResponse},
};

const LOG_TARGET: &str = "c::base_node::sync_rpc::sync_utxo_by_block_task";

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
        let hash = request
            .start_header_hash
            .clone()
            .try_into()
            .map_err(|_| RpcStatus::bad_request("Malformed start header hash received"))?;
        let start_header = self
            .db
            .fetch_header_by_block_hash(hash)
            .await
            .rpc_status_internal_error(LOG_TARGET)?
            .ok_or_else(|| RpcStatus::not_found("Start header hash was not found"))?;
        let hash = request
            .end_header_hash
            .clone()
            .try_into()
            .map_err(|_| RpcStatus::bad_request("Malformed end header hash received"))?;
        let end_header = self
            .db
            .fetch_header_by_block_hash(hash)
            .await
            .rpc_status_internal_error(LOG_TARGET)?
            .ok_or_else(|| RpcStatus::not_found("End header hash was not found"))?;

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

            let (header, outputs) = match self
                .fetch_block_on_requested_chain(height, &start_header, end_header.height, &end_hash)
                .await
            {
                Ok(Some(block)) => block,
                // Every height was at or below the end block when the stream started, so a missing value means the
                // chain was rewound
                Ok(None) => return Err(reorg_conflict(height)),
                Err(err) if err.is_value_not_found() || matches!(err, ChainStorageError::InvalidQuery(_)) => {
                    return Err(reorg_conflict(height));
                },
                Err(err) => return Err(RpcStatus::log_internal_error(LOG_TARGET)(err)),
            };
            let header_hash = header.hash();
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

    /// Returns the header and outputs of the block at `height`, or None if it is no longer an ancestor of the requested
    /// end block. The outputs and the headers at `height` and `end_height` are read under one read lock, so the check
    /// holds for the chain state the outputs were read from, whatever reorgs happened before.
    async fn fetch_block_on_requested_chain(
        &self,
        height: u64,
        start_header: &BlockHeader,
        end_height: u64,
        end_hash: &FixedHash,
    ) -> Result<Option<(BlockHeader, Vec<TransactionOutput>)>, ChainStorageError> {
        let header = if height == start_header.height {
            start_header.clone()
        } else {
            match self.db.fetch_header(height).await? {
                Some(header) => header,
                None => return Ok(None),
            }
        };
        let hash = header.hash();
        let (outputs, header_at_height, end_header) = self
            .db
            .fetch_outputs_in_block_and_header_pair(hash, height, end_height)
            .await?;
        if !is_on_requested_chain(&hash, header_at_height.as_ref(), end_header.as_ref(), end_hash) {
            return Ok(None);
        }
        Ok(Some((header, outputs)))
    }
}

fn reorg_conflict(height: u64) -> RpcStatus {
    warn!(
        target: LOG_TARGET,
        "Block #{height} is no longer an ancestor of the requested end block during UTXO sync by block"
    );
    RpcStatus::conflict("Reorg detected: the requested end block is no longer on the chain")
}
