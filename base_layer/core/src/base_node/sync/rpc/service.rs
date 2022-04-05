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
    convert::TryFrom,
    sync::{Arc, Weak},
};

use log::*;
use tari_comms::{
    peer_manager::NodeId,
    protocol::rpc::{Request, Response, RpcStatus, RpcStatusResultExt, Streaming},
    utils,
};
use tari_crypto::tari_utilities::hex::Hex;
use tari_utilities::Hashable;
use tokio::{
    sync::{mpsc, RwLock},
    task,
};
use tracing::{instrument, span, Instrument, Level};

use crate::{
    base_node::{
        comms_interface::BlockEvent,
        metrics,
        sync::rpc::{sync_utxos_task::SyncUtxosTask, BaseNodeSyncService},
        LocalNodeCommsInterface,
    },
    chain_storage::{async_db::AsyncBlockchainDb, BlockAddResult, BlockchainBackend},
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

pub struct BaseNodeSyncRpcService<B> {
    db: AsyncBlockchainDb<B>,
    active_sessions: RwLock<Vec<Weak<NodeId>>>,
    base_node_service: LocalNodeCommsInterface,
}

impl<B: BlockchainBackend + 'static> BaseNodeSyncRpcService<B> {
    pub fn new(db: AsyncBlockchainDb<B>, base_node_service: LocalNodeCommsInterface) -> Self {
        Self {
            db,
            active_sessions: RwLock::new(Vec::new()),
            base_node_service,
        }
    }

    #[inline]
    fn db(&self) -> AsyncBlockchainDb<B> {
        self.db.clone()
    }

    pub async fn try_add_exclusive_session(&self, peer: NodeId) -> Result<Arc<NodeId>, RpcStatus> {
        let mut lock = self.active_sessions.write().await;
        *lock = lock.drain(..).filter(|l| l.strong_count() > 0).collect();
        debug!(target: LOG_TARGET, "Number of active sync sessions: {}", lock.len());

        if lock.iter().any(|p| p.upgrade().filter(|p| **p == peer).is_some()) {
            return Err(RpcStatus::forbidden(
                "Existing sync session found for this client. Only a single session is permitted",
            ));
        }

        let token = Arc::new(peer);
        lock.push(Arc::downgrade(&token));
        metrics::active_sync_peers().set(lock.len() as i64);
        Ok(token)
    }
}

