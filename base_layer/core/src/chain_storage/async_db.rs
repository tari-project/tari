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
    chain_storage::{BlockchainBackend, BlockchainDatabase, ChainStorageError, MmrTree,
                    blockchain_database::BlockAddResult, HistoricalBlock},
    blocks::{ Block, BlockHeader },
    transaction::{ TransactionOutput, TransactionKernel },
    types::HashOutput,
};
use futures::future::poll_fn;
use std::task::Poll;
use tokio_executor::threadpool::blocking;

macro_rules! make_async {
    ($fn:ident($param:ident:$ptype:ty) -> $rtype:ty) => {
        pub async fn $fn<T>(db: BlockchainDatabase<T>, $param: $ptype) -> Result<$rtype, ChainStorageError>
    where T: BlockchainBackend,
    {
        poll_fn(move |_| {
            let db = db.clone();
            let hash = $param.clone();
            match blocking(move || db.$fn(hash)) {
                Poll::Pending => Poll::Pending,
                // Map BlockingError -> ChainStorageError
                Poll::Ready(Err(e)) => Poll::Ready(Err(ChainStorageError::AccessError(format!(
                    "Could not find a blocking thread to execute DB query. {}",
                    e.to_string()
                )))),
                // Unwrap and lift ChainStorageError
                Poll::Ready(Ok(Err(e))) => Poll::Ready(Err(e)),
                // Unwrap and return result
                Poll::Ready(Ok(Ok(v))) => Poll::Ready(Ok(v)),
            }
        })
        .await
}
    }
}
make_async!(fetch_kernel(hash: HashOutput) -> TransactionKernel);
make_async!(fetch_header_with_block_hash(hash: HashOutput) -> BlockHeader);
make_async!(fetch_header(block_num: u64) -> BlockHeader);
make_async!(fetch_utxo(hash: HashOutput) -> TransactionOutput);
make_async!(fetch_stxo(hash: HashOutput) -> TransactionOutput);
make_async!(fetch_orphan(hash: HashOutput) -> Block);
make_async!(is_utxo(hash: HashOutput) -> bool);
make_async!(fetch_mmr_root(tree: MmrTree) -> HashOutput);
make_async!(fetch_mmr_only_root(tree: MmrTree) -> HashOutput);
make_async!(add_block(block: Block) -> BlockAddResult);
//make_async!(is_new_best_block(block: &Block) -> bool);
make_async!(fetch_block(height: u64) -> HistoricalBlock);
make_async!(rewind_to_height(height: u64) -> ());
//make_async!(fetch_mmr_proof(tree: MmrTree, pos: usize) -> MerkleProof); // TODO support multiple params
