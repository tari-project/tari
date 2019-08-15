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

mod support;

use support::{create_mmr, int_to_hash};
use tari_mmr::pruned_mmr::prune_mmr;

#[test]
fn pruned_mmr_empty() {
    let mmr = create_mmr(0);
    let root = mmr.get_merkle_root();
    let pruned = prune_mmr(&mmr).expect("Could not create empty pruned MMR");
    assert!(pruned.is_empty());
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
        assert!(pruned.get_leaf_hash(*size / 2).is_none());
        assert_eq!(pruned.get_leaf_hash(*size).unwrap(), &new_hash)
    }
}
