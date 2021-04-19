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

use crate::{
    base_node::sync::rpc::BaseNodeSyncService,
    chain_storage::{async_db::AsyncBlockchainDb, BlockchainBackend, OrNotFound, PrunedOutput},
    iterators::NonOverlappingIntegerPairIter,
    proto,
    proto::base_node::{
        FindChainSplitRequest,
        FindChainSplitResponse,
        SyncBlocksRequest,
        SyncHeadersRequest,
        SyncKernelsRequest,
        SyncUtxo,
        SyncUtxos2Response,
        SyncUtxosRequest,
        SyncUtxosResponse,
    },
};
use futures::{channel::mpsc, stream, SinkExt};
use log::*;
use std::cmp;
use tari_comms::protocol::rpc::{Request, Response, RpcStatus, Streaming};
use tari_crypto::tari_utilities::hex::Hex;
use tokio::{task, time::Instant};

const LOG_TARGET: &str = "c::base_node::sync_rpc";

pub struct BaseNodeSyncRpcService<B> {
    db: AsyncBlockchainDb<B>,
}

impl<B: BlockchainBackend + 'static> BaseNodeSyncRpcService<B> {
    pub fn new(db: AsyncBlockchainDb<B>) -> Self {
        Self { db }
    }

    #[inline]
    fn db(&self) -> AsyncBlockchainDb<B> {
        self.db.clone()
    }
}

