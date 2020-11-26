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

use super::error::BlockchainStateServiceError;
use crate::{
    blocks::{Block, BlockHeader},
    transactions::types::HashOutput,
};
use futures::{
    channel::{mpsc, oneshot},
    SinkExt,
};
use std::ops::{Bound, RangeBounds};
use tari_common_types::chain_metadata::ChainMetadata;

type ReplySender<T> = oneshot::Sender<Result<T, BlockchainStateServiceError>>;

#[derive(Debug)]
pub enum BlockchainStateRequest {
    GetBlocks((Bound<u64>, Bound<u64>), ReplySender<Vec<Block>>),
    GetHeaders((Bound<u64>, Bound<u64>), ReplySender<Vec<BlockHeader>>),
    GetHeaderByHeight(u64, ReplySender<Option<BlockHeader>>),
    GetHeaderByHash(HashOutput, ReplySender<Option<BlockHeader>>),
    GetChainMetadata(ReplySender<ChainMetadata>),
    FindHeadersAfterHash((Vec<HashOutput>, u64), ReplySender<Option<(usize, Vec<BlockHeader>)>>),
}

#[derive(Clone)]
pub struct BlockchainStateServiceHandle {
    sender: mpsc::Sender<BlockchainStateRequest>,
}

impl BlockchainStateServiceHandle {
    pub(super) fn new(sender: mpsc::Sender<BlockchainStateRequest>) -> Self {
        Self { sender }
    }

    /// Get blocks within the given height `Bound`s
    pub async fn get_blocks<R: RangeBounds<u64>>(
        &mut self,
        range: R,
    ) -> Result<Vec<Block>, BlockchainStateServiceError>
    {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(BlockchainStateRequest::GetBlocks(get_bounds(range), reply_tx))
            .await?;
        reply_rx.await?
    }

    /// Get headers within the given height `Bound`s
    pub async fn get_headers<R: RangeBounds<u64>>(
        &mut self,
        range: R,
    ) -> Result<Vec<BlockHeader>, BlockchainStateServiceError>
    {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(BlockchainStateRequest::GetHeaders(get_bounds(range), reply_tx))
            .await?;
        reply_rx.await?
    }

    /// Get the current chain metdata
    pub async fn get_chain_metadata(&mut self) -> Result<ChainMetadata, BlockchainStateServiceError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(BlockchainStateRequest::GetChainMetadata(reply_tx))
            .await?;
        reply_rx.await?
    }

    /// Get a header by height. If the header does not exist, None is returned.
    pub async fn get_header_by_height(
        &mut self,
        height: u64,
    ) -> Result<Option<BlockHeader>, BlockchainStateServiceError>
    {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(BlockchainStateRequest::GetHeaderByHeight(height, reply_tx))
            .await?;
        reply_rx.await?
    }

    /// Get a header by block hash. If the header does not exist, None is returned.
    pub async fn get_header_by_hash(
        &mut self,
        hash: HashOutput,
    ) -> Result<Option<BlockHeader>, BlockchainStateServiceError>
    {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(BlockchainStateRequest::GetHeaderByHash(hash, reply_tx))
            .await?;
        reply_rx.await?
    }

    /// Find the first matching header in a list of block hashes, returning the index of the match and the BlockHeader.
    /// Or None if not found.
    pub async fn find_headers_after_hash<I: IntoIterator<Item = HashOutput>>(
        &mut self,
        hashes: I,
        count: u64,
    ) -> Result<Option<(usize, Vec<BlockHeader>)>, BlockchainStateServiceError>
    {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(BlockchainStateRequest::FindHeadersAfterHash(
                (hashes.into_iter().collect(), count),
                reply_tx,
            ))
            .await?;
        reply_rx.await?
    }
}

fn get_bounds<R: RangeBounds<u64>>(range: R) -> (Bound<u64>, Bound<u64>) {
    // `Bound::cloned(self)` has not stabilized
    fn bound_cloned(bound: Bound<&u64>) -> Bound<u64> {
        match bound {
            Bound::Unbounded => Bound::Unbounded,
            Bound::Included(x) => Bound::Included(*x),
            Bound::Excluded(x) => Bound::Excluded(*x),
        }
    }
    (bound_cloned(range.start_bound()), bound_cloned(range.end_bound()))
}
