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
    chain_storage::{async_db},
    test_utils::sample_blockchains::create_blockchain_db_no_cut_through,
    transaction::TransactionKernel,
    types::HashDigest,
};
use bitflags::_core::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tari_utilities::{Hashable};
use tokio;
use tokio::runtime::Runtime;

fn create_runtime(passed: Arc<AtomicBool>) -> Runtime {
    let rt = tokio::runtime::Builder::new()
        .blocking_threads(4)
        // Run the work scheduler on one thread so we can really see the effects of using `blocking` above
        .core_threads(1)
        .panic_handler(move |_| { passed.store(false, Ordering::SeqCst) })
        .build()
        .expect("Could not create runtime");
    rt
}

fn test_async<F>(f: F) where F: FnOnce(&Runtime) {
    let passed = Arc::new(AtomicBool::new(true));
    let rt = create_runtime(passed.clone());
    f(&rt);
    rt.shutdown_on_idle();
    assert!(passed.load(Ordering::SeqCst));
}

#[test]
fn fetch_async_kernel() {
    let (db, blocks) = create_blockchain_db_no_cut_through();
    test_async(|rt| {
        for block in blocks.into_iter() {
            block.body.kernels.into_iter().for_each(|k| {
                let db = db.clone();
                rt.spawn(async move {
                    let kern_db = async_db::fetch_kernel(db, k.hash()).await;
                    assert_eq!(k, kern_db.unwrap());
                });
            })
        }
    });
}

#[test]
fn fetch_async_headers() {
    let (db, blocks) = create_blockchain_db_no_cut_through();
    test_async(move |rt| {
        for block in blocks.into_iter() {
            let height = block.header.height;
            let hash = block.hash();
            let db = db.clone();
            rt.spawn( async move {
                let header_height = async_db::fetch_header(db.clone(), height).await.unwrap();
                let header_hash = async_db::fetch_header_with_block_hash(db.clone(), hash).await.unwrap();
                assert_eq!(block.header, header_height);
                assert_eq!(block.header, header_hash);
            });
        }
    });
}

#[test]
fn fetch_async_utxo() {
    let (db, blocks) = create_blockchain_db_no_cut_through();
    // Retrieve a UTXO and an STXO
    let utxo = blocks[4].body.outputs[0].clone();
    let stxo = blocks[1].body.outputs[0].clone();
    test_async(move |rt| {
        let db = db.clone();
        let db2 = db.clone();
        rt.spawn( async move {
            let utxo_check = async_db::fetch_utxo(db, utxo.hash()).await;
            assert_eq!(utxo, utxo_check.unwrap());
        });
        rt.spawn( async move {
            let stxo_check = async_db::fetch_stxo(db2, stxo.hash()).await;
            assert_eq!(stxo, stxo_check.unwrap());
        });
    });
}

#[test]
fn async_is_utxo() {
    let (db, blocks) = create_blockchain_db_no_cut_through();
    // Retrieve a UTXO and an STXO
    let utxo = blocks[4].body.outputs[0].clone();
    let stxo = blocks[1].body.outputs[0].clone();
    test_async(move |rt| {
        let db = db.clone();
        let db2 = db.clone();
        rt.spawn( async move {
            let is_utxo = async_db::is_utxo(db, utxo.hash()).await;
            assert_eq!(is_utxo.unwrap(), true);
        });
        rt.spawn( async move {
            let is_utxo = async_db::is_utxo(db2, stxo.hash()).await;
            assert_eq!(is_utxo.unwrap(), false);
        });
    });
}