#[tari_comms::async_trait]
impl<B: BlockchainBackend + 'static> BaseNodeSyncService for BaseNodeSyncRpcService<B> {
    async fn sync_blocks(
        &self,
        request: Request<SyncBlocksRequest>,
    ) -> Result<Streaming<proto::base_node::BlockBodyResponse>, RpcStatus>
    {
        let peer_node_id = request.context().peer_node_id().clone();
        let message = request.into_message();

        let db = self.db();
        let start_header = db
            .fetch_header_by_block_hash(message.start_hash)
            .await
            .map_err(RpcStatus::log_internal_error(LOG_TARGET))?
            .ok_or_else(|| RpcStatus::not_found("Header not found with given hash"))?;

        let metadata = db
            .get_chain_metadata()
            .await
            .map_err(RpcStatus::log_internal_error(LOG_TARGET))?;

        info!(target: LOG_TARGET,"Request for blocks for sync from {} to hash:{}", start_header.height, message.end_hash.to_hex());
        let start = start_header.height + 1;
        if start < metadata.pruned_height() {
            warn!(target: LOG_TARGET, "Requested full block body at height {}, however this node has an effective pruned height of {}",
                  start,
                  metadata.pruned_height());
            return Err(RpcStatus::bad_request(format!(
                "Requested full block body at height {}, however this node has an effective pruned height of {}",
                start,
                metadata.pruned_height()
            )));
        }

        if start > metadata.height_of_longest_chain() {
            return Ok(Streaming::empty());
        }

        let end_header = db
            .fetch_header_by_block_hash(message.end_hash)
            .await
            .map_err(RpcStatus::log_internal_error(LOG_TARGET))?
            .ok_or_else(|| RpcStatus::not_found("Requested end block sync hash was not found"))?;

        let end = end_header.height;
        if start > end {
            warn!(target: LOG_TARGET,  "Start block #{} is higher than end block #{}",
                  start, end );
            return Err(RpcStatus::bad_request(format!(
                "Start block #{} is higher than end block #{}",
                start, end
            )));
        }

        info!(
            target: LOG_TARGET,
            "Initiating block sync with peer `{}` from height {} to {}", peer_node_id, start, end,
        );

        // Number of blocks to load and push to the stream before loading the next batch
        const BATCH_SIZE: usize = 4;
        let (mut tx, rx) = mpsc::channel(BATCH_SIZE);

        task::spawn(async move {
            let iter = NonOverlappingIntegerPairIter::new(start, end + 1, BATCH_SIZE);
            for (start, end) in iter {
                if tx.is_closed() {
                    break;
                }

                info!(target: LOG_TARGET, "Sending blocks #{} - #{}", start, end);
                let blocks = db
                    .fetch_blocks(start..=end)
                    .await
                    .map_err(RpcStatus::log_internal_error(LOG_TARGET));

                match blocks {
                    Ok(blocks) if blocks.is_empty() => {

                        info!(target: LOG_TARGET, "No blocks to send");
                        break;
                    },
                    Ok(blocks) => {
                        let mut blocks = stream::iter(
                            blocks
                                .into_iter()
                                .map(|hb| hb.try_into_block().map_err(RpcStatus::log_internal_error(LOG_TARGET)))
                                .map(|block| match block {
                                    Ok(b) => Ok(proto::base_node::BlockBodyResponse::from(b)),
                                    Err(err) => Err(err),
                                })
                                .map(Ok),
                        );

                        // Ensure task stops if the peer prematurely stops their RPC session
                        if tx.send_all(&mut blocks).await.is_err() {
                            break;
                        }
                    },
                    Err(err) => {
                        let _ = tx.send(Err(err)).await;
                        break;
                    },
                }
            }

            info!(
                target: LOG_TARGET,
                "Block sync round complete for peer `{}`.", peer_node_id,
            );
        });

        Ok(Streaming::new(rx))
    }

    async fn sync_headers(
        &self,
        request: Request<SyncHeadersRequest>,
    ) -> Result<Streaming<proto::core::BlockHeader>, RpcStatus>
    {
        let db = self.db();
        let peer_node_id = request.context().peer_node_id().clone();
        let message = request.into_message();

        let start_header = db
            .fetch_header_by_block_hash(message.start_hash)
            .await
            .map_err(RpcStatus::log_internal_error(LOG_TARGET))?
            .ok_or_else(|| RpcStatus::not_found("Header not found with given hash"))?;


        let tip_header = db
            .fetch_tip_header()
            .await
            .map_err(RpcStatus::log_internal_error(LOG_TARGET))?;

        let mut count = message.count;
        if count == 0 {
            count = tip_header.height().saturating_sub(start_header.height);
        }
        if count == 0 {
            return Ok(Streaming::empty());
        }

        // There may be more headers in our DB than blocks. Don't send these because we cannot
        // the body
        count = cmp::min(count, tip_header.height().saturating_sub(start_header.height));

        let chunk_size = cmp::min(100, count) as usize;
        info!(
            target: LOG_TARGET,
            "Initiating header sync with peer `{}` from height {} to {} (chunk_size={})",
            peer_node_id,
            start_header.height,
            count,
            chunk_size
        );

        let (mut tx, rx) = mpsc::channel(chunk_size);
        task::spawn(async move {
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

                match headers {
                    Ok(headers) if headers.is_empty() => {
                        break;
                    },
                    Ok(headers) => {
                        let mut headers =
                            stream::iter(headers.into_iter().map(proto::core::BlockHeader::from).map(Ok).map(Ok));
                        // Ensure task stops if the peer prematurely stops their RPC session
                        if tx.send_all(&mut headers).await.is_err() {
                            break;
                        }
                    },
                    Err(err) => {
                        let _ = tx.send(Err(err)).await;
                        break;
                    },
                }
            }

            debug!(
                target: LOG_TARGET,
                "Header sync round complete for peer `{}`.", peer_node_id,
            );
        });

        Ok(Streaming::new(rx))
    }

    async fn get_header_by_height(
        &self,
        request: Request<u64>,
    ) -> Result<Response<proto::core::BlockHeader>, RpcStatus>
    {
        let height = request.into_message();
        let header = self
            .db()
            .fetch_header(height)
            .await
            .map_err(RpcStatus::log_internal_error(LOG_TARGET))?
            .ok_or_else(|| RpcStatus::not_found(format!("Header not found at height {}", height)))?;

        Ok(Response::new(header.into()))
    }

    async fn find_chain_split(
        &self,
        request: Request<FindChainSplitRequest>,
    ) -> Result<Response<FindChainSplitResponse>, RpcStatus>
    {
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
            return Err(RpcStatus::bad_request(format!(
                "Cannot query more than {} block hashes",
                MAX_ALLOWED_BLOCK_HASHES,
            )));
        }
        if message.header_count > MAX_ALLOWED_HEADER_COUNT {
            return Err(RpcStatus::bad_request(format!(
                "Cannot ask for more than {} headers",
                MAX_ALLOWED_HEADER_COUNT,
            )));
        }

        let db = self.db();
        let maybe_headers = db
            .find_headers_after_hash(message.block_hashes, message.header_count)
            .await
            .map_err(RpcStatus::log_internal_error(LOG_TARGET))?;
        match maybe_headers {
            Some((idx, headers)) => {
                debug!(
                    target: LOG_TARGET,
                    "Sending forked index {} and {} header(s) to peer `{}`",
                    idx,
                    headers.len(),
                    peer
                );
                let metadata = db
                    .get_chain_metadata()
                    .await
                    .map_err(RpcStatus::log_internal_error(LOG_TARGET))?;


                // Only send up to the tip, otherwise we will not be able to provide the blocks
                let headers = headers.into_iter().filter(|h| h.height <= metadata.height_of_longest_chain()).map(Into::into).collect();

                Ok(Response::new(FindChainSplitResponse {
                    fork_hash_index: idx as u32,
                    headers,
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

    async fn get_chain_metadata(&self, _: Request<()>) -> Result<Response<proto::base_node::ChainMetadata>, RpcStatus> {
        let chain_metadata = self
            .db()
            .get_chain_metadata()
            .await
            .map_err(RpcStatus::log_internal_error(LOG_TARGET))?;
        Ok(Response::new(chain_metadata.into()))
    }

    async fn sync_kernels(
        &self,
        request: Request<SyncKernelsRequest>,
    ) -> Result<Streaming<proto::types::TransactionKernel>, RpcStatus>
    {
        let req = request.into_message();
        const BATCH_SIZE: usize = 1000;
        let (mut tx, rx) = mpsc::channel(BATCH_SIZE);
        let db = self.db();

        task::spawn(async move {
            let end = match db
                .fetch_chain_header_by_block_hash(req.end_header_hash.clone())
                .await
                .or_not_found("BlockHeader", "hash", req.end_header_hash.to_hex())
                .map_err(RpcStatus::log_internal_error(LOG_TARGET))
            {
                Ok(header) => {
                    if header.header.kernel_mmr_size < req.start {
                        let _ = tx
                            .send(Err(RpcStatus::bad_request("Start mmr position after requested header")))
                            .await;
                        return;
                    }

                    header.header.kernel_mmr_size
                },
                Err(err) => {
                    let _ = tx.send(Err(err)).await;
                    return;
                },
            };
            let iter = NonOverlappingIntegerPairIter::new(req.start, end, BATCH_SIZE);
            for (start, end) in iter {
                if tx.is_closed() {
                    break;
                }
                debug!(target: LOG_TARGET, "Streaming kernels {} to {}", start, end);
                let res = db
                    .fetch_kernels_by_mmr_position(start, end)
                    .await
                    .map_err(RpcStatus::log_internal_error(LOG_TARGET));
                match res {
                    Ok(kernels) if kernels.is_empty() => {
                        break;
                    },
                    Ok(kernels) => {
                        let mut kernels = stream::iter(
                            kernels
                                .into_iter()
                                .map(proto::types::TransactionKernel::from)
                                .map(Ok)
                                .map(Ok),
                        );
                        // Ensure task stops if the peer prematurely stops their RPC session
                        if tx.send_all(&mut kernels).await.is_err() {
                            break;
                        }
                    },
                    Err(err) => {
                        let _ = tx.send(Err(err)).await;
                        break;
                    },
                }
            }
        });
        Ok(Streaming::new(rx))
    }

    async fn sync_utxos(&self, request: Request<SyncUtxosRequest>) -> Result<Streaming<SyncUtxosResponse>, RpcStatus> {
        let peer = request.context().peer_node_id().clone();
        let req = request.into_message();
        const UTXOS_PER_BATCH: usize = 100;
        const BATCH_SIZE: usize = 100;
        let (mut tx, rx) = mpsc::channel(BATCH_SIZE);
        let db = self.db();

        task::spawn(async move {
            let timer = Instant::now();
            let end_header = match db
                .fetch_header_by_block_hash(req.end_header_hash.clone())
                .await
                .or_not_found("BlockHeader", "hash", req.end_header_hash.to_hex())
                .map_err(RpcStatus::log_internal_error(LOG_TARGET))
            {
                Ok(header) => header,
                Err(err) => {
                    let _ = tx.send(Err(err)).await;
                    return;
                },
            };

            let iter = NonOverlappingIntegerPairIter::new(req.start, end_header.output_mmr_size, UTXOS_PER_BATCH);
            let fetch_header_time = timer.elapsed().as_millis();
            let mut fetch_utxos_time = 0u128;
            for (start, end) in iter {
                let timer = Instant::now();
                if tx.is_closed() {
                    debug!(
                        target: LOG_TARGET,
                        "Exiting sync_utxos early because client ({}) has gone", peer
                    );
                    break;
                }
                debug!(target: LOG_TARGET, "Streaming utxos {} to {}", start, end);
                let res = db
                    .fetch_utxos_by_mmr_position(start, end, req.end_header_hash.clone())
                    .await
                    .map_err(RpcStatus::log_internal_error(LOG_TARGET));
                fetch_utxos_time += timer.elapsed().as_millis();
                match res {
                    Ok((utxos, deleted)) => {
                        if utxos.is_empty() {
                            break;
                        }
                        let response = SyncUtxosResponse {
                            utxos: utxos
                                .into_iter()
                                .map(|pruned_output| match pruned_output {
                                    PrunedOutput::Pruned {
                                        output_hash,
                                        range_proof_hash,
                                    } => SyncUtxo {
                                        output: None,
                                        hash: output_hash,
                                        rangeproof_hash: range_proof_hash,
                                    },
                                    PrunedOutput::NotPruned { output } => SyncUtxo {
                                        output: Some(output.into()),
                                        hash: vec![],
                                        rangeproof_hash: vec![],
                                    },
                                })
                                .collect(),
                            deleted_bitmaps: deleted.into_iter().map(|d| d.serialize()).collect(),
                        };

                        // Ensure task stops if the peer prematurely stops their RPC session
                        if tx.send(Ok(response)).await.is_err() {
                            break;
                        }
                    },
                    Err(err) => {
                        let _ = tx.send(Err(err)).await;
                        break;
                    },
                }

                debug!(
                    target: LOG_TARGET,
                    "Streamed utxos {} to {} in {:.2?}",
                    start,
                    end,
                    timer.elapsed()
                );
            }
            let send_utxos_time = timer.elapsed().as_millis() - fetch_header_time - fetch_utxos_time;
            trace!(
                target: LOG_TARGET,
                "Timings - Fetch header info from db: {} ms, Fetch UTXOs from db: {} ms, RPC send UTXO stream: {} ms",
                fetch_header_time,
                fetch_utxos_time,
                send_utxos_time,
            );
        });
        Ok(Streaming::new(rx))
    }

    async fn sync_utxos2(
        &self,
        request: Request<SyncUtxosRequest>,
    ) -> Result<Streaming<SyncUtxos2Response>, RpcStatus>
    {
        let peer = request.context().peer_node_id().clone();
        let req = request.into_message();
        const BATCH_SIZE: usize = 100;
        let (mut tx, rx) = mpsc::channel(BATCH_SIZE);
        let db = self.db();

        let end_header = db
            .fetch_header_by_block_hash(req.end_header_hash.clone())
            .await
            .map_err(RpcStatus::log_internal_error(LOG_TARGET))?
            .ok_or_else(|| RpcStatus::not_found("end_header_hash was not found"))?;

        debug!(
            target: LOG_TARGET,
            "Received sync_utxos request from {} (start = {}, include_pruned_utxos = {}, include_deleted_bitmaps = {})",
            peer,
            req.start,
            req.include_pruned_utxos,
            req.include_deleted_bitmaps
        );

        if req.start > end_header.output_mmr_size {
            return Err(RpcStatus::bad_request("start index is greater than end index"));
        }

        task::spawn(async move {
            let iter = NonOverlappingIntegerPairIter::new(req.start, end_header.output_mmr_size, BATCH_SIZE);
            for (start, end) in iter {
                let timer = Instant::now();
                if tx.is_closed() {
                    debug!(
                        target: LOG_TARGET,
                        "Exiting sync_utxos early because client ({}) has gone", peer
                    );
                    break;
                }
                debug!(target: LOG_TARGET, "Streaming utxos {} to {}", start, end);
                let res = db
                    .fetch_utxos_by_mmr_position(start, end, req.end_header_hash.clone())
                    .await
                    .map_err(RpcStatus::log_internal_error(LOG_TARGET));

                debug!(
                    target: LOG_TARGET,
                    "Fetched {} utxos in {:.2?}",
                    end - start,
                    timer.elapsed()
                );
                match res {
                    Ok((utxos, deleted)) => {
                        if utxos.is_empty() {
                            break;
                        }

                        let mut utxos = stream::iter(
                            utxos
                                .into_iter()
                                .enumerate()
                                // Only include pruned UTXOs if include_pruned_utxos is true
                                .filter(|(_, utxo)| req.include_pruned_utxos || !utxo.is_pruned())
                                .map(|(i, utxo)| {
                                    let utxo = proto::base_node::SyncUtxo2::from(utxo);
                                    proto::base_node::SyncUtxos2Response {
                                        utxo_or_deleted: Some(proto::base_node::sync_utxos2_response::UtxoOrDeleted::Utxo(
                                            utxo,
                                        )),
                                        mmr_index: start.saturating_add(i as u64),
                                    }
                                })
                                .map(Ok)
                                .map(Ok),
                        );

                        // Ensure task stops if the peer prematurely stops their RPC session
                        if tx.send_all(&mut utxos).await.is_err() {
                            break;
                        }

                        if req.include_deleted_bitmaps {
                            let bitmaps = deleted.into_iter().map(|b| b.serialize()).collect();
                            let bitmaps = proto::base_node::SyncUtxos2Response {
                                utxo_or_deleted: Some(
                                    proto::base_node::sync_utxos2_response::UtxoOrDeleted::DeletedBitmaps(
                                        proto::base_node::Bitmaps { bitmaps },
                                    ),
                                ),
                                mmr_index: 0,
                            };

                            if tx.send(Ok(bitmaps)).await.is_err() {
                                break;
                            }
                        }
                    },
                    Err(err) => {
                        let _ = tx.send(Err(err)).await;
                        break;
                    },
                }

                debug!(
                    target: LOG_TARGET,
                    "Streamed utxos {} to {} in {:.2?} (including stream backpressure)",
                    start,
                    end,
                    timer.elapsed()
                );
            }
        });

        Ok(Streaming::new(rx))
    }
}
