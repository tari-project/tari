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
    chain_storage::{async_db, BlockchainDatabase, MemoryDatabase},
    test_utils::sample_blockchains::create_blockchain_db_no_cut_through,
    transaction::TransactionKernel,
    types::HashDigest,
};
use tari_utilities::Hashable;
use tokio;
use bitflags::_core::sync::atomic::{ Ordering, AtomicBool};
use std::sync::Arc;

fn create_runtime(passed: Arc<AtomicBool>) -> tokio::runtime::Runtime {
    let rt = tokio::runtime::Builder::new()
        .blocking_threads(4)
        // Run the work scheduler on one thread so we can really see the effects of using `blocking` above
        .core_threads(1)
        .panic_handler(move |_| { passed.store(false, Ordering::Relaxed) })
        .build()
        .expect("Could not create runtime");
    rt
}

async fn test_kernel(db: BlockchainDatabase<MemoryDatabase<HashDigest>>, kernel: TransactionKernel) {
    let kern_db = async_db::fetch_kernel(db, kernel.hash()).await;
    assert_eq!(kernel, kern_db.unwrap());
}

#[test]
fn fetch_async_kernel() {
    let passed = Arc::new(AtomicBool::new(true));
    let rt = create_runtime(passed.clone());
    let (db, blocks) = create_blockchain_db_no_cut_through();
    // Fetch all the kernels in parallel
    for block in blocks.iter() {
        block.body.kernels.iter().for_each(|k| {
            rt.spawn(test_kernel(db.clone(), k.clone()));
        })
    }
    rt.shutdown_on_idle();
    assert!(passed.load(Ordering::SeqCst), "Loading kernels failed");
}
