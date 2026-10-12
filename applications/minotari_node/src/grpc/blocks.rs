// Copyright 2019. The Tari Project
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
    cmp,
    net::SocketAddr,
    ops::RangeInclusive,
    pin::Pin,
    sync::{Arc, Mutex, MutexGuard},
    task::{Context, Poll, Waker},
    time::{Duration, Instant},
};

use futures::{SinkExt, Stream, StreamExt, channel::mpsc};
use log::*;
use minotari_app_grpc::tari_rpc;
use tari_core::{base_node::LocalNodeCommsInterface, iterators::NonOverlappingIntegerPairIter};
use tari_node_components::blocks::HistoricalBlock;
use tokio::{
    sync::{Notify, OwnedSemaphorePermit, Semaphore},
    time,
};
use tonic::Status;

use crate::grpc::base_node_grpc_server::obscure_error_if_true;

const LOG_TARGET: &str = "minotari::base_node::grpc";

// The maximum number of blocks that can be requested at a time. These will be streamed to the
// client, so memory is not really a concern here, but a malicious client could request a large
// number here to keep the node busy
pub const GET_BLOCKS_MAX_HEIGHTS: usize = 1000;

// The number of blocks to request from the base node at a time. This is to reduce the number of
// requests to the base node, but if you'd like to stream directly, this can be set to 1.
pub const GET_BLOCKS_PAGE_SIZE: usize = 10;

/// The most GetBlocks streams a node serves at once. A request over this fails straight away with RESOURCE_EXHAUSTED,
/// so that clients back off instead of piling up streams. Each stream holds at most about one page of hydrated blocks
/// plus the blocks in its response channel and the one being sent, i.e. `GET_BLOCKS_PAGE_SIZE + GET_BLOCKS_CHANNEL_SIZE
/// + 2` (13) blocks.
pub const GET_BLOCKS_MAX_CONCURRENT_STREAMS: usize = 8;

/// The number of blocks buffered in a GetBlocks response channel. The channel holds at most this plus one, because each
/// sender has a slot of its own. Kept small: the stream task already holds the rest of the page it is sending.
pub const GET_BLOCKS_CHANNEL_SIZE: usize = 1;

/// The least time between two warnings of the same kind about GetBlocks clients
const GET_BLOCKS_WARN_INTERVAL: Duration = Duration::from_secs(60);

/// Limits a kind of warning to one per [GET_BLOCKS_WARN_INTERVAL], so that a flood of requests cannot flood the log.
/// The warnings suppressed in between are counted and reported with the next one.
#[derive(Debug, Default)]
pub struct WarnLimiter {
    /// When the last warning was logged, and how many were suppressed since
    state: Mutex<(Option<Instant>, u64)>,
}

impl WarnLimiter {
    /// Returns the number of warnings suppressed since the last one if a warning may be logged now, otherwise `None`
    pub fn should_warn(&self) -> Option<u64> {
        self.should_warn_at(Instant::now())
    }

    fn should_warn_at(&self, now: Instant) -> Option<u64> {
        let mut state = match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let (last_warning, num_suppressed) = &mut *state;
        let may_warn = last_warning.is_none_or(|last| now.saturating_duration_since(last) >= GET_BLOCKS_WARN_INTERVAL);
        if may_warn {
            *last_warning = Some(now);
            let suppressed = *num_suppressed;
            *num_suppressed = 0;
            Some(suppressed)
        } else {
            *num_suppressed = num_suppressed.saturating_add(1);
            None
        }
    }
}

