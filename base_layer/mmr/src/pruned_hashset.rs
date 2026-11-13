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

use std::convert::TryFrom;

use digest::Digest;
use serde::{Deserialize, Serialize};

use crate::{ArrayLike, Hash, MerkleMountainRange, common::find_peaks, error::MerkleMountainRangeError};

/// This is a specialised struct that represents a pruned hash set for Merkle Mountain Ranges.
///
/// The basic idea is that when adding a new hash, only the peaks to the left of the new node hierarchy are ever needed.
/// This means that if we don't care about the data earlier than a given leaf node index, n_0, (i.e. we still have the
/// hashes, but can't recalculate them from source), we _only need to store the local peaks for the MMR at that time_
/// and we can forget about the rest. There will never be a request for a hash other than those at the peaks for the
/// MMR with n_0 leaf nodes.
///
/// The awesome thing is that this struct can be dropped into [MerkleMountainRange] as a backend and it. just. works.
///
/// Invariants, enforced on deserialization and checked by [PrunedHashSet::validate]:
/// 1. `base_offset` is a valid MMR size (`find_peaks(base_offset)` is `Some`), including 0.
/// 2. `peak_indices` equals `find_peaks(base_offset)` exactly, so it is sorted ascending.
/// 3. `peak_hashes` has one hash per peak index.
/// 4. `base_offset + hashes.len()` does not overflow `usize` and is a valid MMR size.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(try_from = "RawPrunedHashSet")]
pub struct PrunedHashSet {
    /// The size of the base MMR. Only peaks are available for indices less than this value
    base_offset: usize,
    /// The array of peak indices for an MMR of size `base_offset`
    peak_indices: Vec<usize>,
    /// The array of hashes at the MMR peaks
    peak_hashes: Vec<Hash>,
    /// New hashes added subsequent to `base_offset`.
    hashes: Vec<Hash>,
}

/// Unvalidated wire form of [PrunedHashSet]; same fields in the same order, so the encoding is unchanged.
#[derive(Deserialize)]
struct RawPrunedHashSet {
    base_offset: usize,
    peak_indices: Vec<usize>,
    peak_hashes: Vec<Hash>,
    hashes: Vec<Hash>,
}

impl TryFrom<RawPrunedHashSet> for PrunedHashSet {
    type Error = MerkleMountainRangeError;

    fn try_from(raw: RawPrunedHashSet) -> Result<Self, Self::Error> {
        let set = PrunedHashSet {
            base_offset: raw.base_offset,
            peak_indices: raw.peak_indices,
            peak_hashes: raw.peak_hashes,
            hashes: raw.hashes,
        };
        set.validate()?;
        Ok(set)
    }
}

impl PrunedHashSet {
    /// Checks the struct invariants listed on [PrunedHashSet].
    pub fn validate(&self) -> Result<(), MerkleMountainRangeError> {
        let invalid = |msg: String| Err(MerkleMountainRangeError::BackendError(format!("PrunedHashSet: {msg}")));
        let Some(peaks) = find_peaks(self.base_offset) else {
            return invalid(format!("base_offset {} is not a valid MMR size", self.base_offset));
        };
        if self.peak_indices != peaks {
            return invalid(format!(
                "peak_indices {:?} are not the peaks of base_offset {}",
                self.peak_indices, self.base_offset
            ));
        }
        if self.peak_hashes.len() != self.peak_indices.len() {
            return invalid(format!(
                "peak_hashes has {} entries, expected {}",
                self.peak_hashes.len(),
                self.peak_indices.len()
            ));
        }
        let Some(total) = self.base_offset.checked_add(self.hashes.len()) else {
            return invalid("base_offset + hashes length overflows".to_string());
        };
        if find_peaks(total).is_none() {
            return invalid(format!("base_offset + hashes length {total} is not a valid MMR size"));
        }
        Ok(())
    }
}

impl<D, B> TryFrom<&MerkleMountainRange<D, B>> for PrunedHashSet
where
    D: Digest,
    B: ArrayLike<Value = Hash>,
{
    type Error = MerkleMountainRangeError;

    fn try_from(base_mmr: &MerkleMountainRange<D, B>) -> Result<Self, Self::Error> {
        let base_offset = base_mmr.len()?;
        let peak_indices = find_peaks(base_offset).ok_or(MerkleMountainRangeError::InvalidMmrSize)?;
        let peak_hashes = peak_indices
            .iter()
            .map(|i| match base_mmr.get_node_hash(*i)? {
                Some(h) => Ok(h),
                None => Err(MerkleMountainRangeError::HashNotFound(*i)),
            })
            .collect::<Result<_, _>>()?;
        let set = PrunedHashSet {
            base_offset,
            peak_indices,
            peak_hashes,
            hashes: Vec::new(),
        };
        debug_assert!(set.validate().is_ok());
        Ok(set)
    }
}

