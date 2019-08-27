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

use crate::{
    blocks::{block::Block, blockheader::BlockHeader},
    chain_storage::chain_state_traits::BlockChainState,
    transaction::{TransactionKernel, TransactionOutput},
};
use merklemountainrange::merklenode::MerkleNode;
use std::sync::Arc;

/// A state machine that manages a synchronisation request from a client.
/// First there's the initial sync from the horizon point. This has multiple steps
/// - UTXOs at horizon,
/// - all kernels to horizon,
/// - All headers to horizon
/// Then there's the sequential block sync. Essentially a series of "NewBlock" commands
struct SyncStateMachine<B> {
    blockchain_state: Arc<B>,
}

struct Stream<T>(T); // Point to Tokio streams

impl<B> SyncStateMachine<B>
where B: BlockChainState
{
    fn sync_state_at_height(&mut self, block_height: u64, last_state: SyncStateRequest) -> Stream<SyncStateEvent> {
        unimplemented!()
    }

    fn sync_blocks(&mut self, start_block: u64, end_block: u64) -> Stream<BlockSyncEvent> {
        unimplemented!()
    }
}

pub struct BlockSyncEvent {
    start_height: u64,
    end_height: u64,
    this_height: u64,
    block: Block,
}

pub struct UTXOSyncEvent {
    index: u64,
    total: u64,
    utxo: TransactionOutput,
}

pub struct MMREvent {
    mmr: Vec<MerkleNode>,
}

pub struct HeaderSyncEvent {
    index: u64,
    total: u64,
    header: BlockHeader,
}

pub struct KernelSyncEvent {
    index: u64,
    total: u64,
    kernel: TransactionKernel,
}

pub enum SyncStateEvent {
    UTXO(Box<UTXOSyncEvent>),
    UtxoMMR(Box<Vec<MerkleNode>>),
    Header(Box<HeaderSyncEvent>),
    HeaderMMR(Box<Vec<MerkleNode>>),
    Kernel(Box<KernelSyncEvent>),
    KernelMMR(Box<Vec<MerkleNode>>),
    Done,
}

pub enum SyncStateRequest {
    None,
    UTXO(u64),
    UtxoMMR,
    Header(u64),
    HeaderMMR,
    Kernel(u64),
    KernelMMR,
    Done,
}
