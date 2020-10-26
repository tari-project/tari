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

use super::{handle::BlockchainStateRequest, BlockchainStateServiceError};
use crate::{
    blocks::{Block, BlockHeader},
    chain_storage::{async_db, BlockchainBackend, BlockchainDatabase},
    transactions::types::HashOutput,
};
use futures::{
    channel::{mpsc, oneshot},
    future,
    future::{BoxFuture, Either},
    stream::{Fuse, FuturesUnordered},
    StreamExt,
    TryFutureExt,
};
use log::*;
use std::{future::Future, ops::Bound};
use tokio::future::poll_fn;

const LOG_TARGET: &str = "c::base_node::blockchain_state_service";

pub struct BlockchainStateService<B> {
    db: BlockchainDatabase<B>,
    requests: Fuse<mpsc::Receiver<BlockchainStateRequest>>,
}

impl<B: BlockchainBackend + 'static> BlockchainStateService<B> {
    pub fn new(db: BlockchainDatabase<B>, requests: mpsc::Receiver<BlockchainStateRequest>) -> Self {
        Self {
            db,
            requests: requests.fuse(),
        }
    }

    #[inline]
    fn db(&self) -> BlockchainDatabase<B> {
        self.db.clone()
    }

    pub async fn run(mut self) {
        // This "work queue" holds all the pending tasks. The reason this is a good fit for this service is that this
        // service is only for calls to the database wrapped in some ergonomics. All async_db calls spawn a
        // blocking task. So the work queue doesn't actually have to do much work itself, it is simply waiting
        // for the blocking task to notify it when it is complete, so polling many of these tasks sequentially is very
        // lightweight.
        let mut work_queue = FuturesUnordered::new();

        loop {
            futures::select! {
                // Add each request to a "work queue"
                request = self.requests.select_next_some() => {
                    let work = self.handle_request(request);
                    work_queue.push(work);
                },
                // Process the work queue
                _ = work_queue.select_next_some() => { },
            }
        }
    }

    fn handle_request(&mut self, request: BlockchainStateRequest) -> BoxFuture<'static, ()> {
        use BlockchainStateRequest::*;
        trace!(target: LOG_TARGET, "Got request: {:?}", request);
        match request {
            GetBlocks((start, end), reply) => {
                Box::pin(reply_or_cancel(reply, Self::fetch_blocks(self.db(), start, end)))
            },
            GetHeaders((start, end), reply) => {
                Box::pin(reply_or_cancel(reply, Self::fetch_headers(self.db(), start, end)))
            },
            GetHeaderByHash(hash, reply) => {
                Box::pin(reply_or_cancel(reply, Self::fetch_header_by_hash(self.db(), hash)))
            },
            GetHeaderByHeight(height, reply) => {
                Box::pin(reply_or_cancel(reply, Self::fetch_header_by_height(self.db(), height)))
            },
            GetChainMetadata(reply) => Box::pin(reply_or_cancel(
                reply,
                async_db::get_chain_metadata(self.db()).map_err(Into::into),
            )),
            FindHeadersAfterHash((hashes, count), reply) => Box::pin(reply_or_cancel(
                reply,
                Self::find_headers_after_hash(self.db(), hashes, count),
            )),
        }
    }

    fn fetch_header_by_height(
        db: BlockchainDatabase<B>,
        height: u64,
    ) -> impl Future<Output = Result<Option<BlockHeader>, BlockchainStateServiceError>>
    {
        async_db::fetch_header(db, height)
            .and_then(|h| future::ready(Ok(Some(h))))
            .or_else(|err| {
                future::ready({
                    if err.is_value_not_found() {
                        Ok(None)
                    } else {
                        Err(err.into())
                    }
                })
            })
    }

    fn fetch_header_by_hash(
        db: BlockchainDatabase<B>,
        hash: HashOutput,
    ) -> impl Future<Output = Result<Option<BlockHeader>, BlockchainStateServiceError>>
    {
        async_db::fetch_header_by_block_hash(db, hash)
            .and_then(|h| future::ready(Ok(Some(h))))
            .or_else(|err| {
                future::ready({
                    if err.is_value_not_found() {
                        Ok(None)
                    } else {
                        Err(err.into())
                    }
                })
            })
    }

    async fn fetch_blocks(
        db: BlockchainDatabase<B>,
        start: Bound<u64>,
        end: Bound<u64>,
    ) -> Result<Vec<Block>, BlockchainStateServiceError>
    {
        let (mut start, mut end) = convert_height_bounds(start, end);

        let mut metadata = None;
        if start.is_none() || end.is_none() {
            metadata = Some(async_db::get_chain_metadata(db.clone()).await?);
        }

        if start.is_none() {
            // `(..n)` means fetch blocks with the lowest height possible until `n`
            start = Some(metadata.as_ref().unwrap().effective_pruned_height);
        }
        if end.is_none() {
            // `(n..)` means fetch blocks until this node's tip
            end = Some(metadata.as_ref().unwrap().height_of_longest_chain());
        }

        let (start, end) = (start.unwrap(), end.unwrap());

        let blocks = async_db::fetch_blocks(db, start, end).await?;
        debug!(target: LOG_TARGET, "Fetched {} block(s)", blocks.len());

        // `HistoricalBlock`s are converted to `Block`s here because we're exposing a minimal required interface.
        // If the backends are refactored and it becomes an issue to include the extra historical data along with
        // the block, this will make that refactor easier because nothing can rely on that data if it isn't exposed.
        // If however, the historical data is needed, then exposing it here is a much easier refactor.
        Ok(blocks.into_iter().map(|b| b.block).collect())
    }

    async fn fetch_headers(
        db: BlockchainDatabase<B>,
        start: Bound<u64>,
        end: Bound<u64>,
    ) -> Result<Vec<BlockHeader>, BlockchainStateServiceError>
    {
        let (start, mut end) = convert_height_bounds(start, end);
        if end.is_none() {
            // `(n..)` means fetch block headers until this node's tip
            end = Some(
                async_db::get_chain_metadata(db.clone())
                    .await?
                    .height_of_longest_chain(),
            );
        }
        let (start, end) = (start.unwrap_or(0), end.unwrap());

        let headers = async_db::fetch_headers(db, start, end).await?;
        debug!(target: LOG_TARGET, "Fetched {} headers(s)", headers.len());
        Ok(headers)
    }

    async fn find_headers_after_hash(
        db: BlockchainDatabase<B>,
        ordered_hashes: Vec<HashOutput>,
        count: u64,
    ) -> Result<Option<(usize, Vec<BlockHeader>)>, BlockchainStateServiceError>
    {
        for (i, hash) in ordered_hashes.into_iter().enumerate() {
            match async_db::fetch_header_by_block_hash(db.clone(), hash).await {
                Ok(header) => {
                    if count == 0 {
                        return Ok(Some((i, Vec::new())));
                    }

                    let end_height = header.height.checked_add(count).ok_or_else(|| {
                        BlockchainStateServiceError::InvalidArguments {
                            func: "find_headers_after_hash",
                            arg: "count",
                            message: "count + block height will overflow u64".into(),
                        }
                    })?;
                    let headers = async_db::fetch_headers(db.clone(), header.height + 1, end_height).await?;
                    return Ok(Some((i, headers)));
                },
                Err(err) if err.is_value_not_found() => continue,
                Err(err) => return Err(err.into()),
            };
        }
        Ok(None)
    }
}

