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

#[allow(dead_code)]
mod support;

use croaring::Bitmap;
use support::{combine_hashes, int_to_hash, Hasher};
use tari_mmr::{ArrayLike, ArrayLikeExt, MemBackendVec, MerkleCheckPoint, MmrCache, MmrCacheConfig};

#[test]
fn create_cache_update_and_rewind() {
    let config = MmrCacheConfig { rewind_hist_len: 2 };
    let mut checkpoint_db = MemBackendVec::<MerkleCheckPoint>::new();
    let mut mmr_cache = MmrCache::<Hasher, _, _>::new(Vec::new(), checkpoint_db.clone(), config).unwrap();

    let h1 = int_to_hash(1);
    let h2 = int_to_hash(2);
    let h3 = int_to_hash(3);
    let h4 = int_to_hash(4);
    let h5 = int_to_hash(5);
    let h6 = int_to_hash(6);
    let h7 = int_to_hash(7);
    let h8 = int_to_hash(8);
    let ha = combine_hashes(&[&h1, &h2]);
    let hb = combine_hashes(&[&h3, &h4]);
    let hc = combine_hashes(&[&h5, &h6]);
    let hd = combine_hashes(&[&h7, &h8]);
    let hahb = combine_hashes(&[&ha, &hb]);
    let hchd = combine_hashes(&[&hc, &hd]);
    let cp1_mmr_only_root = combine_hashes(&[&ha]);
    let cp2_mmr_only_root = combine_hashes(&[&hahb]);
    let cp3_mmr_only_root = combine_hashes(&[&hahb, &hc]);
    let cp4_mmr_only_root = combine_hashes(&[&combine_hashes(&[&hahb, &hchd])]);

    checkpoint_db
        .push(MerkleCheckPoint::new(vec![h1.clone(), h2.clone()], Bitmap::create(), 0))
        .unwrap();
    assert!(mmr_cache.update().is_ok());
    assert_eq!(mmr_cache.get_mmr_only_root(), Ok(cp1_mmr_only_root.clone()));

    checkpoint_db
        .push(MerkleCheckPoint::new(vec![h3.clone(), h4.clone()], Bitmap::create(), 0))
        .unwrap();
    assert!(mmr_cache.update().is_ok());
    assert_eq!(mmr_cache.get_mmr_only_root(), Ok(cp2_mmr_only_root.clone()));

    // Two checkpoint update
    checkpoint_db
        .push(MerkleCheckPoint::new(vec![h5.clone(), h6.clone()], Bitmap::create(), 0))
        .unwrap();
    checkpoint_db
        .push(MerkleCheckPoint::new(vec![h7.clone(), h8.clone()], Bitmap::create(), 0))
        .unwrap();
    assert!(mmr_cache.update().is_ok());
    assert_eq!(mmr_cache.get_mmr_only_root(), Ok(cp4_mmr_only_root.clone()));

    // No rewind
    checkpoint_db.truncate(4).unwrap();
    assert!(mmr_cache.update().is_ok());
    assert_eq!(mmr_cache.get_mmr_only_root(), Ok(cp4_mmr_only_root));

    // Only current MMR update
    checkpoint_db.truncate(3).unwrap();
    assert!(mmr_cache.update().is_ok());
    assert_eq!(mmr_cache.get_mmr_only_root(), Ok(cp3_mmr_only_root));

    // Full cache update
    checkpoint_db.truncate(1).unwrap();
    assert!(mmr_cache.update().is_ok());
    assert_eq!(mmr_cache.get_mmr_only_root(), Ok(cp1_mmr_only_root));
}