impl ArrayLike for PrunedHashSet {
    type Error = MerkleMountainRangeError;
    type Value = Hash;

    fn len(&self) -> Result<usize, Self::Error> {
        Ok(self.base_offset.saturating_add(self.hashes.len()))
    }

    fn is_empty(&self) -> Result<bool, Self::Error> {
        Ok(self.len()? == 0)
    }

    fn push(&mut self, item: Self::Value) -> Result<usize, Self::Error> {
        self.hashes.push(item);
        Ok(self.len()?.saturating_sub(1))
    }

    // fn get(&self, index: usize) -> Result<Option<Self::Value>, Self::Error> {
    //     // If the index is from before we started adding hashes, we can return the hash *if and only if* it is a peak
    //     if index < self.base_offset {
    //         return Ok(match self.peak_indices.binary_search(&index) {
    //             Ok(nth_peak) => self.peak_hashes.get(nth_peak),
    //             Err(_) => None,
    //         });
    //     }
    //     self.hashes.get(index - self.base_offset)
    // }

    fn get(&self, index: usize) -> Option<Self::Value> {
        // If the index is from before we started adding hashes, we can return the hash *if and only if* it is a peak
        if index < self.base_offset {
            return match self.peak_indices.binary_search(&index) {
                Ok(nth_peak) => self.peak_hashes.get(nth_peak),
                Err(_) => None,
            };
        }
        self.hashes.get(index.saturating_sub(self.base_offset))
    }

    fn clear(&mut self) -> Result<(), Self::Error> {
        self.base_offset = 0;
        self.peak_indices.clear();
        self.peak_hashes.clear();
        self.hashes.clear();
        Ok(())
    }

