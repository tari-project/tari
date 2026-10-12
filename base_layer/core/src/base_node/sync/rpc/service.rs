//  Copyright 2020, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{
    cmp,
    convert::{TryFrom, TryInto},
    sync::{Arc, Weak},
};

use log::*;
use prost::Message;
use tari_common_types::types::FixedHash;
use tari_comms::{
    peer_manager::NodeId,
    protocol::rpc::{Request, Response, RpcStatus, RpcStatusResultExt, Streaming},
    utils,
    utils::mpsc::ChannelByteBudget,
};
use tari_node_components::blocks::BlockHeader;
use tari_utilities::hex::Hex;
use tokio::{
    sync::{Mutex, mpsc},
    task,
};
use tracing::{Instrument, Level, instrument, span};

#[cfg(feature = "metrics")]
use crate::base_node::metrics;
use crate::{
    base_node::{
        LocalNodeCommsInterface,
        sync::{
            header_sync::HEADER_SYNC_INITIAL_MAX_HEADERS,
            rpc::{
                BaseNodeSyncService,
                sync_utxos_task::{HEADER_CHUNK_SIZE, SyncUtxosTask},
            },
        },
    },
    chain_storage::{BlockchainBackend, ChainStorageError, async_db::AsyncBlockchainDb},
    iterators::NonOverlappingIntegerPairIter,
    proto,
    proto::base_node::{
        FindChainSplitRequest,
        FindChainSplitResponse,
        SyncBlocksRequest,
        SyncHeadersRequest,
        SyncKernelsRequest,
        SyncUtxosRequest,
        SyncUtxosResponse,
    },
};

const LOG_TARGET: &str = "c::base_node::sync_rpc";

/// The byte budget of one `sync_blocks` session. It counts the blocks queued for the RPC server and the last block the
/// server took, and the next block is fetched only while that is below this. So a session holds at most this plus two
/// blocks: the block being fetched, and the block the RPC server is still writing while it holds the next one
/// prefetched. Typical mainnet blocks still fill the whole `block_batch_size` queue.
const SYNC_BLOCKS_MAX_BUFFERED_BYTES: usize = 16 * 1024 * 1024;

pub struct BaseNodeSyncRpcService<B> {
    db: AsyncBlockchainDb<B>,
    active_sessions: Mutex<Vec<Weak<NodeId>>>,
    /// The most blocks queued for the RPC server per `sync_blocks` session
    block_batch_size: usize,
    /// The most bytes of blocks buffered per `sync_blocks` session (see [SYNC_BLOCKS_MAX_BUFFERED_BYTES])
    max_buffered_block_bytes: usize,
}

impl<B: BlockchainBackend + 'static> BaseNodeSyncRpcService<B> {
    /// `_base_node_service` is no longer used: reorgs during block sync are detected from the chain itself. It is kept
    /// so that callers do not change.
    pub fn new(db: AsyncBlockchainDb<B>, _base_node_service: LocalNodeCommsInterface, block_batch_size: usize) -> Self {
        if block_batch_size == 0 {
            warn!(
                target: LOG_TARGET,
                "blockchain_sync_config.rpc_block_batch_size is 0, using 1 instead"
            );
        }
        Self {
            db,
            active_sessions: Mutex::new(Vec::new()),
            block_batch_size: cmp::max(block_batch_size, 1),
            max_buffered_block_bytes: SYNC_BLOCKS_MAX_BUFFERED_BYTES,
        }
    }

    #[cfg(test)]
    pub(super) fn with_max_buffered_block_bytes(mut self, max_bytes: usize) -> Self {
        self.max_buffered_block_bytes = max_bytes;
        self
    }

    #[inline]
    fn db(&self) -> AsyncBlockchainDb<B> {
        self.db.clone()
    }

    pub async fn try_add_exclusive_session(&self, peer: NodeId) -> Result<Arc<NodeId>, RpcStatus> {
        let mut lock = self.active_sessions.lock().await;
        *lock = lock.drain(..).filter(|l| l.strong_count() > 0).collect();
        debug!(target: LOG_TARGET, "Number of active sync sessions: {}", lock.len());

        if lock.iter().any(|p| p.upgrade().filter(|p| **p == peer).is_some()) {
            return Err(RpcStatus::forbidden(
                "Existing sync session found for this client. Only a single session is permitted",
            ));
        }

        let token = Arc::new(peer);
        lock.push(Arc::downgrade(&token));
        #[allow(clippy::cast_possible_wrap)]
        #[cfg(feature = "metrics")]
        metrics::active_sync_peers().set(lock.len() as i64);
        Ok(token)
    }
}

