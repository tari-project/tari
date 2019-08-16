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

// TODO - move all the to_hex serde stuff into a common module
pub mod hash {
    use crate::Hash;
    use serde::{
        de::{self, SeqAccess, Visitor},
        ser::SerializeSeq,
        Deserializer,
        Serializer,
    };
    use std::fmt;
    use tari_utilities::hex::{self, Hex};

    pub fn serialize<S>(hashes: &[Hash], ser: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        let is_human_readable = ser.is_human_readable();
        let mut seq = ser.serialize_seq(Some(hashes.len()))?;
        for hash in hashes {
            if is_human_readable {
                seq.serialize_element(&hash.to_hex())?;
            } else {
                seq.serialize_element(hash.as_slice())?;
            }
        }
        seq.end()
    }

    pub fn deserialize<'de, D>(de: D) -> Result<Vec<Hash>, D::Error>
    where D: Deserializer<'de> {
        struct HashVecVisitor(bool);

        impl<'de> Visitor<'de> for HashVecVisitor {
            type Value = Vec<Hash>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a vector of hashes")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where A: SeqAccess<'de> {
                let is_human_readable = self.0;
                let mut result = Vec::<Hash>::with_capacity(seq.size_hint().unwrap_or(10));
                if is_human_readable {
                    while let Some(v) = seq.next_element::<String>()? {
                        let val = hex::from_hex(&v).map_err(de::Error::custom)?;
                        result.push(val);
                    }
                } else {
                    while let Some(v) = seq.next_element()? {
                        result.push(v);
                    }
                }
                Ok(result)
            }
        }
        let is_human_readable = de.is_human_readable();
        de.deserialize_seq(HashVecVisitor(is_human_readable))
    }
}
