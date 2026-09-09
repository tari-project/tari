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

use std::{cmp, ops::RangeInclusive};

use futures::{SinkExt, channel::mpsc};
use log::*;
use minotari_app_grpc::tari_rpc;
use tari_core::{base_node::LocalNodeCommsInterface, iterators::NonOverlappingIntegerPairIter};
use tari_node_components::blocks::HistoricalBlock;
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

/// Fetches each page of blocks in turn and streams them to the client.
///
/// Returns as soon as the receiver is gone, i.e. the client has cancelled the request or disconnected. Without that
/// early return the task would keep hydrating every remaining page from the database only to discard it, which is
/// what allowed a client that repeatedly timed out to accumulate work on the node.
pub async fn stream_blocks(
    mut handler: LocalNodeCommsInterface,
    pages: Vec<(u64, u64)>,
    mut tx: mpsc::Sender<Result<tari_rpc::HistoricalBlock, Status>>,
    report_error_flag: bool,
) {
    for (start, end) in pages {
        let blocks = match handler.get_blocks(start..=end, false).await {
            Err(err) => {
                warn!(
                    target: LOG_TARGET,
                    "Error communicating with local base node: {err:?}"
                );
                return;
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
            if tx.send(result).await.is_err() {
                warn!(
                    target: LOG_TARGET,
                    "[get_blocks] Request was cancelled while sending a response"
                );
                return;
            }
        }
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
        let (tx, rx) = mpsc::channel(GET_BLOCKS_PAGE_SIZE);

        stream_blocks(handler, pages, tx, true).await;

        assert_eq!(*requested.lock().unwrap(), vec![500..=500, 100_500..=100_500]);
        let streamed = rx.collect::<Vec<_>>().await;
        assert_eq!(streamed.len(), 2);
    }

    #[tokio::test]
    async fn stream_blocks_streams_every_page_of_a_contiguous_request() {
        let (handler, requested) = mock_node_service();
        let heights = (10u64..=34).collect::<Vec<_>>();
        let pages = height_pages(&heights, GET_BLOCKS_PAGE_SIZE).unwrap();
        let (tx, rx) = mpsc::channel(GET_BLOCKS_PAGE_SIZE);

        task::spawn(stream_blocks(handler, pages, tx, true));

        let streamed = rx.collect::<Vec<_>>().await;
        assert_eq!(streamed.len(), heights.len());
        assert_eq!(*requested.lock().unwrap(), vec![10..=19, 20..=29, 30..=34]);
    }

    #[tokio::test]
    async fn stream_blocks_stops_fetching_once_the_client_is_gone() {
        // Simulate a client that cancelled the request before any response could be sent: the very first send fails,
        // so only the first page may ever be fetched no matter how many pages were queued.
        let (handler, requested) = mock_node_service();
        let heights = (1u64..=100).collect::<Vec<_>>();
        let pages = height_pages(&heights, GET_BLOCKS_PAGE_SIZE).unwrap();
        assert_eq!(pages.len(), 10);
        let (tx, rx) = mpsc::channel(GET_BLOCKS_PAGE_SIZE);
        drop(rx);

        stream_blocks(handler, pages, tx, true).await;

        assert_eq!(*requested.lock().unwrap(), vec![1..=10]);
    }

    #[tokio::test]
    async fn stream_blocks_stops_fetching_when_the_client_disconnects_mid_stream() {
        // The client reads the first page and then goes away. No page after the one in flight may be fetched.
        let (handler, requested) = mock_node_service();
        let heights = (1u64..=100).collect::<Vec<_>>();
        let pages = height_pages(&heights, GET_BLOCKS_PAGE_SIZE).unwrap();
        let (tx, mut rx) = mpsc::channel(GET_BLOCKS_PAGE_SIZE);

        let streaming = task::spawn(stream_blocks(handler, pages, tx, true));
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
        let (tx, rx) = mpsc::channel(GET_BLOCKS_PAGE_SIZE);

        stream_blocks(handler, Vec::new(), tx, true).await;

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
