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
//

pub mod hash {
    use std::fmt;

    use serde::{
        Deserializer,
        Serializer,
        de::{self, SeqAccess, Visitor},
        ser::SerializeSeq,
    };
    use tari_utilities::hex::{self, Hex};

    use crate::Hash;

    /// Maximum number of hashes accepted in a sequence. A merkle path and the set of peaks are each bounded by the
    /// height of an MMR whose size fits in a `u64`, so any longer sequence is invalid.
    pub const MAX_HASHES: usize = 64;

    pub fn serialize<S>(hashes: &[Hash], ser: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        let is_human_readable = ser.is_human_readable();
        let mut sequence = ser.serialize_seq(Some(hashes.len()))?;
        for hash in hashes {
            if is_human_readable {
                sequence.serialize_element(&hash.to_hex())?;
            } else {
                sequence.serialize_element(hash.as_slice())?;
            }
        }
        sequence.end()
    }

    pub fn deserialize<'de, D>(de: D) -> Result<Vec<Hash>, D::Error>
    where D: Deserializer<'de> {
        struct HashVecVisitor(bool);

        impl<'de> Visitor<'de> for HashVecVisitor {
            type Value = Vec<Hash>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write!(formatter, "a vector of at most {MAX_HASHES} hashes")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where A: SeqAccess<'de> {
                let is_human_readable = self.0;
                if let Some(len) = seq.size_hint().filter(|len| *len > MAX_HASHES) {
                    return Err(de::Error::invalid_length(len, &self));
                }
                let mut result = Vec::<Hash>::with_capacity(seq.size_hint().unwrap_or(0));
                loop {
                    let next = if is_human_readable {
                        seq.next_element::<String>()?
                            .map(|v| hex::from_hex(&v).map_err(de::Error::custom))
                            .transpose()?
                    } else {
                        seq.next_element()?
                    };
                    let Some(hash) = next else { break };
                    if result.len() == MAX_HASHES {
                        return Err(de::Error::invalid_length(MAX_HASHES + 1, &self));
                    }
                    result.push(hash);
                }
                Ok(result)
            }
        }
        let is_human_readable = de.is_human_readable();
        de.deserialize_seq(HashVecVisitor(is_human_readable))
    }
}

#[cfg(test)]
mod test {
    use super::hash::MAX_HASHES;
    use crate::MerkleProof;

    fn proof_with_path_len(len: usize) -> MerkleProof {
        MerkleProof {
            mmr_size: 3,
            path: vec![vec![1u8; 32]; len],
            peaks: vec![vec![3u8; 32]],
        }
    }

    #[test]
    fn it_rejects_an_oversized_length_prefix() {
        // 1 << 52 is the length prefix seen in a real-world incident
        for len in [MAX_HASHES as u64 + 1, 1 << 52, u64::MAX] {
            // mmr_size, then a path length prefix with no elements following
            let mut buf = 0u64.to_le_bytes().to_vec();
            buf.extend_from_slice(&len.to_le_bytes());
            let err = bincode::deserialize::<MerkleProof>(&buf).unwrap_err();
            assert!(
                err.to_string().contains("invalid length"),
                "len {len}: unexpected error: {err}"
            );
        }
    }

    #[test]
    fn it_rejects_more_than_max_hashes() {
        let proof = proof_with_path_len(MAX_HASHES + 1);
        let bytes = bincode::serialize(&proof).unwrap();
        bincode::deserialize::<MerkleProof>(&bytes).unwrap_err();
        let json = serde_json::to_string(&proof).unwrap();
        serde_json::from_str::<MerkleProof>(&json).unwrap_err();
    }

    #[test]
    fn it_round_trips() {
        for len in [0, 2, MAX_HASHES] {
            let proof = proof_with_path_len(len);
            let bytes = bincode::serialize(&proof).unwrap();
            assert_eq!(bincode::deserialize::<MerkleProof>(&bytes).unwrap(), proof);
            let json = serde_json::to_string(&proof).unwrap();
            assert_eq!(serde_json::from_str::<MerkleProof>(&json).unwrap(), proof);
        }
    }
}