    fn position(&self, item: &Self::Value) -> Result<Option<usize>, Self::Error> {
        for index in 0..self.len()? {
            if let Some(stored_item) = self.get(index) &&
                stored_item == *item
            {
                return Ok(Some(index));
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod test {
    use blake2::Blake2b;
    use digest::consts::U32;
    use tari_utilities::hex::to_hex;

    use super::*;

    type TestMmr<B> = MerkleMountainRange<Blake2b<U32>, B>;

    /// Mirror of [RawPrunedHashSet] that can be re-encoded after tampering.
    #[derive(Serialize, Deserialize)]
    struct Mirror {
        base_offset: usize,
        peak_indices: Vec<usize>,
        peak_hashes: Vec<Hash>,
        hashes: Vec<Hash>,
    }

    fn create_mmr(leaves: usize) -> TestMmr<Vec<Hash>> {
        let mut mmr = TestMmr::new(Vec::new());
        for i in 0..leaves {
            mmr.push(Blake2b::<U32>::digest(i.to_le_bytes()).to_vec()).unwrap();
        }
        mmr
    }

    fn tampered(f: impl FnOnce(&mut Mirror)) -> String {
        let bytes = bincode::serialize(&PrunedHashSet::try_from(&create_mmr(11)).unwrap()).unwrap();
        let mut raw: Mirror = bincode::deserialize(&bytes).unwrap();
        f(&mut raw);
        bincode::deserialize::<PrunedHashSet>(&bincode::serialize(&raw).unwrap())
            .unwrap_err()
            .to_string()
    }

    #[test]
    fn serialization_is_unchanged() {
        let mut pruned = TestMmr::new(PrunedHashSet::try_from(&create_mmr(5)).unwrap());
        pruned.push(vec![7u8; 32]).unwrap();
        let bytes = bincode::serialize(&pruned.hashes).unwrap();
        assert_eq!(
            to_hex(&bytes),
            "080000000000000002000000000000000600000000000000070000000000000002000000000000002000000000000000ef390e\
             c690d2b2ebd8bdc89057894b9e5d897039157db457dedc85b7d263eaa12000000000000000e96760d274653a39b429a87ebaae9d\
             3aa4fdf58b9096cf0bebc7c4e5a4c2ed8d0200000000000000200000000000000007070707070707070707070707070707070707\
             070707070707070707070707072000000000000000a795cd760caa7079ba597ca664261f1c54de761630c65a9af57be28a54583e\
             5a"
        );
        let decoded: PrunedHashSet = bincode::deserialize(&bytes).unwrap();
        assert_eq!(bincode::serialize(&decoded).unwrap(), bytes);
    }

    #[test]
    fn deserialize_rejects_wrong_peak_indices() {
        // 11 leaves -> 19 nodes, peaks [14, 17, 18]
        let dropped = tampered(|r| {
            r.peak_indices.pop();
        });
        assert!(dropped.contains("peak_indices"), "{dropped}");
        let reordered = tampered(|r| r.peak_indices.reverse());
        assert!(reordered.contains("peak_indices"), "{reordered}");
        let non_peak = tampered(|r| {
            if let Some(i) = r.peak_indices.first_mut() {
                *i = 3;
            }
        });
        assert!(non_peak.contains("peak_indices"), "{non_peak}");
        let appended = tampered(|r| {
            r.peak_indices.push(3);
            r.peak_hashes.push(vec![0u8; 32]);
        });
        assert!(appended.contains("peak_indices"), "{appended}");
    }

    #[test]
    fn deserialize_rejects_short_peak_hashes() {
        let err = tampered(|r| {
            r.peak_hashes.pop();
        });
        assert!(err.contains("peak_hashes"), "{err}");
    }

    #[test]
    fn deserialize_rejects_invalid_base_offset() {
        let err = tampered(|r| {
            r.base_offset = 2;
            r.peak_indices.clear();
            r.peak_hashes.clear();
        });
        assert!(err.contains("base_offset"), "{err}");
    }

    #[test]
    fn deserialize_rejects_overflowing_length() {
        let err = tampered(|r| {
            r.base_offset = usize::MAX;
            r.peak_indices = find_peaks(usize::MAX).unwrap();
            r.peak_hashes.truncate(1);
            r.hashes.push(vec![0u8; 32]);
        });
        assert!(err.contains("overflows"), "{err}");
    }

    #[test]
    fn deserialize_rejects_invalid_total_size() {
        let h = vec![0u8; 32];
        let raw = Mirror {
            base_offset: 1,
            peak_indices: vec![0],
            peak_hashes: vec![h.clone()],
            hashes: vec![h],
        };
        let err = bincode::deserialize::<PrunedHashSet>(&bincode::serialize(&raw).unwrap())
            .unwrap_err()
            .to_string();
        assert!(err.contains("hashes length"), "{err}");
    }

    #[test]
    fn deserialize_accepts_every_valid_size() {
        for leaves in 0..=64 {
            let mmr = create_mmr(leaves);
            let bytes = bincode::serialize(&PrunedHashSet::try_from(&mmr).unwrap()).unwrap();
            let decoded: PrunedHashSet = bincode::deserialize(&bytes).unwrap();
            let mut pruned = TestMmr::new(decoded);
            assert_eq!(pruned.get_merkle_root().unwrap(), mmr.get_merkle_root().unwrap());
            // Every state reached by a completed push must stay valid.
            for i in 0..8u8 {
                pruned.push(vec![i; 32]).unwrap();
                pruned.hashes.validate().unwrap();
            }
        }
    }

    /// Backend that reports one node as missing.
    struct MissingNode {
        nodes: Vec<Hash>,
        missing: usize,
    }

    impl ArrayLike for MissingNode {
        type Error = MerkleMountainRangeError;
        type Value = Hash;

        fn len(&self) -> Result<usize, Self::Error> {
            Ok(self.nodes.len())
        }

        fn is_empty(&self) -> Result<bool, Self::Error> {
            Ok(self.nodes.is_empty())
        }

        fn push(&mut self, item: Hash) -> Result<usize, Self::Error> {
            self.nodes.push(item);
            Ok(self.nodes.len().saturating_sub(1))
        }

        fn get(&self, index: usize) -> Option<Hash> {
            (index != self.missing).then(|| self.nodes.get(index)).flatten()
        }

        fn clear(&mut self) -> Result<(), Self::Error> {
            self.nodes.clear();
            Ok(())
        }

        fn position(&self, _item: &Hash) -> Result<Option<usize>, Self::Error> {
            Ok(None)
        }
    }

    #[test]
    fn push_and_root_return_error_on_missing_peak() {
        // 3 leaves -> 4 nodes with peaks [2, 3]; a 4th leaf needs left siblings 3 then 2.
        let missing = |missing| {
            TestMmr::new(MissingNode {
                nodes: create_mmr(3).hashes,
                missing,
            })
        };
        assert_eq!(
            missing(3).get_merkle_root(),
            Err(MerkleMountainRangeError::HashNotFound(3))
        );
        assert_eq!(
            missing(3).push(vec![1u8; 32]),
            Err(MerkleMountainRangeError::HashNotFound(3))
        );
        assert_eq!(
            missing(2).push(vec![1u8; 32]),
            Err(MerkleMountainRangeError::HashNotFound(2))
        );
    }
}