#[test]
fn multiple_rewinds() {
    let config = MmrCacheConfig { rewind_hist_len: 2 };
    let mut checkpoint_db = MemBackendVec::<MerkleCheckPoint>::new();
    let mut mmr_cache = MmrCache::<Hasher, _, _>::new(Vec::new(), checkpoint_db.clone(), config).unwrap();

    // Add h1, h2, h3 and h4 checkpoints
    let h1 = int_to_hash(1);
    let h2 = int_to_hash(2);
    let h3 = int_to_hash(3);
    let h4 = int_to_hash(4);
    let h5 = int_to_hash(5);
    checkpoint_db
        .push(MerkleCheckPoint::new(vec![h1.clone()], Bitmap::create(), 0))
        .unwrap();
    assert!(mmr_cache.update().is_ok());
    assert_eq!(mmr_cache.get_mmr_only_root(), Ok(combine_hashes(&[&h1]).clone()));

    checkpoint_db
        .push(MerkleCheckPoint::new(vec![h2.clone()], Bitmap::create(), 0))
        .unwrap();
    assert!(mmr_cache.update().is_ok());
    let h1h2 = combine_hashes(&[&h1, &h2]);
    assert_eq!(mmr_cache.get_mmr_only_root(), Ok(combine_hashes(&[&h1h2]).clone()));

    checkpoint_db
        .push(MerkleCheckPoint::new(vec![h3.clone()], Bitmap::create(), 0))
        .unwrap();
    assert!(mmr_cache.update().is_ok());
    assert_eq!(mmr_cache.get_mmr_only_root(), Ok(combine_hashes(&[&h1h2, &h3]).clone()));

    checkpoint_db
        .push(MerkleCheckPoint::new(vec![h4.clone()], Bitmap::create(), 0))
        .unwrap();
    assert!(mmr_cache.update().is_ok());
    let h3h4 = combine_hashes(&[&h3, &h4]);
    let h1h2h3h4 = combine_hashes(&[&h1h2, &h3h4]);
    assert_eq!(mmr_cache.get_mmr_only_root(), Ok(combine_hashes(&[&h1h2h3h4]).clone()));
    assert_eq!(checkpoint_db.len(), Ok(4));

    // Remove h4 checkpoint
    checkpoint_db.truncate(3).unwrap();
    assert_eq!(checkpoint_db.len(), Ok(3));
    assert!(mmr_cache.update().is_ok());
    assert_eq!(mmr_cache.get_mmr_only_root(), Ok(combine_hashes(&[&h1h2, &h3]).clone()));

    // Add h5 checkpoint
    checkpoint_db
        .push(MerkleCheckPoint::new(vec![h5.clone()], Bitmap::create(), 0))
        .unwrap();
    assert!(mmr_cache.update().is_ok());
    let h3h5 = combine_hashes(&[&h3, &h5]);
    let h1h2h3h5 = combine_hashes(&[&h1h2, &h3h5]);
    assert_eq!(mmr_cache.get_mmr_only_root(), Ok(combine_hashes(&[&h1h2h3h5]).clone()));

    // Remove h5 checkpoint
    checkpoint_db.truncate(3).unwrap();
    assert_eq!(checkpoint_db.len(), Ok(3));
    assert!(mmr_cache.update().is_ok());
    assert_eq!(mmr_cache.get_mmr_only_root(), Ok(combine_hashes(&[&h1h2, &h3]).clone()));

    // Remove h3 checkpoint
    checkpoint_db.truncate(2).unwrap();
    assert_eq!(checkpoint_db.len(), Ok(2));
    assert!(mmr_cache.update().is_ok());
    assert_eq!(mmr_cache.get_mmr_only_root(), Ok(combine_hashes(&[&h1h2]).clone()));
}