/// Polls both `fut` and the oneshot `poll_canceled` function. If the oneshot is cancelled before `fut` resolves, the
/// `fut` is not polled again and the function exits. Otherwise the result of the future is sent on the oneshot.
// TODO: This is how most if not all replies should be sent in services. Make available in service framework.
async fn reply_or_cancel<T, F>(mut reply: oneshot::Sender<Result<T, BlockchainStateServiceError>>, fut: F)
where F: Future<Output = Result<T, BlockchainStateServiceError>> {
    futures::pin_mut!(fut);
    let either = future::select(poll_fn(|cx| reply.poll_canceled(cx)), fut).await;
    match either {
        Either::Left((_, _)) => { /* Do nothing */ },
        Either::Right((v, _)) => {
            let _ = reply.send(v);
        },
    }
}

fn convert_height_bounds(start: Bound<u64>, end: Bound<u64>) -> (Option<u64>, Option<u64>) {
    use Bound::*;
    let start = match start {
        Included(n) => Some(n),
        Excluded(n) => Some(n.saturating_add(1)),
        Unbounded => None,
    };
    let end = match end {
        Included(n) => Some(n.saturating_add(1)),
        Excluded(n) => Some(n),
        // `(n..)` means fetch from the last block until `n`
        Unbounded => None,
    };

    (start, end)
}