#[tari_comms::async_trait]
impl<B: BlockchainBackend + 'static> BaseNodeSyncService for BaseNodeSyncRpcService<B> {
    #[instrument(level = "trace", name = "sync_rpc::sync_blocks", skip(self), err)]
    async fn sync_blocks(
        &self,
        request: Request<SyncBlocksRequest>,
    ) -> Result<Streaming<proto::base_node::BlockBodyResponse>, RpcStatus> {
        let peer_node_id = request.context().peer_node_id().clone();
        let message = request.into_message();
        let mut block_event_stream = self.base_node_service.get_block_event_stream();

        let db = self.db();
        let start_header = db
            .fetch_header_by_block_hash(message.start_hash)
            .await
            .rpc_status_internal_error(LOG_TARGET)?
            .ok_or_else(|| RpcStatus::not_found("Header not found with given hash"))?;

        let metadata = db.get_chain_metadata().await.rpc_status_internal_error(LOG_TARGET)?;

        let start_height = start_header.height + 1;
        if start_height < metadata.pruned_height() {
            return Err(RpcStatus::bad_request(&format!(
                "Requested full block body at height {}, however this node has an effective pruned height of {}",
                start_height,
                metadata.pruned_height()
            )));
        }

        if start_height > metadata.height_of_longest_chain() {
            return Ok(Streaming::empty());
        }

        let end_header = db
            .fetch_header_by_block_hash(message.end_hash)
            .await
            .rpc_status_internal_error(LOG_TARGET)?
            .ok_or_else(|| RpcStatus::not_found("Requested end block sync hash was not found"))?;

        let end_height = end_header.height;
        if start_height > end_height {
            return Err(RpcStatus::bad_request(&format!(
                "Start block #{} is higher than end block #{}",
                start_height, end_height
            )));
        }

        debug!(
            target: LOG_TARGET,
            "Initiating block sync with peer `{}` from height {} to {}", peer_node_id, start_height, end_height,
        );

        let session_token = self.try_add_exclusive_session(peer_node_id).await?;
        // Number of blocks to load and push to the stream before loading the next batch
        const BATCH_SIZE: usize = 2;
        let (tx, rx) = mpsc::channel(BATCH_SIZE);

        let span = span!(Level::TRACE, "sync_rpc::block_sync::inner_worker");
        task::spawn(
            async move {
                // Move token into this task
                let peer_node_id = session_token;
                let iter = NonOverlappingIntegerPairIter::new(start_height, end_height + 1, BATCH_SIZE);
                for (start, end) in iter {
                    if tx.is_closed() {
                        break;
                    }

                    // Check for reorgs during sync
                    while let Ok(block_event) = block_event_stream.try_recv() {
                        if let BlockEvent::ValidBlockAdded(_, BlockAddResult::ChainReorg { removed, .. }) =
                            &*block_event
                        {
                            if let Some(reorg_block) = removed
                                .iter()
                                // If the reorg happens before the end height of sync we let the peer know that the chain they are syncing with has changed
                                .find(|block| block.height() <= end_height)
                            {
                                warn!(
                                    target: LOG_TARGET,
                                    "Block reorg detected at height {} during sync, letting the sync peer {} know.",
                                    reorg_block.height(),
                                    peer_node_id
                                );
                                let _result = tx.send(Err(RpcStatus::conflict(&format!(
                                    "Reorg at height {} detected",
                                    reorg_block.height()
                                ))));
                                return;
                            }
                        }
                    }

                    debug!(
                        target: LOG_TARGET,
                        "Sending blocks #{} - #{} to '{}'", start, end, peer_node_id
                    );
                    let blocks = db
                        .fetch_blocks(start..=end)
                        .await
                        .map_err(RpcStatus::log_internal_error(LOG_TARGET));

                    if tx.is_closed() {
                        debug!(
                            target: LOG_TARGET,
                            "Block sync session for peer '{}' terminated early", peer_node_id
                        );
                        break;
                    }

                    match blocks {
                        Ok(blocks) if blocks.is_empty() => {
                            break;
                        },
                        Ok(blocks) => {
                            let blocks = blocks
                                .into_iter()
                                .map(|hb| {
                                    match hb.try_into_block().map_err(RpcStatus::log_internal_error(LOG_TARGET)) {
                                        Ok(b) => Ok(b.to_compact()),
                                        Err(e) => Err(e),
                                    }
                                })
                                .map(|block| match block {
                                    Ok(b) => proto::base_node::BlockBodyResponse::try_from(b).map_err(|e| {
                                        log::error!(target: LOG_TARGET, "Internal error: {}", e);
                                        RpcStatus::general_default()
                                    }),
                                    Err(err) => Err(err),
                                });

                            // Ensure task stops if the peer prematurely stops their RPC session
                            if utils::mpsc::send_all(&tx, blocks).await.is_err() {
                                debug!(
                                    target: LOG_TARGET,
                                    "Block sync session for peer '{}' terminated early", peer_node_id
                                );
                                break;
                            }
                        },
                        Err(err) => {
                            let _result = tx.send(Err(err)).await;
                            break;
                        },
                    }
                }

                metrics::active_sync_peers().dec();
                debug!(
                    target: LOG_TARGET,
                    "Block sync round complete for peer `{}`.", peer_node_id,
                );
            }
            .instrument(span),
        );

        Ok(Streaming::new(rx))
    }

    #[instrument(level = "trace", name = "sync_rpc::sync_headers", skip(self), err)]
    async fn sync_headers(
        &self,
        request: Request<SyncHeadersRequest>,
    ) -> Result<Streaming<proto::core::BlockHeader>, RpcStatus> {
        let db = self.db();
        let peer_node_id = request.context().peer_node_id().clone();
        let message = request.into_message();

        let start_header = db
            .fetch_header_by_block_hash(message.start_hash)
            .await
            .rpc_status_internal_error(LOG_TARGET)?
            .ok_or_else(|| RpcStatus::not_found("Header not found with given hash"))?;

        let mut count = message.count;
        if count == 0 {
            let tip_header = db.fetch_tip_header().await.rpc_status_internal_error(LOG_TARGET)?;
            count = tip_header.height().saturating_sub(start_header.height);
        }
        if count == 0 {
            return Ok(Streaming::empty());
        }

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
        task::spawn(
            async move {
                // Move token into this task
                let peer_node_id = session_token;
                let iter = NonOverlappingIntegerPairIter::new(
                    start_header.height + 1,
                    start_header.height.saturating_add(count).saturating_add(1),
                    chunk_size,
                );
                for (start, end) in iter {
                    if tx.is_closed() {
                        break;
                    }
                    debug!(target: LOG_TARGET, "Sending headers #{} - #{}", start, end);
                    let headers = db
                        .fetch_headers(start..=end)
                        .await
                        .map_err(RpcStatus::log_internal_error(LOG_TARGET));

                    if tx.is_closed() {
                        debug!(
                            target: LOG_TARGET,
                            "Header sync session for peer '{}' terminated early", peer_node_id
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

                metrics::active_sync_peers().dec();
                debug!(
                    target: LOG_TARGET,
                    "Header sync round complete for peer `{}`.", peer_node_id,
                );
            }
            .instrument(span),
        );

        Ok(Streaming::new(rx))
    }

    #[instrument(skip(self), err)]
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
            .ok_or_else(|| RpcStatus::not_found(&format!("Header not found at height {}", height)))?;

        Ok(Response::new(header.into()))
    }

    #[instrument(level = "debug", skip(self), err)]
    async fn find_chain_split(
        &self,
        request: Request<FindChainSplitRequest>,
    ) -> Result<Response<FindChainSplitResponse>, RpcStatus> {
        const MAX_ALLOWED_BLOCK_HASHES: usize = 1000;
        const MAX_ALLOWED_HEADER_COUNT: u64 = 1000;

        let peer = request.context().peer_node_id().clone();
        let message = request.into_message();
        if message.block_hashes.is_empty() {
            return Err(RpcStatus::bad_request(
                "Cannot find chain split because no hashes were sent",
            ));
        }
        if message.block_hashes.len() > MAX_ALLOWED_BLOCK_HASHES {
            return Err(RpcStatus::bad_request(&format!(
                "Cannot query more than {} block hashes",
                MAX_ALLOWED_BLOCK_HASHES,
            )));
        }
        if message.header_count > MAX_ALLOWED_HEADER_COUNT {
            return Err(RpcStatus::bad_request(&format!(
                "Cannot ask for more than {} headers",
                MAX_ALLOWED_HEADER_COUNT,
            )));
        }

        let db = self.db();
        let maybe_headers = db
            .find_headers_after_hash(message.block_hashes, message.header_count)
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
                let metadata = db.get_chain_metadata().await.rpc_status_internal_error(LOG_TARGET)?;

                Ok(Response::new(FindChainSplitResponse {
                    fork_hash_index: idx as u64,
                    headers: headers.into_iter().map(Into::into).collect(),
                    tip_height: metadata.height_of_longest_chain(),
                }))
            },
            None => {
                debug!(
                    target: LOG_TARGET,
                    "Unable to find link to main chain from peer `{}`", peer
                );
                Err(RpcStatus::not_found("No link found to main chain"))
            },
        }
    }

    #[instrument(skip(self), err)]
    async fn get_chain_metadata(&self, _: Request<()>) -> Result<Response<proto::base_node::ChainMetadata>, RpcStatus> {
        let chain_metadata = self
            .db()
            .get_chain_metadata()
            .await
            .rpc_status_internal_error(LOG_TARGET)?;
        Ok(Response::new(chain_metadata.into()))
    }

    #[instrument(skip(self), err)]
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

        let end_header = db
            .fetch_header_by_block_hash(req.end_header_hash.clone())
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
            while current_height <= end_height {
                if tx.is_closed() {
                    break;
                }
                let res = db
                    .fetch_kernels_in_block(current_header_hash.clone())
                    .await
                    .map_err(RpcStatus::log_internal_error(LOG_TARGET));

                if tx.is_closed() {
                    debug!(
                        target: LOG_TARGET,
                        "Kernel sync session for peer '{}' terminated early", peer_node_id
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
                            current_mmr_position + kernels.len() as u64
                        );
                        current_mmr_position += kernels.len() as u64;
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

                current_height += 1;

                if current_height <= end_height {
                    let res = db
                        .fetch_header(current_height)
                        .await
                        .map_err(RpcStatus::log_internal_error(LOG_TARGET));
                    match res {
                        Ok(Some(header)) => {
                            current_header_hash = header.hash();
                        },
                        Ok(None) => {
                            let _result = tx
                                .send(Err(RpcStatus::not_found(&format!(
                                    "Could not find header #{} while streaming UTXOs after position {}",
                                    current_height, current_mmr_position
                                ))))
                                .await;
                            break;
                        },
                        Err(err) => {
                            error!(target: LOG_TARGET, "DB error while streaming kernels: {}", err);
                            let _result = tx
                                .send(Err(RpcStatus::general("DB error while streaming kernels")))
                                .await;
                            break;
                        },
                    }
                }
            }

            metrics::active_sync_peers().dec();
            debug!(
                target: LOG_TARGET,
                "Kernel sync round complete for peer `{}`.", peer_node_id,
            );
        });
        Ok(Streaming::new(rx))
    }

    #[instrument(skip(self), err)]
    async fn sync_utxos(&self, request: Request<SyncUtxosRequest>) -> Result<Streaming<SyncUtxosResponse>, RpcStatus> {
        let req = request.message();
        let peer_node_id = request.context().peer_node_id();
        debug!(
            target: LOG_TARGET,
            "Received sync_utxos request from header {} to {} (start = {}, include_pruned_utxos = {}, \
             include_deleted_bitmaps = {})",
            peer_node_id,
            req.start,
            req.end_header_hash.to_hex(),
            req.include_pruned_utxos,
            req.include_deleted_bitmaps
        );

        let session_token = self.try_add_exclusive_session(peer_node_id.clone()).await?;
        let (tx, rx) = mpsc::channel(200);
        let task = SyncUtxosTask::new(self.db(), session_token);
        task.run(request, tx).await?;

        Ok(Streaming::new(rx))
    }
}