/// Formats a client's address for logging
pub fn display_client_addr(remote_addr: Option<SocketAddr>) -> String {
    remote_addr
        .map(|addr| addr.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

/// The longest a GetBlocks stream waits for the client to take a block. A client that stays connected but stops
/// reading would otherwise hold its stream slot forever, and enough of them would lock every other client out.
pub const GET_BLOCKS_SEND_TIMEOUT: Duration = Duration::from_secs(60);

/// Reserves one of the GetBlocks stream slots in `slots`. The slot is released when the returned permit is dropped.
#[allow(clippy::result_large_err)]
pub fn try_reserve_get_blocks_slot(slots: &Arc<Semaphore>) -> Result<OwnedSemaphorePermit, Status> {
    slots.clone().try_acquire_owned().map_err(|_| {
        Status::resource_exhausted(format!(
            "The node is already serving {GET_BLOCKS_MAX_CONCURRENT_STREAMS} GetBlocks streams. Try again later."
        ))
    })
}

/// Groups an ascending, sorted list of block heights into maximal runs of consecutive heights.
///
/// This lets a caller fetch only the heights that were actually requested instead of hydrating the entire span
/// between the lowest and the highest requested height. A fully contiguous request yields a single range that is
/// identical to `first..=last`, so the common case is unchanged.
///
/// Duplicate heights are tolerated (they are absorbed into the run they belong to). The input is expected to be
/// sorted ascending; an out-of-order height simply starts a new run.
pub fn contiguous_runs(sorted_heights: &[u64]) -> Vec<RangeInclusive<u64>> {
    let mut runs: Vec<RangeInclusive<u64>> = Vec::new();
    let mut iter = sorted_heights.iter().copied();
    let Some(first) = iter.next() else {
        return runs;
    };

    let mut start = first;
    let mut prev = first;
    for height in iter {
        if height == prev {
            // Duplicate height, nothing to extend
            continue;
        }
        if prev.checked_add(1) == Some(height) {
            prev = height;
            continue;
        }
        runs.push(start..=prev);
        start = height;
        prev = height;
    }
    runs.push(start..=prev);

    runs
}

/// Normalises the heights of a `GetBlocks` request into the ascending, deduplicated list of heights to serve.
///
/// An empty request means "the tip block"; a failure to read the chain metadata is a node health problem and is
/// reported to the client rather than being answered with a silent, empty stream. The result is capped at
/// `GET_BLOCKS_MAX_HEIGHTS`, which bounds the number of blocks a single request can make the node hydrate.
pub async fn resolve_requested_heights(
    mut handler: LocalNodeCommsInterface,
    mut heights: Vec<u64>,
    report_error_flag: bool,
) -> Result<Vec<u64>, Status> {
    if heights.is_empty() {
        let metadata = handler.get_metadata().await.map_err(|e| {
            warn!(
                target: LOG_TARGET,
                "[get_blocks] Could not get node tip: {e}"
            );
            obscure_error_if_true(report_error_flag, Status::internal(e.to_string()))
        })?;
        heights.push(metadata.best_block_height());
    }

    heights.truncate(GET_BLOCKS_MAX_HEIGHTS);
    heights.sort_unstable();
    heights.dedup();

    Ok(heights)
}

/// Splits an ascending, sorted list of block heights into the inclusive `(start, end)` pages that must be fetched
/// from the base node to serve exactly those heights.
///
/// Each maximal contiguous run of heights is paged independently with at most `page_size` heights per page, so no
/// unrequested block is ever fetched. A fully contiguous request produces exactly the same page sequence as paging
/// `first..=last` directly.
pub fn height_pages(sorted_heights: &[u64], page_size: usize) -> Result<Vec<(u64, u64)>, String> {
    let mut pages = Vec::new();
    for run in contiguous_runs(sorted_heights) {
        let (run_start, run_end) = (*run.start(), *run.end());
        pages.extend(NonOverlappingIntegerPairIter::new(
            run_start,
            run_end.saturating_add(1),
            page_size,
        )?);
        if run_end == u64::MAX {
            // The exclusive end of a run ending at `u64::MAX` is not representable, so the iterator stops one short
            // of it. Page the final height on its own.
            pages.push((u64::MAX, u64::MAX));
        }
    }
    Ok(pages)
}

/// The GetBlocks response stream
pub type GetBlocksStream = GetBlocksResponse;

/// What a GetBlocks stream task and its response stream share
struct GetBlocksState {
    /// The blocks waiting for the client. `None` once the stream task has cut the stream off, which drops the blocks
    /// that were still queued, or once the channel has ended.
    blocks: Option<mpsc::Receiver<Result<tari_rpc::HistoricalBlock, Status>>>,
    /// The error that ends the response, if the stream task cut the stream off
    final_error: Option<Status>,
    /// The response stream's waker, woken when the stream task cuts the stream off
    waker: Option<Waker>,
    /// The number of items the stream task has sent, or is sending, that the response has not taken yet
    num_queued: usize,
}

fn lock_state(state: &Mutex<GetBlocksState>) -> MutexGuard<'_, GetBlocksState> {
    match state.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// The response stream of a GetBlocks call: every block the stream task sends, then the error it cut the stream off
/// with, if any. Without that error a client would see a cut-off response end as if it were complete.
pub struct GetBlocksResponse {
    state: Arc<Mutex<GetBlocksState>>,
    /// Notified whenever the response takes an item, ends or is dropped
    progress: Arc<Notify>,
}

impl Stream for GetBlocksResponse {
    type Item = Result<tari_rpc::HistoricalBlock, Status>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut state = lock_state(&self.state);
        if let Some(blocks) = state.blocks.as_mut() {
            match blocks.poll_next_unpin(cx) {
                Poll::Ready(Some(item)) => {
                    state.num_queued = state.num_queued.saturating_sub(1);
                    self.progress.notify_one();
                    return Poll::Ready(Some(item));
                },
                Poll::Ready(None) => {
                    state.blocks = None;
                    self.progress.notify_one();
                },
                Poll::Pending => {
                    state.waker = Some(cx.waker().clone());
                    return Poll::Pending;
                },
            }
        }
        Poll::Ready(state.final_error.take().map(Err))
    }
}

impl Drop for GetBlocksResponse {
    /// Drops the receiver, so that the stream task sees that the client is gone
    fn drop(&mut self) {
        let blocks = lock_state(&self.state).blocks.take();
        drop(blocks);
        self.progress.notify_one();
    }
}

/// Lets a GetBlocks stream task track and cut off its stream
pub struct GetBlocksCutOff {
    state: Arc<Mutex<GetBlocksState>>,
    progress: Arc<Notify>,
}

impl GetBlocksCutOff {
    /// Records that the stream task is about to send an item
    fn add_queued(&self) {
        let mut state = lock_state(&self.state);
        state.num_queued = state.num_queued.saturating_add(1);
    }

    /// Returns true once the response holds nothing for the client: it has taken every item, ended or was dropped
    fn is_drained(&self) -> bool {
        let state = lock_state(&self.state);
        state.blocks.is_none() || state.num_queued == 0
    }

    /// Waits until the response has taken every item sent. A client that does not take an item for
    /// [GET_BLOCKS_SEND_TIMEOUT] has the stream cut off, which drops the items still queued. Returns true if the stream
    /// was cut off.
    async fn wait_until_drained(self) -> bool {
        loop {
            if self.is_drained() {
                return false;
            }
            // A notification sent since the check above is kept, so none is missed
            if time::timeout(GET_BLOCKS_SEND_TIMEOUT, self.progress.notified())
                .await
                .is_err()
            {
                self.cut_off(Status::deadline_exceeded(format!(
                    "The client did not read a block within {GET_BLOCKS_SEND_TIMEOUT:.0?}"
                )));
                return true;
            }
        }
    }

    /// Ends the response with `error` after the blocks already queued, once the stream task has dropped its sender
    fn end_with_error(&self, error: Status) {
        lock_state(&self.state).final_error = Some(error);
    }

    /// Ends the response with `error`. The blocks still queued for the client are dropped straight away, so a client
    /// that has stopped reading does not keep them in memory.
    pub fn cut_off(self, error: Status) {
        let mut state = lock_state(&self.state);
        let blocks = state.blocks.take();
        state.final_error = Some(error);
        let waker = state.waker.take();
        drop(state);
        drop(blocks);
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

/// Creates the channel for a GetBlocks stream: the sender and cut-off handle for the stream task, and the response
/// stream for the client
pub fn get_blocks_channel(
    buffer: usize,
) -> (
    mpsc::Sender<Result<tari_rpc::HistoricalBlock, Status>>,
    GetBlocksCutOff,
    GetBlocksResponse,
) {
    let (tx, rx) = mpsc::channel(buffer);
    let state = Arc::new(Mutex::new(GetBlocksState {
        blocks: Some(rx),
        final_error: None,
        waker: None,
        num_queued: 0,
    }));
    let progress = Arc::new(Notify::new());
    let cut_off = GetBlocksCutOff {
        state: state.clone(),
        progress: progress.clone(),
    };
    (tx, cut_off, GetBlocksResponse { state, progress })
}

/// Fetches each page of blocks in turn and streams them to the client.
///
/// Returns as soon as the receiver is gone, i.e. the client has cancelled the request or disconnected. Without that
/// early return the task would keep hydrating every remaining page from the database only to discard it, which is
/// what allowed a client that repeatedly timed out to accumulate work on the node.
///
/// If a page cannot be fetched, the response ends with an error status after the blocks already queued. If the client
/// does not take a block within [GET_BLOCKS_SEND_TIMEOUT], the stream is cut off with an error status, which also drops
/// the blocks still queued for the client (see [GetBlocksCutOff]). After the last block is sent, or a page fetch
/// failed, this waits until the client has taken every block, under the same timeout, so that `_slot`, the caller's
/// GetBlocks stream slot, is held for as long as blocks are queued for the client. The slot is released when this
/// returns or is dropped. Cut-offs are logged with `remote_addr`, at most one warning per [GET_BLOCKS_WARN_INTERVAL]
/// across streams sharing `cut_off_warnings`.
#[allow(clippy::too_many_arguments)]
pub async fn stream_blocks(
    mut handler: LocalNodeCommsInterface,
    pages: Vec<(u64, u64)>,
    mut tx: mpsc::Sender<Result<tari_rpc::HistoricalBlock, Status>>,
    cut_off: GetBlocksCutOff,
    report_error_flag: bool,
    _slot: OwnedSemaphorePermit,
    remote_addr: Option<SocketAddr>,
    cut_off_warnings: Arc<WarnLimiter>,
) {
    for (start, end) in pages {
        // A client that went away is otherwise only noticed when a block is sent, after the page was fetched
        if tx.is_closed() {
            debug!(
                target: LOG_TARGET,
                "[get_blocks] Request was cancelled, not fetching blocks #{start} - #{end}"
            );
            return;
        }
        let blocks = match handler.get_blocks(start..=end, false).await {
            Err(err) => {
                warn!(
                    target: LOG_TARGET,
                    "Error communicating with local base node: {err:?}"
                );
                // The client still gets the blocks already queued, then this error
                cut_off.end_with_error(obscure_error_if_true(
                    report_error_flag,
                    Status::internal(format!("Could not fetch blocks #{start} - #{end}: {err}")),
                ));
                break;
            },
            Ok(data) => data,
        };

        for block in blocks {
            trace!(
                target: LOG_TARGET,
                "GetBlock GRPC sending block #{}",
                block.header().height
            );
            let result = block.try_into().map_err(|err| {
                obscure_error_if_true(
                    report_error_flag,
                    Status::internal(format!("Could not provide block: {err}")),
                )
            });
            cut_off.add_queued();
            match time::timeout(GET_BLOCKS_SEND_TIMEOUT, tx.send(result)).await {
                Ok(Ok(())) => {},
                Ok(Err(_)) => {
                    warn!(
                        target: LOG_TARGET,
                        "[get_blocks] Request was cancelled while sending a response"
                    );
                    return;
                },
                Err(_) => {
                    cut_off.cut_off(Status::deadline_exceeded(format!(
                        "The client did not read a block within {GET_BLOCKS_SEND_TIMEOUT:.0?}"
                    )));
                    warn_cut_off(remote_addr, &cut_off_warnings);
                    return;
                },
            }
        }
    }

    // Close the channel so the response can end, then hold the slot until the client has taken the queued blocks
    drop(tx);
    if cut_off.wait_until_drained().await {
        warn_cut_off(remote_addr, &cut_off_warnings);
    }
}

/// Logs that a GetBlocks stream was cut off because its client did not read, rate-limited by `cut_off_warnings`
fn warn_cut_off(remote_addr: Option<SocketAddr>, cut_off_warnings: &WarnLimiter) {
    // A suppressed warning is counted and reported with the next one
    if let Some(num_suppressed) = cut_off_warnings.should_warn() {
        let client = display_client_addr(remote_addr);
        warn!(
            target: LOG_TARGET,
            "[get_blocks] Client {client} did not take a block within {GET_BLOCKS_SEND_TIMEOUT:.0?}. Ended its stream \
             and dropped the blocks queued for it ({num_suppressed} similar warning(s) suppressed)."
        );
    }
}

/// Magic number for input and output sizes
pub const BLOCK_INPUT_SIZE: u64 = 4;
pub const BLOCK_OUTPUT_SIZE: u64 = 13;

/// Returns the block heights based on the start and end heights or from_tip
pub async fn block_heights(
    mut handler: LocalNodeCommsInterface,
    start_height: u64,
    end_height: u64,
    from_tip: u64,
) -> Result<(u64, u64), Status> {
    if end_height > 0 {
        if start_height > end_height {
            return Err(Status::invalid_argument("Start height was greater than end height"));
        }
        Ok((start_height, end_height))
    } else if from_tip > 0 {
        let metadata = handler
            .get_metadata()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        let tip = metadata.best_block_height();
        // Avoid overflow
        let height_from_tip = cmp::min(tip, from_tip);
        let start = tip.saturating_sub(height_from_tip);
        Ok((start, tip))
    } else {
        Err(Status::invalid_argument("Invalid arguments provided"))
    }
}

pub fn block_size(block: &HistoricalBlock) -> u64 {
    let body = &block.block().body;

    let input_size = (body.inputs().len() as u64).saturating_mul(BLOCK_INPUT_SIZE);
    let output_size = (body.outputs().len() as u64).saturating_mul(BLOCK_OUTPUT_SIZE);
    input_size.saturating_add(output_size)
}

pub fn block_fees(block: &HistoricalBlock) -> u64 {
    let body = &block.block().body;
    body.kernels()
        .iter()
        .filter(|k| !k.is_coinbase())
        .map(|k| k.fee.into())
        .collect::<Vec<u64>>()
        .iter()
        .sum::<u64>()
}

#[cfg(test)]
mod test {
    use std::sync::{Arc, Mutex};

    use futures::StreamExt;
    use tari_common_types::{
        chain_metadata::ChainMetadata,
        types::{FixedHash, PrivateKey},
    };
    use tari_core::base_node::comms_interface::{CommsInterfaceError, NodeCommsRequest, NodeCommsResponse};
    use tari_node_components::blocks::{Block, BlockHeader, BlockHeaderAccumulatedData};
    use tari_service_framework::reply_channel;
    use tari_transaction_components::aggregated_body::AggregateBody;
    use tokio::{sync::broadcast, task};

    use super::*;

    /// The block ranges a mock base node was asked to fetch, in the order they were requested.
    type RequestedRanges = Arc<Mutex<Vec<RangeInclusive<u64>>>>;

    fn stub_block(height: u64) -> HistoricalBlock {
        let mut header = BlockHeader::new(0);
        header.height = height;
        HistoricalBlock::new(
            Block::new(header, AggregateBody::empty()),
            1,
            BlockHeaderAccumulatedData::genesis(FixedHash::zero(), PrivateKey::default()),
        )
    }

    /// Builds a real `LocalNodeCommsInterface` backed by a mock base node that records every block range it is asked
    /// for and answers with one stub block per height in that range. `tip` of `None` makes metadata lookups fail, as
    /// they would on a node whose database is unavailable.
    fn mock_node_service_with_tip(tip: Option<u64>) -> (LocalNodeCommsInterface, RequestedRanges) {
        let (request_tx, mut request_rx) = reply_channel::unbounded();
        let (block_tx, _block_rx) = reply_channel::unbounded();
        let (block_event_tx, _block_event_rx) = broadcast::channel(1);
        let requested: RequestedRanges = Arc::new(Mutex::new(Vec::new()));

        let recorded = requested.clone();
        task::spawn(async move {
            while let Some(request) = request_rx.next().await {
                let (request, reply) = request.split();
                let response = match request {
                    NodeCommsRequest::FetchMatchingBlocks { range, .. } => {
                        let blocks = range.clone().map(stub_block).collect::<Vec<_>>();
                        recorded.lock().unwrap().push(range);
                        Ok(NodeCommsResponse::HistoricalBlocks(blocks))
                    },
                    NodeCommsRequest::GetChainMetadata => match tip {
                        Some(height) => Ok(NodeCommsResponse::ChainMetadata(
                            ChainMetadata::new(height, FixedHash::zero(), 0, 0, 1.into(), 0).unwrap(),
                        )),
                        None => Err(CommsInterfaceError::UnexpectedApiResponse),
                    },
                    _ => Err(CommsInterfaceError::UnexpectedApiResponse),
                };
                let _result = reply.send(response);
            }
        });

        (
            LocalNodeCommsInterface::new(request_tx, block_tx, block_event_tx),
            requested,
        )
    }

    fn mock_node_service() -> (LocalNodeCommsInterface, RequestedRanges) {
        mock_node_service_with_tip(Some(0))
    }

    /// A GetBlocks stream slot that is not shared with any other test
    fn slot() -> OwnedSemaphorePermit {
        Arc::new(Semaphore::new(1)).try_acquire_owned().unwrap()
    }

    #[tokio::test]
    async fn resolve_requested_heights_defaults_to_the_tip() {
        let (handler, _requested) = mock_node_service_with_tip(Some(1234));
        assert_eq!(resolve_requested_heights(handler, vec![], true).await.unwrap(), vec![
            1234
        ]);
    }

    #[tokio::test]
    async fn resolve_requested_heights_reports_a_failed_tip_lookup() {
        // Regression: a failed metadata lookup used to leave `heights` empty, which produced a stream that closed
        // immediately with no items and no error, hiding a real node health problem from the client
        let (handler, _requested) = mock_node_service_with_tip(None);
        let err = resolve_requested_heights(handler, vec![], true).await.unwrap_err();
        assert_eq!(err.code(), tonic::Code::Internal);
    }

    #[tokio::test]
    async fn resolve_requested_heights_sorts_and_dedups() {
        let (handler, _requested) = mock_node_service();
        assert_eq!(
            resolve_requested_heights(handler, vec![9, 1, 9, 5, 1], true)
                .await
                .unwrap(),
            vec![1, 5, 9]
        );
    }

    #[tokio::test]
    async fn resolve_requested_heights_caps_the_number_of_blocks() {
        let (handler, _requested) = mock_node_service();
        let requested_heights = (0u64..5000).collect::<Vec<_>>();
        let resolved = resolve_requested_heights(handler, requested_heights, true)
            .await
            .unwrap();
        assert_eq!(resolved.len(), GET_BLOCKS_MAX_HEIGHTS);
        assert_eq!(resolved.first(), Some(&0));
    }

    #[tokio::test]
    async fn stream_blocks_fetches_only_the_two_requested_heights() {
        // The acceptance case at the `LocalNodeCommsInterface` boundary: a request for two far-apart heights must
        // not hydrate the 100_000 blocks between them
        let (handler, requested) = mock_node_service();
        let pages = height_pages(&[500, 100_500], GET_BLOCKS_PAGE_SIZE).unwrap();
        let (tx, cut_off, rx) = get_blocks_channel(GET_BLOCKS_PAGE_SIZE);

        let streaming = task::spawn(stream_blocks(
            handler,
            pages,
            tx,
            cut_off,
            true,
            slot(),
            None,
            Arc::default(),
        ));
        let streamed = rx.collect::<Vec<_>>().await;
        streaming.await.unwrap();

        assert_eq!(*requested.lock().unwrap(), vec![500..=500, 100_500..=100_500]);
        assert_eq!(streamed.len(), 2);
    }

    #[tokio::test]
    async fn stream_blocks_streams_every_page_of_a_contiguous_request() {
        let (handler, requested) = mock_node_service();
        let heights = (10u64..=34).collect::<Vec<_>>();
        let pages = height_pages(&heights, GET_BLOCKS_PAGE_SIZE).unwrap();
        let (tx, cut_off, rx) = get_blocks_channel(GET_BLOCKS_PAGE_SIZE);

        task::spawn(stream_blocks(
            handler,
            pages,
            tx,
            cut_off,
            true,
            slot(),
            None,
            Arc::default(),
        ));

        let streamed = rx.collect::<Vec<_>>().await;
        assert_eq!(streamed.len(), heights.len());
        assert_eq!(*requested.lock().unwrap(), vec![10..=19, 20..=29, 30..=34]);
    }

    #[tokio::test]
    async fn a_cancelled_stream_stops_before_the_next_page_fetch() {
        // A client that cancelled the request before any response could be sent: no page is fetched for it, rather
        // than one page that is only found to be unwanted when its first block is sent
        let (handler, requested) = mock_node_service();
        let heights = (1u64..=100).collect::<Vec<_>>();
        let pages = height_pages(&heights, GET_BLOCKS_PAGE_SIZE).unwrap();
        assert_eq!(pages.len(), 10);
        let (tx, cut_off, rx) = get_blocks_channel(GET_BLOCKS_PAGE_SIZE);
        drop(rx);

        stream_blocks(handler, pages, tx, cut_off, true, slot(), None, Arc::default()).await;

        assert!(requested.lock().unwrap().is_empty());
    }

    #[test]
    fn warnings_are_limited_to_one_per_interval() {
        let limiter = WarnLimiter::default();
        let start = Instant::now();
        assert_eq!(limiter.should_warn_at(start), Some(0));
        assert_eq!(limiter.should_warn_at(start + Duration::from_secs(1)), None);
        assert_eq!(limiter.should_warn_at(start + Duration::from_secs(2)), None);
        // The next warning reports how many were suppressed
        assert_eq!(limiter.should_warn_at(start + GET_BLOCKS_WARN_INTERVAL), Some(2));
        assert_eq!(limiter.should_warn_at(start + GET_BLOCKS_WARN_INTERVAL), None);
    }

    /// A stream whose client is not reading holds about one page: the rest of the page it is sending, the response
    /// channel and the block being sent
    #[tokio::test]
    async fn a_stream_holds_about_one_page_for_a_client_that_is_not_reading() {
        let (handler, requested) = mock_node_service();
        let heights = (1u64..=100).collect::<Vec<_>>();
        let pages = height_pages(&heights, GET_BLOCKS_PAGE_SIZE).unwrap();
        let (tx, cut_off, response) = get_blocks_channel(GET_BLOCKS_CHANNEL_SIZE);

        let streaming = task::spawn(stream_blocks(
            handler,
            pages,
            tx,
            cut_off,
            true,
            slot(),
            None,
            Arc::default(),
        ));
        for _ in 0..100 {
            task::yield_now().await;
        }

        // Only the first page was fetched, and only the channel and the send in progress hold its converted blocks
        assert_eq!(*requested.lock().unwrap(), vec![1..=10]);
        let num_queued = lock_state(&response.state).num_queued;
        assert!(num_queued >= 1);
        assert!(num_queued <= GET_BLOCKS_CHANNEL_SIZE + 2, "{num_queued} blocks queued");
        streaming.abort();
    }

    #[test]
    fn the_request_over_the_stream_cap_gets_resource_exhausted() {
        let slots = Arc::new(Semaphore::new(GET_BLOCKS_MAX_CONCURRENT_STREAMS));
        let held = (0..GET_BLOCKS_MAX_CONCURRENT_STREAMS)
            .map(|_| try_reserve_get_blocks_slot(&slots).unwrap())
            .collect::<Vec<_>>();
        let err = try_reserve_get_blocks_slot(&slots).unwrap_err();
        assert_eq!(err.code(), tonic::Code::ResourceExhausted);

        drop(held);
        assert!(try_reserve_get_blocks_slot(&slots).is_ok());
    }

    #[tokio::test(start_paused = true)]
    async fn a_slot_is_released_when_the_client_has_read_a_complete_stream() {
        let slots = Arc::new(Semaphore::new(GET_BLOCKS_MAX_CONCURRENT_STREAMS));
        let (handler, _requested) = mock_node_service();
        let pages = height_pages(&[1, 2, 3], GET_BLOCKS_PAGE_SIZE).unwrap();
        let (tx, cut_off, rx) = get_blocks_channel(GET_BLOCKS_PAGE_SIZE);

        let slot = try_reserve_get_blocks_slot(&slots).unwrap();
        assert_eq!(slots.available_permits(), GET_BLOCKS_MAX_CONCURRENT_STREAMS - 1);
        let streaming = task::spawn(stream_blocks(
            handler,
            pages,
            tx,
            cut_off,
            true,
            slot,
            None,
            Arc::default(),
        ));

        // Read to the end, keeping the response alive as a server does until it has sent the end of the stream
        let mut rx = rx;
        let mut num_read = 0usize;
        while let Some(item) = rx.next().await {
            assert!(item.is_ok());
            num_read = num_read.saturating_add(1);
        }
        assert_eq!(num_read, 3);
        // Time is paused, so a stream still waiting for the client would not end within this second
        time::timeout(Duration::from_secs(1), streaming)
            .await
            .expect("the slot was not released once the client had read everything")
            .unwrap();
        assert_eq!(slots.available_permits(), GET_BLOCKS_MAX_CONCURRENT_STREAMS);
    }

    #[tokio::test(start_paused = true)]
    async fn a_completed_stream_holds_its_slot_until_the_client_reads_the_queued_blocks() {
        let slots = Arc::new(Semaphore::new(GET_BLOCKS_MAX_CONCURRENT_STREAMS));
        let (handler, requested) = mock_node_service();
        let pages = height_pages(&[1, 2, 3], GET_BLOCKS_PAGE_SIZE).unwrap();
        // Every block fits in the channel, so the stream task sends them all without waiting
        let (tx, cut_off, response) = get_blocks_channel(GET_BLOCKS_PAGE_SIZE);

        let slot = try_reserve_get_blocks_slot(&slots).unwrap();
        let streaming = task::spawn(stream_blocks(
            handler,
            pages,
            tx,
            cut_off,
            true,
            slot,
            None,
            Arc::default(),
        ));
        while requested.lock().unwrap().is_empty() {
            task::yield_now().await;
        }
        time::sleep(GET_BLOCKS_SEND_TIMEOUT / 2).await;
        assert!(!streaming.is_finished());
        assert_eq!(slots.available_permits(), GET_BLOCKS_MAX_CONCURRENT_STREAMS - 1);

        // The client never reads, so the queued blocks are dropped and the slot is released after the timeout
        time::timeout(GET_BLOCKS_SEND_TIMEOUT, streaming)
            .await
            .expect("the stream did not give up on a client that is not reading")
            .unwrap();
        assert_eq!(slots.available_permits(), GET_BLOCKS_MAX_CONCURRENT_STREAMS);
        assert!(lock_state(&response.state).blocks.is_none());
        let responses = response.collect::<Vec<_>>().await;
        assert_eq!(responses.len(), 1);
        assert_eq!(
            responses.last().unwrap().as_ref().unwrap_err().code(),
            tonic::Code::DeadlineExceeded
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_completed_stream_releases_its_slot_when_the_response_is_dropped() {
        let slots = Arc::new(Semaphore::new(GET_BLOCKS_MAX_CONCURRENT_STREAMS));
        let (handler, requested) = mock_node_service();
        let pages = height_pages(&[1, 2, 3], GET_BLOCKS_PAGE_SIZE).unwrap();
        let (tx, cut_off, response) = get_blocks_channel(GET_BLOCKS_PAGE_SIZE);

        let slot = try_reserve_get_blocks_slot(&slots).unwrap();
        let streaming = task::spawn(stream_blocks(
            handler,
            pages,
            tx,
            cut_off,
            true,
            slot,
            None,
            Arc::default(),
        ));
        while requested.lock().unwrap().is_empty() {
            task::yield_now().await;
        }
        time::sleep(Duration::from_secs(1)).await;
        assert!(!streaming.is_finished());

        drop(response);
        time::timeout(Duration::from_secs(1), streaming)
            .await
            .expect("the slot was not released when the response was dropped")
            .unwrap();
        assert_eq!(slots.available_permits(), GET_BLOCKS_MAX_CONCURRENT_STREAMS);
    }

    #[tokio::test]
    async fn a_slot_is_released_when_a_stream_errors() {
        let slots = Arc::new(Semaphore::new(GET_BLOCKS_MAX_CONCURRENT_STREAMS));
        // A base node that is gone, so fetching the first page fails
        let (request_tx, _request_rx) = reply_channel::unbounded();
        let (block_tx, _block_rx) = reply_channel::unbounded();
        let (block_event_tx, _block_event_rx) = broadcast::channel(1);
        let handler = LocalNodeCommsInterface::new(request_tx, block_tx, block_event_tx);
        drop(_request_rx);
        let pages = height_pages(&[1, 2, 3], GET_BLOCKS_PAGE_SIZE).unwrap();
        let (tx, cut_off, rx) = get_blocks_channel(GET_BLOCKS_PAGE_SIZE);

        let slot = try_reserve_get_blocks_slot(&slots).unwrap();
        stream_blocks(handler, pages, tx, cut_off, true, slot, None, Arc::default()).await;

        assert_eq!(slots.available_permits(), GET_BLOCKS_MAX_CONCURRENT_STREAMS);
        let responses = rx.collect::<Vec<_>>().await;
        assert_eq!(responses.len(), 1);
        assert!(responses.iter().all(|r| r.is_err()));
    }

    #[tokio::test(start_paused = true)]
    async fn a_slot_is_released_when_the_client_stops_reading() {
        let slots = Arc::new(Semaphore::new(GET_BLOCKS_MAX_CONCURRENT_STREAMS));
        let (handler, requested) = mock_node_service();
        let heights = (1u64..=100).collect::<Vec<_>>();
        let pages = height_pages(&heights, GET_BLOCKS_PAGE_SIZE).unwrap();
        // The client keeps the receiver but never reads, so the stream waits on a full channel
        let (tx, cut_off, _rx) = get_blocks_channel(1);

        let slot = try_reserve_get_blocks_slot(&slots).unwrap();
        let streaming = task::spawn(stream_blocks(
            handler,
            pages,
            tx,
            cut_off,
            true,
            slot,
            None,
            Arc::default(),
        ));
        while requested.lock().unwrap().is_empty() {
            task::yield_now().await;
        }
        assert_eq!(slots.available_permits(), GET_BLOCKS_MAX_CONCURRENT_STREAMS - 1);

        // Time is paused and advances once every task is idle, so this returns once the send has timed out
        time::timeout(GET_BLOCKS_SEND_TIMEOUT * 2, streaming)
            .await
            .expect("the stream did not give up on a client that stopped reading")
            .unwrap();
        assert_eq!(slots.available_permits(), GET_BLOCKS_MAX_CONCURRENT_STREAMS);
        assert_eq!(*requested.lock().unwrap(), vec![1..=10]);
    }

    #[tokio::test(start_paused = true)]
    async fn a_stream_cut_off_by_the_send_timeout_drops_the_queued_blocks_and_ends_with_an_error() {
        let slots = Arc::new(Semaphore::new(GET_BLOCKS_MAX_CONCURRENT_STREAMS));
        let (handler, _requested) = mock_node_service();
        let heights = (1u64..=100).collect::<Vec<_>>();
        let pages = height_pages(&heights, GET_BLOCKS_PAGE_SIZE).unwrap();
        // The client never reads, so the stream fills the channel and waits
        let (tx, cut_off, response) = get_blocks_channel(1);

        let slot = try_reserve_get_blocks_slot(&slots).unwrap();
        let streaming = task::spawn(stream_blocks(
            handler,
            pages,
            tx,
            cut_off,
            true,
            slot,
            None,
            Arc::default(),
        ));
        time::timeout(GET_BLOCKS_SEND_TIMEOUT * 2, streaming)
            .await
            .expect("the send did not time out")
            .unwrap();

        // The queued blocks were dropped without the response being polled, and the slot is free
        assert!(lock_state(&response.state).blocks.is_none());
        assert_eq!(slots.available_permits(), GET_BLOCKS_MAX_CONCURRENT_STREAMS);

        let responses = response.collect::<Vec<_>>().await;
        assert_eq!(responses.len(), 1);
        let err = responses.last().unwrap().as_ref().unwrap_err();
        assert_eq!(err.code(), tonic::Code::DeadlineExceeded);
    }

    #[tokio::test]
    async fn a_waiting_response_is_woken_when_the_stream_is_cut_off() {
        let (_tx, cut_off, mut response) = get_blocks_channel(1);
        let next = task::spawn(async move { response.next().await });
        task::yield_now().await;
        assert!(!next.is_finished());

        cut_off.cut_off(Status::deadline_exceeded("test"));

        let item = time::timeout(Duration::from_secs(10), next)
            .await
            .expect("the waiting response was not woken")
            .unwrap();
        assert_eq!(item.unwrap().unwrap_err().code(), tonic::Code::DeadlineExceeded);
    }

    /// A mock base node that serves the first page and then fails, so the second page fetch fails
    fn mock_node_service_failing_after_one_page() -> LocalNodeCommsInterface {
        let (request_tx, mut request_rx) = reply_channel::unbounded();
        let (block_tx, _block_rx) = reply_channel::unbounded();
        let (block_event_tx, _block_event_rx) = broadcast::channel(1);
        task::spawn(async move {
            if let Some(request) = request_rx.next().await {
                let (request, reply) = request.split();
                let response = match request {
                    NodeCommsRequest::FetchMatchingBlocks { range, .. } => {
                        Ok(NodeCommsResponse::HistoricalBlocks(range.map(stub_block).collect()))
                    },
                    _ => Err(CommsInterfaceError::UnexpectedApiResponse),
                };
                let _result = reply.send(response);
            }
            // Dropping the request receiver fails every later request
        });
        LocalNodeCommsInterface::new(request_tx, block_tx, block_event_tx)
    }

    #[tokio::test]
    async fn a_page_fetch_error_ends_the_stream_with_an_error_after_the_queued_blocks() {
        let handler = mock_node_service_failing_after_one_page();
        let pages = height_pages(&[1, 2, 3, 20], GET_BLOCKS_PAGE_SIZE).unwrap();
        assert_eq!(pages.len(), 2);
        let (tx, cut_off, response) = get_blocks_channel(GET_BLOCKS_PAGE_SIZE);

        let streaming = task::spawn(stream_blocks(
            handler,
            pages,
            tx,
            cut_off,
            true,
            slot(),
            None,
            Arc::default(),
        ));
        let responses = response.collect::<Vec<_>>().await;
        streaming.await.unwrap();

        assert_eq!(responses.len(), 4);
        assert!(responses.iter().take(3).all(|r| r.is_ok()));
        let err = responses.last().unwrap().as_ref().unwrap_err();
        assert_eq!(err.code(), tonic::Code::Internal);
    }

    #[tokio::test(start_paused = true)]
    async fn a_client_that_is_not_reading_after_a_page_fetch_error_is_cut_off() {
        let slots = Arc::new(Semaphore::new(GET_BLOCKS_MAX_CONCURRENT_STREAMS));
        let handler = mock_node_service_failing_after_one_page();
        let pages = height_pages(&[1, 2, 3, 20], GET_BLOCKS_PAGE_SIZE).unwrap();
        let (tx, cut_off, response) = get_blocks_channel(GET_BLOCKS_PAGE_SIZE);

        let slot = try_reserve_get_blocks_slot(&slots).unwrap();
        let streaming = task::spawn(stream_blocks(
            handler,
            pages,
            tx,
            cut_off,
            true,
            slot,
            None,
            Arc::default(),
        ));
        time::sleep(GET_BLOCKS_SEND_TIMEOUT / 2).await;
        assert!(!streaming.is_finished());
        assert_eq!(slots.available_permits(), GET_BLOCKS_MAX_CONCURRENT_STREAMS - 1);

        time::timeout(GET_BLOCKS_SEND_TIMEOUT, streaming)
            .await
            .expect("the stream did not give up on a client that is not reading")
            .unwrap();
        assert_eq!(slots.available_permits(), GET_BLOCKS_MAX_CONCURRENT_STREAMS);
        assert!(lock_state(&response.state).blocks.is_none());
        let responses = response.collect::<Vec<_>>().await;
        assert_eq!(responses.len(), 1);
        assert_eq!(
            responses.last().unwrap().as_ref().unwrap_err().code(),
            tonic::Code::DeadlineExceeded
        );
    }

    #[tokio::test]
    async fn a_complete_stream_ends_without_an_error() {
        let (handler, _requested) = mock_node_service();
        let pages = height_pages(&[1, 2, 3], GET_BLOCKS_PAGE_SIZE).unwrap();
        let (tx, cut_off, response) = get_blocks_channel(GET_BLOCKS_PAGE_SIZE);

        let streaming = task::spawn(stream_blocks(
            handler,
            pages,
            tx,
            cut_off,
            true,
            slot(),
            None,
            Arc::default(),
        ));
        let responses = response.collect::<Vec<_>>().await;
        streaming.await.unwrap();

        assert_eq!(responses.len(), 3);
        assert!(responses.iter().all(|r| r.is_ok()));
    }

    #[tokio::test]
    async fn a_slot_is_released_when_a_stream_is_dropped() {
        let slots = Arc::new(Semaphore::new(GET_BLOCKS_MAX_CONCURRENT_STREAMS));
        let (handler, requested) = mock_node_service();
        let heights = (1u64..=100).collect::<Vec<_>>();
        let pages = height_pages(&heights, GET_BLOCKS_PAGE_SIZE).unwrap();
        // The client never reads, so the stream waits on a full channel
        let (tx, cut_off, _rx) = get_blocks_channel(1);

        let slot = try_reserve_get_blocks_slot(&slots).unwrap();
        let streaming = task::spawn(stream_blocks(
            handler,
            pages,
            tx,
            cut_off,
            true,
            slot,
            None,
            Arc::default(),
        ));
        while requested.lock().unwrap().is_empty() {
            task::yield_now().await;
        }
        assert_eq!(slots.available_permits(), GET_BLOCKS_MAX_CONCURRENT_STREAMS - 1);

        streaming.abort();
        assert!(streaming.await.unwrap_err().is_cancelled());
        assert_eq!(slots.available_permits(), GET_BLOCKS_MAX_CONCURRENT_STREAMS);
    }

    #[tokio::test]
    async fn stream_blocks_stops_fetching_when_the_client_disconnects_mid_stream() {
        // The client reads the first page and then goes away. No page after the one in flight may be fetched.
        let (handler, requested) = mock_node_service();
        let heights = (1u64..=100).collect::<Vec<_>>();
        let pages = height_pages(&heights, GET_BLOCKS_PAGE_SIZE).unwrap();
        let (tx, cut_off, mut rx) = get_blocks_channel(GET_BLOCKS_PAGE_SIZE);

        let streaming = task::spawn(stream_blocks(
            handler,
            pages,
            tx,
            cut_off,
            true,
            slot(),
            None,
            Arc::default(),
        ));
        for _ in 0..GET_BLOCKS_PAGE_SIZE {
            assert!(rx.next().await.is_some());
        }
        drop(rx);
        streaming.await.unwrap();

        // At most the page that was already in flight when the receiver was dropped
        let fetched = requested.lock().unwrap().len();
        assert!(
            fetched <= 2,
            "expected the stream to stop, but {fetched} pages were fetched"
        );
    }

    #[tokio::test]
    async fn stream_blocks_with_no_pages_fetches_nothing() {
        let (handler, requested) = mock_node_service();
        let (tx, cut_off, rx) = get_blocks_channel(GET_BLOCKS_PAGE_SIZE);

        stream_blocks(handler, Vec::new(), tx, cut_off, true, slot(), None, Arc::default()).await;

        assert!(requested.lock().unwrap().is_empty());
        assert!(rx.collect::<Vec<_>>().await.is_empty());
    }

    #[test]
    fn contiguous_runs_empty() {
        assert!(contiguous_runs(&[]).is_empty());
    }

    #[test]
    fn contiguous_runs_single_height() {
        assert_eq!(contiguous_runs(&[42]), vec![42..=42]);
    }

    #[test]
    fn contiguous_runs_fully_contiguous() {
        assert_eq!(contiguous_runs(&[10, 11, 12, 13]), vec![10..=13]);
    }

    #[test]
    fn contiguous_runs_fully_sparse() {
        assert_eq!(contiguous_runs(&[1, 3, 5]), vec![1..=1, 3..=3, 5..=5]);
        assert_eq!(contiguous_runs(&[100, 100_100]), vec![100..=100, 100_100..=100_100]);
    }

    #[test]
    fn contiguous_runs_mixed_runs() {
        assert_eq!(contiguous_runs(&[1, 2, 3, 7, 8, 20]), vec![1..=3, 7..=8, 20..=20]);
    }

    #[test]
    fn contiguous_runs_duplicates() {
        assert_eq!(contiguous_runs(&[5, 5, 5]), vec![5..=5]);
        assert_eq!(contiguous_runs(&[5, 5, 6, 6, 9, 9]), vec![5..=6, 9..=9]);
    }

    #[test]
    fn contiguous_runs_adjacent_vs_gap_boundary() {
        // A difference of one is the same run, a difference of two is not
        assert_eq!(contiguous_runs(&[10, 11]), vec![10..=11]);
        assert_eq!(contiguous_runs(&[10, 12]), vec![10..=10, 12..=12]);
    }

    #[test]
    fn contiguous_runs_handles_u64_max() {
        assert_eq!(contiguous_runs(&[u64::MAX]), vec![u64::MAX..=u64::MAX]);
        assert_eq!(contiguous_runs(&[u64::MAX - 1, u64::MAX]), vec![
            u64::MAX - 1..=u64::MAX
        ]);
        assert_eq!(contiguous_runs(&[0, u64::MAX, u64::MAX]), vec![
            0..=0,
            u64::MAX..=u64::MAX
        ]);
    }

    #[test]
    fn height_pages_is_unchanged_for_a_contiguous_request() {
        // Identical to paging `first..=last` directly
        let contiguous = (100u64..=125).collect::<Vec<_>>();
        assert_eq!(height_pages(&contiguous, 10).unwrap(), vec![
            (100, 109),
            (110, 119),
            (120, 125)
        ]);
    }

    #[test]
    fn height_pages_only_fetches_the_requested_heights() {
        // A wide, sparse request must not hydrate the span between the two heights
        let pages = height_pages(&[100, 100_100], GET_BLOCKS_PAGE_SIZE).unwrap();
        assert_eq!(pages, vec![(100, 100), (100_100, 100_100)]);
        let fetched: u64 = pages
            .iter()
            .map(|(start, end)| end.saturating_sub(*start).saturating_add(1))
            .sum();
        assert_eq!(fetched, 2);
    }

    #[test]
    fn height_pages_never_fetches_more_than_the_requested_heights() {
        let heights = [1u64, 2, 3, 50, 51, 999, 1_000_000];
        let fetched: u64 = height_pages(&heights, GET_BLOCKS_PAGE_SIZE)
            .unwrap()
            .iter()
            .map(|(start, end)| end.saturating_sub(*start).saturating_add(1))
            .sum();
        assert_eq!(fetched, heights.len() as u64);
    }

    #[test]
    fn height_pages_empty() {
        assert!(height_pages(&[], GET_BLOCKS_PAGE_SIZE).unwrap().is_empty());
    }

    #[test]
    fn height_pages_handles_u64_max() {
        assert_eq!(height_pages(&[u64::MAX], 10).unwrap(), vec![(u64::MAX, u64::MAX)]);
        assert_eq!(height_pages(&[u64::MAX - 2, u64::MAX - 1, u64::MAX], 2).unwrap(), vec![
            (u64::MAX - 2, u64::MAX - 1),
            (u64::MAX, u64::MAX)
        ]);
    }

    #[test]
    fn contiguous_runs_total_height_count_is_preserved() {
        let heights = [1u64, 2, 3, 9, 10, 77];
        let total: u64 = contiguous_runs(&heights)
            .iter()
            .map(|r| r.end().saturating_sub(*r.start()).saturating_add(1))
            .sum();
        assert_eq!(total, heights.len() as u64);
    }
}