#[test]
fn checkpoint_merging() {
    let config = MmrCacheConfig { rewind_hist_len: 2 };
    let mut checkpoint_db = MemBackendVec::<MerkleCheckPoint>::new();
    let mut mmr_cache = MmrCache::<Hasher, _, _>::new(Vec::new(), checkpoint_db.clone(), config).unwrap();

    let h1 = int_to_hash(1);
    let h2 = int_to_hash(2);
    let h3 = int_to_hash(3);
    let h4 = int_to_hash(4);
    let h5 = int_to_hash(5);
    let h6 = int_to_hash(6);
    let ha = combine_hashes(&[&h1, &h2]);
    let hb = combine_hashes(&[&h3, &h4]);
    let hc = combine_hashes(&[&h5, &h6]);
    let hahb = combine_hashes(&[&ha, &hb]);
    let cp4_mmr_only_root = combine_hashes(&[&hahb]);
    let cp6_mmr_only_root = combine_hashes(&[&hahb, &hc]);
    let cp1 = MerkleCheckPoint::new(vec![h1.clone()], Bitmap::create(), 0);
    let cp2 = MerkleCheckPoint::new(vec![h2.clone()], Bitmap::create(), 0);
    let cp3 = MerkleCheckPoint::new(vec![h3.clone()], Bitmap::create(), 0);
    let cp4 = MerkleCheckPoint::new(vec![h4.clone()], Bitmap::create(), 0);
    let cp5 = MerkleCheckPoint::new(vec![h5.clone()], Bitmap::create(), 0);
    let cp6 = MerkleCheckPoint::new(vec![h6.clone()], Bitmap::create(), 0);

    checkpoint_db.push(cp1).unwrap();
    assert!(mmr_cache.update().is_ok());
    checkpoint_db.push(cp2).unwrap();
    assert!(mmr_cache.update().is_ok());
    checkpoint_db.push(cp3).unwrap();
    assert!(mmr_cache.update().is_ok());
    checkpoint_db.push(cp4).unwrap();
    assert!(mmr_cache.update().is_ok());
    assert_eq!(mmr_cache.get_mmr_only_root(), Ok(cp4_mmr_only_root.clone()));

    let mut merged_cp = checkpoint_db.get(0).unwrap().unwrap();
    merged_cp.append(checkpoint_db.get(1).unwrap().unwrap());
    assert!(checkpoint_db.shift(2).is_ok());
    assert!(checkpoint_db.push_front(merged_cp).is_ok());
    assert_eq!(checkpoint_db.len(), Ok(3));
    assert!(mmr_cache.checkpoints_merged(2).is_ok());
    assert_eq!(mmr_cache.get_mmr_only_root(), Ok(cp4_mmr_only_root.clone()));

    checkpoint_db.push(cp5).unwrap();
    assert!(mmr_cache.update().is_ok());
    checkpoint_db.push(cp6).unwrap();
    assert!(mmr_cache.update().is_ok());
    assert_eq!(mmr_cache.get_mmr_only_root(), Ok(cp6_mmr_only_root.clone()));

    let mut merged_cp = checkpoint_db.get(0).unwrap().unwrap();
    merged_cp.append(checkpoint_db.get(1).unwrap().unwrap());
    merged_cp.append(checkpoint_db.get(2).unwrap().unwrap());
    assert!(checkpoint_db.shift(3).is_ok());
    assert!(checkpoint_db.push_front(merged_cp).is_ok());
    assert_eq!(checkpoint_db.len(), Ok(3));
    assert!(mmr_cache.checkpoints_merged(3).is_ok());
    assert_eq!(mmr_cache.get_mmr_only_root(), Ok(cp6_mmr_only_root.clone()));

    // Recreate the MmrCache from the altered checkpoints
    let mut mmr_cache = MmrCache::<Hasher, _, _>::new(Vec::new(), checkpoint_db.clone(), config).unwrap();
    assert_eq!(mmr_cache.get_mmr_only_root(), Ok(cp6_mmr_only_root.clone()));

    // Replace all existing checkpoints with a single accumulated checkpoint
    let mut merged_cp = checkpoint_db.get(0).unwrap().unwrap();
    merged_cp.append(checkpoint_db.get(1).unwrap().unwrap());
    merged_cp.append(checkpoint_db.get(2).unwrap().unwrap());
    assert!(checkpoint_db.shift(3).is_ok());
    assert!(checkpoint_db.push_front(merged_cp).is_ok());
    assert_eq!(checkpoint_db.len(), Ok(1));
    assert!(mmr_cache.checkpoints_merged(3).is_ok());
    assert_eq!(mmr_cache.get_mmr_only_root(), Ok(cp6_mmr_only_root.clone()));
}
