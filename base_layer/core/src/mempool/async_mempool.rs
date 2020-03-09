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
    blocks::Block,
    chain_storage::BlockchainBackend,
    mempool::{error::MempoolError, Mempool, StatsResponse, TxStorageResponse},
    transactions::{transaction::Transaction, types::Signature},
};
use std::sync::Arc;

macro_rules! make_async {
    ($fn:ident($($param1:ident:$ptype1:ty,$param2:ident:$ptype2:ty),+) -> $rtype:ty) => {
        pub async fn $fn<T>(mp: Mempool<T>, $($param1: $ptype1, $param2: $ptype2),+) -> Result<$rtype, MempoolError>
        where T: BlockchainBackend + 'static {
            tokio::task::spawn_blocking(move || mp.$fn($($param1,$param2),+))
                .await
                .or_else(|err| Err(MempoolError::BlockingTaskSpawnError(err.to_string())))
                .and_then(|inner_result| inner_result)
        }
    };

    ($fn:ident($($param:ident:$ptype:ty),+) -> $rtype:ty) => {
        pub async fn $fn<T>(mp: Mempool<T>, $($param: $ptype),+) -> Result<$rtype, MempoolError>
        where T: BlockchainBackend + 'static {
            tokio::task::spawn_blocking(move || mp.$fn($($param),+))
                .await
                .or_else(|err| Err(MempoolError::BlockingTaskSpawnError(err.to_string())))
                .and_then(|inner_result| inner_result)
        }
    };

    ($fn:ident() -> $rtype:ty) => {
        pub async fn $fn<T>(mp: Mempool<T>) -> Result<$rtype, MempoolError>
        where T: BlockchainBackend + 'static {
            tokio::task::spawn_blocking(move || {
                mp.$fn()
            })
            .await
            .or_else(|err| Err(MempoolError::BlockingTaskSpawnError(err.to_string())))
            .and_then(|inner_result| inner_result)
        }
    };
}

make_async!(insert(tx: Arc<Transaction>) -> ());
make_async!(process_published_block(published_block: Block) -> ());
make_async!(process_reorg(removed_blocks: Vec<Block>, new_blocks: Vec<Block>) -> ());
make_async!(snapshot() -> Vec<Arc<Transaction>>);
make_async!(retrieve(total_weight: u64) -> Vec<Arc<Transaction>>);
make_async!(has_tx_with_excess_sig(excess_sig: Signature) -> TxStorageResponse);
make_async!(stats() -> StatsResponse);
