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

pub mod support;

use rand::{
    distributions::{Distribution, Uniform},
    Rng,
};
use support::{create_mmr, create_mutable_mmr, int_to_hash};
use tari_mmr::{
    functions::{calculate_mmr_root, calculate_pruned_mmr_root, prune_mmr},
    Hash,
};

#[test]
fn pruned_mmr_empty() {
    let mmr = create_mmr(0);
    let root = mmr.get_merkle_root();
    let pruned = prune_mmr(&mmr).expect("Could not create empty pruned MMR");
    assert_eq!(pruned.is_empty(), Ok(true));
    assert_eq!(pruned.get_merkle_root(), root);
}

#[test]
fn pruned_mmrs() {
    for size in &[6, 14, 63, 64, 65, 127] {
        let mmr = create_mmr(*size);
        let mmr2 = create_mmr(size + 2);

        let root = mmr.get_merkle_root();
        let mut pruned = prune_mmr(&mmr).expect("Could not create empty pruned MMR");
        assert_eq!(pruned.len(), mmr.len());
        assert_eq!(pruned.get_merkle_root(), root);
        // The pruned MMR works just like the normal one
        let new_hash = int_to_hash(*size);
        assert!(pruned.push(&new_hash).is_ok());
        assert!(pruned.push(&int_to_hash(*size + 1)).is_ok());
        assert_eq!(pruned.get_merkle_root(), mmr2.get_merkle_root());
        // But you can only get recent hashes
        assert_eq!(pruned.get_leaf_hash(*size / 2), Ok(None));
        assert_eq!(pruned.get_leaf_hash(*size), Ok(Some(new_hash)))
    }
}

fn get_changes() -> (usize, Vec<Hash>, Vec<u32>) {
    let mut rng = rand::thread_rng();
    let src_size: usize = rng.gen_range(25, 150);
    let addition_length = rng.gen_range(1, 100);
    let additions: Vec<Hash> = Uniform::from(1..1000)
        .sample_iter(rng)
        .take(addition_length)
        .map(int_to_hash)
        .collect();
    let deletions: Vec<u32> = Uniform::from(0..src_size)
        .sample_iter(rng)
        .take(src_size / 5)
        .map(|v| v as u32)
        .collect();
    (src_size, additions, deletions)
}

/// Create a random-sized MMR. Add a random number of additions and deletions; and check the new root against the
/// result of `calculate_pruned_mmr_root`
#[test]
pub fn calculate_pruned_mmr_roots() {
    let (src_size, additions, deletions) = get_changes();
    let mut src = create_mutable_mmr(src_size);
    let src_root = src.get_merkle_root().expect("Did not get source root");
    let root =
        calculate_pruned_mmr_root(&src, additions.clone(), deletions.clone()).expect("Did not calculate new root");
    assert_ne!(src_root, root);
    // Double check
    additions.iter().for_each(|h| {
        src.push(h).unwrap();
    });
    deletions.iter().for_each(|i| {
        src.delete(*i);
    });
    let new_root = src.get_merkle_root().expect("Did not calculate new root");
    assert_eq!(root, new_root);
}

/// Create a random-sized MMR. Add a random number of additions; and check the new root against the
/// result of `calculate_mmr_root`
#[test]
pub fn calculate_mmr_roots() {
    let (src_size, additions, _) = get_changes();
    let mut src = create_mmr(src_size);
    let src_root = src.get_merkle_root().expect("Did not get source root");
    let root = calculate_mmr_root(&src, additions.clone()).expect("Did not calculate new root");
    assert_ne!(src_root, root);
    // Double check
    additions.iter().for_each(|h| {
        src.push(h).unwrap();
    });
    let new_root = src.get_merkle_root().expect("Did not calculate new root");
    assert_eq!(root, new_root);
}
