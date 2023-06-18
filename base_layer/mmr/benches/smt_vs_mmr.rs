// Copyright 2023. The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

#[cfg(feature = "native_bitmap")]
use croaring::Bitmap;
use tari_crypto::hash::blake2::Blake256;
#[cfg(feature = "native_bitmap")]
use tari_crypto::hash_domain;
#[cfg(feature = "native_bitmap")]
use tari_crypto::hashing::DomainSeparatedHasher;
#[cfg(feature = "native_bitmap")]
use tari_crypto::tari_utilities::hex::Hex;
use tari_mmr::sparse_merkle_tree::{NodeKey, SparseMerkleTree, ValueHash};
#[cfg(feature = "native_bitmap")]
use tari_mmr::{Hash, MutableMmr};

#[cfg(feature = "native_bitmap")]
hash_domain!(
    MmrBenchTestHashDomain,
    "com.tari.tari_project.base_layer.mmr.benches",
    1
);
#[cfg(feature = "native_bitmap")]
pub type MmrTestHasherBlake256 = DomainSeparatedHasher<Blake256, MmrBenchTestHashDomain>;
#[cfg(feature = "native_bitmap")]
pub type TestMmr = MutableMmr<MmrTestHasherBlake256, Vec<Hash>>;

fn random_key() -> NodeKey {
    let key = rand::random::<[u8; 32]>();
    NodeKey::from(key)
}

fn get_keys(n: usize) -> Vec<NodeKey> {
    (0..n).map(|_| random_key()).collect()
}

fn create_smt() -> SparseMerkleTree<Blake256> {
    SparseMerkleTree::<Blake256>::new()
}

fn insert_into_smt(keys: &[NodeKey], tree: &mut SparseMerkleTree<Blake256>) {
    keys.iter().for_each(|key| {
        tree.upsert(key.clone(), ValueHash::default()).unwrap();
    });
}

fn delete_from_smt(keys: &[NodeKey], tree: &mut SparseMerkleTree<Blake256>) {
    keys.iter().for_each(|key| {
        tree.delete(key).unwrap();
    });
}

#[cfg(feature = "native_bitmap")]
fn insert_into_mmr(keys: &[Vec<u8>], mmr: &mut TestMmr) {
    keys.iter().for_each(|key| {
        mmr.push(key.clone()).unwrap();
    });
}

#[cfg(feature = "native_bitmap")]
fn delete_from_mmr(start: u32, n: u32, mmr: &mut TestMmr) {
    (start..start + n).for_each(|i| {
        mmr.delete(i);
    });
}

fn time_function(header: &str, f: impl FnOnce()) -> std::time::Duration {
    println!("Starting: {header}");
    let now = std::time::Instant::now();
    f();
    let t = now.elapsed();
    println!("Finished: {header} - {t:?}");
    t
}

fn main() {
    let size = 1_000_000;
    let half_size = size / 2;
    let keys = get_keys(size);
    let mut tree = create_smt();
    time_function(&format!("SMT: Inserting {size} keys"), || {
        insert_into_smt(&keys, &mut tree);
    });
    time_function("SMT: Calculating root hash", || {
        let size = tree.size();
        let hash = tree.hash();
        println!("Tree size: {size}. Root hash: {hash:x}");
    });
    time_function(&format!("SMT: Deleting {half_size} keys"), || {
        delete_from_smt(&keys[0..half_size], &mut tree);
    });
    time_function("SMT: Calculating root hash", || {
        let size = tree.size();
        let hash = tree.hash();
        println!("Tree size: {size}. Root hash: {hash:x}");
    });
    time_function(&format!("SMT: Deleting another {half_size} keys"), || {
        delete_from_smt(&keys[half_size..], &mut tree);
    });
    time_function("SMT: Calculating root hash", || {
        let size = tree.size();
        let hash = tree.hash();
        println!("Tree size: {size}. Root hash: {hash:x}");
    });
    #[cfg(feature = "native_bitmap")]
    {
        let mut mmr = TestMmr::new(Vec::default(), Bitmap::default()).unwrap();
        let keys = keys.into_iter().map(|k| k.as_slice().to_vec()).collect::<Vec<_>>();
        time_function(&format!("MMR: Inserting {size} keys"), || {
            insert_into_mmr(&keys, &mut mmr);
        });
        time_function("MMR: Calculating root hash", || {
            let size = mmr.len();
            let hash = mmr.get_merkle_root().unwrap();
            println!("Tree size: {size}. Root hash: {}", hash.to_hex());
        });
        time_function(&format!("MMR: Deleting {half_size} keys"), || {
            delete_from_mmr(0, half_size as u32, &mut mmr);
        });
        time_function("MMR: Calculating root hash", || {
            let size = mmr.len();
            let hash = mmr.get_merkle_root().unwrap();
            println!("Tree size: {size}. Root hash: {}", hash.to_hex());
        });
        time_function(&format!("MMR: Deleting another {half_size} keys"), || {
            delete_from_mmr(half_size as u32, half_size as u32, &mut mmr);
        });
        time_function("MMR: Calculating root hash", || {
            let size = mmr.len();
            let hash = mmr.get_merkle_root().unwrap();
            println!("Tree size: {size}. Root hash: {}", hash.to_hex());
        });
    }
}
