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
    base_node::{
        proto::base_node::{FindChainSplitRequest, FindChainSplitResponse, SyncBlocksRequest, SyncHeadersRequest},
        service::blockchain_state::BlockchainStateServiceHandle,
        sync_rpc::BaseNodeSyncService,
    },
    blocks::BlockHeader,
    iterators::NonOverlappingIntegerPairIter,
    proto::{generated as proto, generated::core::Block},
};
use futures::{channel::mpsc, stream, SinkExt};
use log::*;
use std::cmp;
use tari_comms::protocol::rpc::{Request, Response, RpcStatus, Streaming};
use tokio::task;

const LOG_TARGET: &str = "c::base_node::sync_rpc";

pub struct BaseNodeSyncRpcService {
    base_node: BlockchainStateServiceHandle,
}

impl BaseNodeSyncRpcService {
    pub fn new(base_node: BlockchainStateServiceHandle) -> Self {
        Self { base_node }
    }

    #[inline]
    fn base_node(&self) -> BlockchainStateServiceHandle {
        self.base_node.clone()
    }

    async fn get_start_header_and_end_height(
        &self,
        block_hash: Vec<u8>,
        mut count: u64,
    ) -> Result<(BlockHeader, u64), RpcStatus>
    {
        let mut base_node = self.base_node();
        let start_header = base_node
            .get_header_by_hash(block_hash)
            .await
            .map_err(RpcStatus::log_internal_error(LOG_TARGET))?
            .ok_or_else(|| RpcStatus::not_found("Header not found with given hash"))?;

        if count == 0 {
            let metadata = base_node
                .get_chain_metadata()
                .await
                .map_err(RpcStatus::log_internal_error(LOG_TARGET))?;
            count = metadata.height_of_longest_chain();
        }

        Ok((start_header, count))
    }
}

#[tari_comms::async_trait]
impl BaseNodeSyncService for BaseNodeSyncRpcService {
    async fn sync_blocks(
        &self,
        request: Request<SyncBlocksRequest>,
    ) -> Result<Streaming<proto::core::Block>, RpcStatus>
    {
        let peer_node_id = request.context().peer_node_id().clone();
        let message = request.into_message();

        let (start_header, count) = self
            .get_start_header_and_end_height(message.start_hash, message.count)
            .await?;

        let chunk_size = cmp::min(10, count) as usize;
        debug!(
            target: LOG_TARGET,
            "Initiating block sync with peer `{}` from height {} to {} (chunk_size={})",
            peer_node_id,
            start_header.height,
            Some(message.count)
                .filter(|n| *n != 0)
                .map(|n| n.to_string())
                .unwrap_or_else(|| "∞".to_string()),
            chunk_size
        );

        let (mut tx, rx) = mpsc::channel(chunk_size);

        let mut base_node = self.base_node();
        task::spawn(async move {
            let iter = NonOverlappingIntegerPairIter::new(
                start_header.height,
                start_header.height.saturating_add(count).saturating_add(1),
                chunk_size,
            );
            for (start, end) in iter {
                trace!(target: LOG_TARGET, "Sending blocks #{} - #{}", start, end);
                let blocks = base_node
                    .get_blocks(start..=end)
                    .await
                    .map_err(RpcStatus::log_internal_error(LOG_TARGET));

                match blocks {
                    Ok(blocks) if blocks.is_empty() => {
                        break;
                    },
                    Ok(blocks) => {
                        let mut blocks = stream::iter(blocks.into_iter().map(Block::from).map(Ok).map(Ok));
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

            debug!(
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
        let mut base_node = self.base_node();
        let peer_node_id = request.context().peer_node_id().clone();
        let message = request.into_message();

        let (start_header, count) = self
            .get_start_header_and_end_height(message.start_hash, message.count)
            .await?;

        let chunk_size = cmp::min(10, count) as usize;
        debug!(
            target: LOG_TARGET,
            "Initiating header sync with peer `{}` from height {} to {} (chunk_size={})",
            peer_node_id,
            start_header.height,
            Some(message.count)
                .filter(|n| *n != 0)
                .map(|n| n.to_string())
                .unwrap_or_else(|| "∞".to_string()),
            chunk_size
        );

        let (mut tx, rx) = mpsc::channel(chunk_size);
        task::spawn(async move {
            let iter = NonOverlappingIntegerPairIter::new(
                start_header.height,
                start_header.height.saturating_add(count).saturating_add(1),
                chunk_size,
            );
            for (start, end) in iter {
                trace!(target: LOG_TARGET, "Sending headers #{} - #{}", start, end);
                let headers = base_node
                    .get_headers(start..=end)
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
            .base_node()
            .get_header_by_height(height)
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
        const MAX_ALLOWED_BLOCK_HASHES: usize = 500;
        const MAX_ALLOWED_COUNT: u64 = 100;

        let message = request.into_message();
        if message.block_hashes.len() > MAX_ALLOWED_BLOCK_HASHES {
            return Err(RpcStatus::bad_request(format!(
                "Cannot query more than {} block hashes",
                MAX_ALLOWED_BLOCK_HASHES,
            )));
        }
        if message.count > MAX_ALLOWED_COUNT {
            return Err(RpcStatus::bad_request(format!(
                "Cannot ask for more than {} headers",
                MAX_ALLOWED_COUNT,
            )));
        }

        let mut base_node = self.base_node();
        let maybe_headers = base_node
            .find_headers_after_hash(message.block_hashes, message.count)
            .await
            .map_err(RpcStatus::log_internal_error(LOG_TARGET))?;
        match maybe_headers {
            Some((idx, headers)) => Ok(Response::new(FindChainSplitResponse {
                found_hash_index: idx as u32,
                headers: headers.into_iter().map(Into::into).collect(),
            })),
            None => Err(RpcStatus::not_found("No link found to main chain")),
        }
    }
}
