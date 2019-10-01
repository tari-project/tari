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
    chain_storage::{async_db, BlockchainDatabase, MemoryDatabase, MmrTree},
    test_utils::{
        builders::create_test_block,
        sample_blockchains::{create_blockchain_db_no_cut_through, generate_new_block},
    },
    transaction::{TransactionOutput, UnblindedOutput},
    types::{HashDigest, COMMITMENT_FACTORY},
};
use bitflags::_core::sync::atomic::{AtomicBool, Ordering};
use std::{fs::File, io::Write, sync::Arc};
use tari_crypto::commitment::HomomorphicCommitmentFactory;
use tari_utilities::{hex::Hex, Hashable};
use tokio::{self, runtime::Runtime};

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

fn test_async<F>(f: F)
where F: FnOnce(&Runtime) {
    let passed = Arc::new(AtomicBool::new(true));
    let rt = create_runtime(passed.clone());
    f(&rt);
    rt.shutdown_on_idle();
    assert!(passed.load(Ordering::SeqCst));
}

fn write_logs(db: &BlockchainDatabase<MemoryDatabase<HashDigest>>, blocks: &[Block]) -> Result<(), std::io::Error> {
    {
        let mut block_output = File::create("block_output.txt")?;
        for block in blocks.iter() {
            block_output.write_all(format!("{}\n", block).as_bytes())?;
        }
    }
    {
        let mut db_output = File::create("db_output.txt")?;
        db_output.write_all("---------  Metadata -------------\n".as_bytes())?;
        let metadata = db.get_metadata().unwrap();
        db_output.write_all(format!("{}", metadata).as_bytes())?;
        db_output.write_all("\n---------  Database -------------\n".as_bytes())?;
        let s = format!("{:?}", db.db());
        db_output.write_all(s.as_bytes())?;
    }
    Ok(())
}

fn dump_logs(db: &BlockchainDatabase<MemoryDatabase<HashDigest>>, blocks: &[Block]) -> String {
    match write_logs(db, blocks) {
        Err(e) => e.to_string(),
        Ok(()) => "Logs written".into(),
    }
}

/// Finds the UTXO in a block corresponding to the unblinded output. We have to search for outputs because UTXOs get
/// sorted in blocks, and so the order they were inserted in can change.
fn find_utxo(output: &UnblindedOutput, block: &Block) -> Option<TransactionOutput> {
    for utxo in block.body.outputs.iter() {
        if COMMITMENT_FACTORY.open_value(&output.spending_key, output.value.into(), &utxo.commitment) {
            return Some(utxo.clone());
        }
    }
    return None;
}

#[test]
fn fetch_async_kernel() {
    let (db, blocks, _) = create_blockchain_db_no_cut_through();
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
    let (db, blocks, _) = create_blockchain_db_no_cut_through();
    test_async(move |rt| {
        for block in blocks.into_iter() {
            let height = block.header.height;
            let hash = block.hash();
            let db = db.clone();
            rt.spawn(async move {
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
    let (db, blocks, outputs) = create_blockchain_db_no_cut_through();
    // Retrieve a UTXO and an STXO
    let utxo = find_utxo(&outputs[4][0], &blocks[4]).unwrap();
    let stxo = find_utxo(&outputs[1][0], &blocks[1]).unwrap();
    test_async(move |rt| {
        let db = db.clone();
        let db2 = db.clone();
        let blocks2 = blocks.clone();
        rt.spawn(async move {
            let utxo_check = async_db::fetch_utxo(db.clone(), utxo.hash()).await;
            assert_eq!(utxo_check, Ok(utxo), "{}", dump_logs(&db, &blocks));
        });
        rt.spawn(async move {
            let stxo_check = async_db::fetch_stxo(db2.clone(), stxo.hash()).await;
            assert_eq!(stxo_check, Ok(stxo), "{}", dump_logs(&db2, &blocks2));
        });
    });
}

#[test]
fn async_is_utxo() {
    let (db, blocks, outputs) = create_blockchain_db_no_cut_through();
    blocks.iter().for_each(|b| println!("{}", b));
    // Retrieve a UTXO and an STXO
    let utxo = find_utxo(&outputs[4][0], &blocks[4]).unwrap();
    let stxo = find_utxo(&outputs[1][0], &blocks[1]).unwrap();
    // Check using sync functions
    assert_eq!(db.is_utxo(utxo.hash()), Ok(true), "{}", dump_logs(&db, &blocks));
    assert_eq!(db.is_utxo(stxo.hash()), Ok(false), "{}", dump_logs(&db, &blocks));
    test_async(move |rt| {
        let db = db.clone();
        let db2 = db.clone();
        let blocks2 = blocks.clone();
        rt.spawn(async move {
            let is_utxo = async_db::is_utxo(db.clone(), utxo.hash()).await;
            assert_eq!(is_utxo, Ok(true), "{}", dump_logs(&db, &blocks));
        });
        rt.spawn(async move {
            let is_utxo = async_db::is_utxo(db2.clone(), stxo.hash()).await;
            assert_eq!(is_utxo, Ok(false), "{}", dump_logs(&db2, &blocks2));
        });
    });
}

#[test]
fn fetch_async_block() {
    let (db, blocks, _) = create_blockchain_db_no_cut_through();
    test_async(move |rt| {
        for block in blocks.into_iter() {
            let height = block.header.height;
            let db = db.clone();
            rt.spawn(async move {
                let block_check = async_db::fetch_block(db.clone(), height).await.unwrap();
                assert_eq!(&block, block_check.block());
            });
        }
    });
}

#[test]
fn fetch_async_mmr_roots() {
    let (db, blocks, _) = create_blockchain_db_no_cut_through();
    let metadata = db.get_metadata().unwrap();
    test_async(move |rt| {
        let dbc = db.clone();
        rt.spawn(async move {
            let root = futures::join!(
                async_db::fetch_mmr_root(dbc.clone(), MmrTree::Utxo),
                async_db::fetch_mmr_root(dbc.clone(), MmrTree::Kernel),
            );
            let block_height = metadata.height_of_longest_chain.unwrap();
            let header = async_db::fetch_header(dbc.clone(), block_height).await.unwrap();
            let utxo_mmr = root.0.unwrap().to_hex();
            let kernel_mmr = root.1.unwrap().to_hex();
            assert_eq!(utxo_mmr, header.output_mr.to_hex(), "{}", dump_logs(&dbc, &blocks));
            assert_eq!(kernel_mmr, header.kernel_mr.to_hex(), "Kernel MMR roots don't match");
        });
    });
}

#[test]
fn async_add_block_fetch_orphan() {
    let (db, blocks, _) = create_blockchain_db_no_cut_through();
    let orphan = create_test_block(7, None, vec![]);
    let block_hash = orphan.hash();
    test_async(move |rt| {
        let dbc = db.clone();
        rt.spawn(async move {
            async_db::add_block(dbc.clone(), orphan.clone()).await.unwrap();
            let block = async_db::fetch_orphan(dbc.clone(), block_hash).await.unwrap();
            assert_eq!(orphan, block);
        });
    });
}
