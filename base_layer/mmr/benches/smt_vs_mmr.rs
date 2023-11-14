// Copyright 2023. The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::convert::TryFrom;

use blake2::Blake2b;
use digest::consts::U32;
use tari_mmr::sparse_merkle_tree::{NodeKey, SparseMerkleTree, ValueHash};

fn random_key() -> NodeKey {
    let key = rand::random::<[u8; 32]>();
    NodeKey::from(key)
}

fn get_keys(n: usize) -> Vec<NodeKey> {
    (0..n).map(|_| random_key()).collect()
}

fn create_smt() -> SparseMerkleTree<Blake2b<U32>> {
    SparseMerkleTree::<Blake2b<U32>>::new()
}

fn insert_into_smt(keys: &[NodeKey], tree: &mut SparseMerkleTree<Blake2b<U32>>) {
    keys.iter().for_each(|key| {
        tree.upsert(key.clone(), ValueHash::default()).unwrap();
    });
}

fn delete_from_smt(keys: &[NodeKey], tree: &mut SparseMerkleTree<Blake2b<U32>>) {
    keys.iter().for_each(|key| {
        tree.delete(key).unwrap();
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
}