/// Returns true if the chain whose headers at the fetched block's height and at the end height are `header_at_height`
/// and `end_header` has the fetched block at its height and the requested end block at the end height. Both headers
/// must come from the same chain state; the fetched block is then an ancestor of the requested end block.
pub(super) fn is_on_requested_chain(
    block_hash: &FixedHash,
    header_at_height: Option<&BlockHeader>,
    end_header: Option<&BlockHeader>,
    end_hash: &FixedHash,
) -> bool {
    header_at_height.is_some_and(|header| header.hash() == *block_hash) &&
        end_header.is_some_and(|header| header.hash() == *end_hash)
}

#[tari_comms::async_trait]
impl<B: BlockchainBackend + 'static> BaseNodeSyncService for BaseNodeSyncRpcService<B> {
    #[instrument(level = "trace", name = "sync_rpc::sync_blocks", skip(self), err)]
    #[allow(clippy::blocks_in_conditions)]
    async fn sync_blocks(
        &self,
        request: Request<SyncBlocksRequest>,
    ) -> Result<Streaming<proto::base_node::BlockBodyResponse>, RpcStatus> {
        let peer_node_id = request.context().peer_node_id().clone();
        let message = request.into_message();

        let db = self.db();
        let hash = message
            .start_hash
            .try_into()
            .map_err(|_| RpcStatus::bad_request(&"Malformed starting hash received".to_string()))?;
        if db.fetch_block_by_hash(hash, true).await.is_err() {
            return Err(RpcStatus::not_found("Requested start block sync hash was not found"));
        }
        let start_header = db
            .fetch_header_by_block_hash(hash)
            .await
            .rpc_status_internal_error(LOG_TARGET)?
            .ok_or_else(|| RpcStatus::not_found("Header not found with given hash"))?;

        let metadata = db.get_chain_metadata().await.rpc_status_internal_error(LOG_TARGET)?;

        let start_height = start_header.height.saturating_add(1);
        if start_height < metadata.pruned_height() {
            return Err(RpcStatus::bad_request(&format!(
                "Requested full block body at height {}, however this node has an effective pruned height of {}",
                start_height,
                metadata.pruned_height()
            )));
        }

        let hash: FixedHash = message
            .end_hash
            .try_into()
            .map_err(|_| RpcStatus::bad_request(&"Malformed end hash received".to_string()))?;
        let end_hash = hash;
        if db.fetch_block_by_hash(hash, true).await.is_err() {
            return Err(RpcStatus::not_found("Requested end block sync hash was not found"));
        }

        let end_header = db
            .fetch_header_by_block_hash(hash)
            .await
            .rpc_status_internal_error(LOG_TARGET)?
            .ok_or_else(|| RpcStatus::not_found("Requested end block sync hash was not found"))?;

        let end_height = end_header.height;
        if start_height > end_height {
            return Err(RpcStatus::bad_request(&format!(
                "Start block #{start_height} is higher than end block #{end_height}"
            )));
        }

        debug!(
            target: LOG_TARGET,
            "Initiating block sync with peer `{peer_node_id}` from height {start_height} to {end_height}"
        );

        let session_token = self.try_add_exclusive_session(peer_node_id).await?;
        let (tx, rx) = mpsc::channel(self.block_batch_size);
        let max_buffered_bytes = self.max_buffered_block_bytes;

        let span = span!(Level::TRACE, "sync_rpc::block_sync::inner_worker");
        task::spawn(
            async move {
                // Move token into this task
                let peer_node_id = session_token;
                // Blocks are fetched one at a time, so that the bytes buffered for this session can be checked before
                // each block is fetched
                let mut buffered = ChannelByteBudget::new(max_buffered_bytes);
                // Reorgs are detected from the chain itself, not from block events, which can arrive late, be lost
                // or announce a reorg that leaves the requested chain intact: after each block is fetched, the headers
                // at its height and at the end height are read together, from one chain state. If that chain has the
                // fetched block at its height and the requested end block at the end height, the fetched block is an
                // ancestor of the requested end block (blocks are hash-linked), so it is on the chain the peer asked
                // for, whatever reorgs happened in between. Reorgs above the end height do not affect the blocks
                // served.
                for height in start_height..=end_height {
                    if tx.is_closed() {
                        break;
                    }

                    // Ensure task stops if the peer prematurely stops their RPC session
                    if buffered.wait_for_room(&tx).await.is_err() {
                        debug!(
                            target: LOG_TARGET,
                            "Block sync session for peer '{peer_node_id}' terminated early"
                        );
                        break;
                    }

                    trace!(
                        target: LOG_TARGET,
                        "Sending block #{height} to '{peer_node_id}'"
                    );
                    let block = match db.fetch_block(height, true).await {
                        Ok(block) => block,
                        // The height was at or below the tip when the session started, so a missing block means the
                        // chain was rewound
                        Err(err) if err.is_value_not_found() || matches!(err, ChainStorageError::InvalidQuery(_)) => {
                            warn!(
                                target: LOG_TARGET,
                                "Block #{height} is gone during sync ({err}), letting the sync peer {peer_node_id} know."
                            );
                            let _result = tx
                                .send(Err(RpcStatus::conflict(&format!("Block #{height} is no longer on the chain"))))
                                .await;
                            break;
                        },
                        Err(err) => {
                            let _result = tx.send(Err(RpcStatus::log_internal_error(LOG_TARGET)(err))).await;
                            break;
                        },
                    };
                    let (header_at_height, end_header) = match db.fetch_header_pair(height, end_height).await {
                        Ok(headers) => headers,
                        Err(err) => {
                            let _result = tx.send(Err(RpcStatus::log_internal_error(LOG_TARGET)(err))).await;
                            break;
                        },
                    };
                    if !is_on_requested_chain(block.hash(), header_at_height.as_ref(), end_header.as_ref(), &end_hash) {
                        warn!(
                            target: LOG_TARGET,
                            "Block #{height} is no longer an ancestor of the requested end block #{end_height} during \
                             sync, letting the sync peer {peer_node_id} know that the chain has changed."
                        );
                        let _result = tx
                            .send(Err(RpcStatus::conflict("Reorg detected: the requested end block is no longer on the chain")))
                            .await;
                        break;
                    }
                    let response = match proto::base_node::BlockBodyResponse::try_from(block.into_block()) {
                        Ok(response) => response,
                        Err(e) => {
                            log::error!(target: LOG_TARGET, "Internal error: {e}");
                            let _result = tx.send(Err(RpcStatus::general_default())).await;
                            break;
                        },
                    };
                    let num_bytes = response.encoded_len();
                    if tx.send(Ok(response)).await.is_err() {
                        debug!(
                            target: LOG_TARGET,
                            "Block sync session for peer '{peer_node_id}' terminated early"
                        );
                        break;
                    }
                    buffered.record_sent(num_bytes);
                }

                #[cfg(feature = "metrics")]
                metrics::active_sync_peers().dec();
                debug!(
                    target: LOG_TARGET,
                    "Block sync round complete for peer `{peer_node_id}`."
                );
            }
            .instrument(span),
        );

        Ok(Streaming::new(rx))
    }

    #[instrument(level = "trace", name = "sync_rpc::sync_headers", skip(self), err)]
    #[allow(clippy::blocks_in_conditions)]
    async fn sync_headers(
        &self,
        request: Request<SyncHeadersRequest>,
    ) -> Result<Streaming<proto::core::BlockHeader>, RpcStatus> {
        let db = self.db();
        let peer_node_id = request.context().peer_node_id().clone();
        let message = request.into_message();
        let hash = message
            .start_hash
            .try_into()
            .map_err(|_| RpcStatus::bad_request(&"Malformed starting hash received".to_string()))?;
        let start_header = db
            .fetch_header_by_block_hash(hash)
            .await
            .rpc_status_internal_error(LOG_TARGET)?
            .ok_or_else(|| RpcStatus::not_found("Header not found with given hash"))?;

        let mut count = message.count;
        if count == 0 {
            let tip_header = db.fetch_tip_header().await.rpc_status_internal_error(LOG_TARGET)?;
            count = tip_header.height().saturating_sub(start_header.height);
        }
        // if its the start(tip_header == tip), return empty
        if count == 0 {
            return Ok(Streaming::empty());
        }

        #[allow(clippy::cast_possible_truncation)]
        let chunk_size = cmp::min(100, count) as usize;
        debug!(
            target: LOG_TARGET,
            "Initiating header sync with peer `{}` from height {} to {} (chunk_size={})",
            peer_node_id,
            start_header.height,
            count,
            chunk_size
        );

        let session_token = self.try_add_exclusive_session(peer_node_id.clone()).await?;
        let (tx, rx) = mpsc::channel(chunk_size);
        let span = span!(Level::TRACE, "sync_rpc::sync_headers::inner_worker");
        let iter = NonOverlappingIntegerPairIter::new(
            start_header.height.saturating_add(1),
            start_header.height.saturating_add(count).saturating_add(1),
            chunk_size,
        )
        .map_err(|e| RpcStatus::bad_request(&e))?;
        task::spawn(
            async move {
                // Move token into this task
                let peer_node_id = session_token;
                for (start, end) in iter {
                    if tx.is_closed() {
                        break;
                    }
                    debug!(target: LOG_TARGET, "Sending headers #{start} - #{end}");
                    let headers = db
                        .fetch_headers(start..=end)
                        .await
                        .map_err(RpcStatus::log_internal_error(LOG_TARGET));

                    if tx.is_closed() {
                        debug!(
                            target: LOG_TARGET,
                            "Header sync session for peer '{peer_node_id}' terminated early"
                        );
                        break;
                    }
                    match headers {
                        Ok(headers) if headers.is_empty() => {
                            break;
                        },
                        Ok(headers) => {
                            let headers = headers.into_iter().map(proto::core::BlockHeader::from).map(Ok);
                            // Ensure task stops if the peer prematurely stops their RPC session
                            if utils::mpsc::send_all(&tx, headers).await.is_err() {
                                break;
                            }
                        },
                        Err(err) => {
                            let _result = tx.send(Err(err)).await;
                            break;
                        },
                    }
                }

                #[cfg(feature = "metrics")]
                metrics::active_sync_peers().dec();
                debug!(
                    target: LOG_TARGET,
                    "Header sync round complete for peer `{peer_node_id}`."
                );
            }
            .instrument(span),
        );

        Ok(Streaming::new(rx))
    }

    #[instrument(level = "trace", skip(self), err)]
    #[allow(clippy::blocks_in_conditions)]
    async fn get_header_by_height(
        &self,
        request: Request<u64>,
    ) -> Result<Response<proto::core::BlockHeader>, RpcStatus> {
        let height = request.into_message();
        let header = self
            .db()
            .fetch_header(height)
            .await
            .rpc_status_internal_error(LOG_TARGET)?
            .ok_or_else(|| RpcStatus::not_found(&format!("Header not found at height {height}")))?;

        Ok(Response::new(header.into()))
    }

    #[instrument(level = "trace", skip(self), err(level = "debug"))]
    #[allow(clippy::blocks_in_conditions)]
    async fn find_chain_split(
        &self,
        request: Request<FindChainSplitRequest>,
    ) -> Result<Response<FindChainSplitResponse>, RpcStatus> {
        const MAX_ALLOWED_BLOCK_HASHES: usize = 1000;

        let peer = request.context().peer_node_id().clone();
        let message = request.into_message();
        if message.block_hashes.is_empty() {
            return Err(RpcStatus::bad_request(
                "Cannot find chain split because no hashes were sent",
            ));
        }
        if message.block_hashes.len() > MAX_ALLOWED_BLOCK_HASHES {
            return Err(RpcStatus::bad_request(&format!(
                "Cannot query more than {MAX_ALLOWED_BLOCK_HASHES} block hashes"
            )));
        }
        if message.header_count > (HEADER_SYNC_INITIAL_MAX_HEADERS as u64) {
            return Err(RpcStatus::bad_request(&format!(
                "Cannot ask for more than {HEADER_SYNC_INITIAL_MAX_HEADERS} headers"
            )));
        }

        let db = self.db();
        let hashes: Vec<FixedHash> = message
            .block_hashes
            .into_iter()
            .map(|hash| hash.try_into().map_err(|_| "Malformed pruned hash".to_string()))
            .collect::<Result<_, _>>()
            .map_err(|_| RpcStatus::bad_request(&"Malformed block hash received".to_string()))?;
        let maybe_headers = db
            .find_headers_after_hash(hashes, message.header_count)
            .await
            .rpc_status_internal_error(LOG_TARGET)?;
        match maybe_headers {
            Some((idx, headers)) => {
                debug!(
                    target: LOG_TARGET,
                    "Sending forked index {} and {} header(s) to peer `{}`",
                    idx,
                    headers.len(),
                    peer
                );

                Ok(Response::new(FindChainSplitResponse {
                    fork_hash_index: idx as u64,
                    headers: headers.into_iter().map(Into::into).collect(),
                }))
            },
            None => {
                debug!(
                    target: LOG_TARGET,
                    "Unable to find link to main chain from peer `{peer}`"
                );
                Err(RpcStatus::not_found("No link found to main chain"))
            },
        }
    }

    #[instrument(level = "trace", skip(self), err)]
    #[allow(clippy::blocks_in_conditions)]
    async fn get_chain_metadata(&self, _: Request<()>) -> Result<Response<proto::base_node::ChainMetadata>, RpcStatus> {
        let chain_metadata = self
            .db()
            .get_chain_metadata()
            .await
            .rpc_status_internal_error(LOG_TARGET)?;
        Ok(Response::new(chain_metadata.into()))
    }

    #[instrument(level = "trace", skip(self), err)]
    #[allow(clippy::blocks_in_conditions)]
    async fn sync_kernels(
        &self,
        request: Request<SyncKernelsRequest>,
    ) -> Result<Streaming<proto::types::TransactionKernel>, RpcStatus> {
        let peer_node_id = request.context().peer_node_id().clone();
        let req = request.into_message();
        let (tx, rx) = mpsc::channel(100);
        let db = self.db();

        let start_header = db
            .fetch_header_containing_kernel_mmr(req.start)
            .await
            .rpc_status_internal_error(LOG_TARGET)?
            .into_header();
        let hash = req
            .end_header_hash
            .try_into()
            .map_err(|_| RpcStatus::bad_request(&"Malformed end hash received".to_string()))?;
        let end_header = db
            .fetch_header_by_block_hash(hash)
            .await
            .rpc_status_internal_error(LOG_TARGET)?
            .ok_or_else(|| RpcStatus::not_found("Unknown end header"))?;

        let mut current_height = start_header.height;
        let end_height = end_header.height;
        let mut current_mmr_position = start_header.kernel_mmr_size;
        let mut current_header_hash = start_header.hash();

        if current_height > end_height {
            return Err(RpcStatus::bad_request("start header height is after end header"));
        }

        let session_token = self.try_add_exclusive_session(peer_node_id).await?;
        task::spawn(async move {
            // Move session token into task
            let peer_node_id = session_token;
            let mut header_chunk = Vec::new().into_iter();
            while current_height <= end_height {
                if tx.is_closed() {
                    break;
                }
                let res = db
                    .fetch_kernels_in_block(current_header_hash)
                    .await
                    .map_err(RpcStatus::log_internal_error(LOG_TARGET));

                if tx.is_closed() {
                    debug!(
                        target: LOG_TARGET,
                        "Kernel sync session for peer '{peer_node_id}' terminated early",
                    );
                    break;
                }

                match res {
                    Ok(kernels) if kernels.is_empty() => {
                        let _result = tx
                            .send(Err(RpcStatus::general(&format!(
                                "No kernels in block {}",
                                current_header_hash.to_hex()
                            ))))
                            .await;
                        break;
                    },
                    Ok(kernels) => {
                        debug!(
                            target: LOG_TARGET,
                            "Streaming kernels {} to {}",
                            current_mmr_position,
                            current_mmr_position.saturating_add(kernels.len() as u64)
                        );
                        current_mmr_position = current_mmr_position.saturating_add(kernels.len() as u64);
                        let kernels = kernels.into_iter().map(proto::types::TransactionKernel::from).map(Ok);
                        // Ensure task stops if the peer prematurely stops their RPC session
                        if utils::mpsc::send_all(&tx, kernels).await.is_err() {
                            break;
                        }
                    },
                    Err(err) => {
                        let _result = tx.send(Err(err)).await;
                        break;
                    },
                }

                current_height = current_height.saturating_add(1);

                if current_height <= end_height {
                    let res = match header_chunk.next() {
                        Some(header) => Ok(Some(header)),
                        None => {
                            // Prefetch the next chunk of headers. The chunk stops early at the first missing header.
                            let chunk_end = current_height.saturating_add(HEADER_CHUNK_SIZE - 1).min(end_height);
                            db.fetch_headers(current_height..=chunk_end)
                                .await
                                .map_err(RpcStatus::log_internal_error(LOG_TARGET))
                                .map(|headers| {
                                    header_chunk = headers.into_iter();
                                    header_chunk.next()
                                })
                        },
                    };
                    match res {
                        Ok(Some(header)) => {
                            current_header_hash = header.hash();
                        },
                        Ok(None) => {
                            let _result = tx
                                .send(Err(RpcStatus::not_found(&format!(
                                    "Could not find header #{current_height} while streaming UTXOs after position \
                                     {current_mmr_position}"
                                ))))
                                .await;
                            break;
                        },
                        Err(err) => {
                            error!(target: LOG_TARGET, "DB error while streaming kernels: {err}");
                            let _result = tx
                                .send(Err(RpcStatus::general("DB error while streaming kernels")))
                                .await;
                            break;
                        },
                    }
                }
            }

            #[cfg(feature = "metrics")]
            metrics::active_sync_peers().dec();
            debug!(
                target: LOG_TARGET,
                "Kernel sync round complete for peer `{peer_node_id}`.",
            );
        });
        Ok(Streaming::new(rx))
    }

    #[instrument(level = "trace", skip(self), err)]
    #[allow(clippy::blocks_in_conditions)]
    async fn sync_utxos(&self, request: Request<SyncUtxosRequest>) -> Result<Streaming<SyncUtxosResponse>, RpcStatus> {
        let req = request.message();
        let peer_node_id = request.context().peer_node_id();
        debug!(
            target: LOG_TARGET,
            "Received sync_utxos-{} request from header {} to {}",
            peer_node_id,
            req.start_header_hash.to_hex(),
            req.end_header_hash.to_hex(),
        );

        let session_token = self.try_add_exclusive_session(peer_node_id.clone()).await?;
        let (tx, rx) = mpsc::channel(200);
        let task = SyncUtxosTask::new(self.db(), session_token);
        task.run(request, tx).await?;

        Ok(Streaming::new(rx))
    }
}
