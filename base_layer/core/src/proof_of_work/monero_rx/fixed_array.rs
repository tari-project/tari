//  Copyright 2021, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use crate::crypto::tari_utilities::ByteArrayError;
use serde::{Deserialize, Serialize};
use std::{convert::TryFrom, ops::Deref};
use tari_crypto::tari_utilities::ByteArray;

mod arrays {
    use std::{convert::TryInto, marker::PhantomData};

    use serde::{
        de::{SeqAccess, Visitor},
        ser::SerializeTuple,
        Deserialize,
        Deserializer,
        Serialize,
        Serializer,
    };
    pub fn serialize<S: Serializer, T: Serialize, const N: usize>(data: &[T; N], ser: S) -> Result<S::Ok, S::Error> {
        let mut s = ser.serialize_tuple(N)?;
        for item in data {
            s.serialize_element(item)?;
        }
        s.end()
    }

    struct ArrayVisitor<T, const N: usize>(PhantomData<T>);

    impl<'de, T, const N: usize> Visitor<'de> for ArrayVisitor<T, N>
    where T: Deserialize<'de>
    {
        type Value = [T; N];

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str(&format!("an array of length {}", N))
        }

        #[inline]
        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where A: SeqAccess<'de> {
            let mut data = Vec::with_capacity(N);
            for _ in 0..N {
                match (seq.next_element())? {
                    Some(val) => data.push(val),
                    None => return Err(serde::de::Error::invalid_length(N, &self)),
                }
            }
            match data.try_into() {
                Ok(arr) => Ok(arr),
                Err(_) => unreachable!(),
            }
        }
    }
    pub fn deserialize<'de, D, T, const N: usize>(deserializer: D) -> Result<[T; N], D::Error>
    where
        D: Deserializer<'de>,
        T: Deserialize<'de>,
    {
        deserializer.deserialize_tuple(N, ArrayVisitor::<T, N>(PhantomData))
    }
}

pub const MAX_ARR_SIZE: usize = 63;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FixedByteArray<const N: usize> {
    #[serde(with = "arrays")]
    elems: [u8; N],
    len: u8,
}

impl FixedByteArray<MAX_ARR_SIZE> {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn as_slice(&self) -> &[u8] {
        &self[..self.len()]
    }

    #[inline]
    pub fn is_full(&self) -> bool {
        self.len() == MAX_ARR_SIZE
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len as usize
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl Deref for FixedByteArray<MAX_ARR_SIZE> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.elems[..self.len as usize]
    }
}

impl Default for FixedByteArray<MAX_ARR_SIZE> {
    fn default() -> Self {
        Self {
            elems: [0u8; MAX_ARR_SIZE],
            len: 0,
        }
    }
}

impl ByteArray for FixedByteArray<MAX_ARR_SIZE> {
    fn from_bytes(bytes: &[u8]) -> Result<Self, ByteArrayError> {
        let len = u8::try_from(bytes.len()).map_err(|_| ByteArrayError::IncorrectLength)?;
        if len > MAX_ARR_SIZE as u8 {
            return Err(ByteArrayError::IncorrectLength);
        }

        let mut elems = [0u8; MAX_ARR_SIZE];
        elems[..len as usize].copy_from_slice(&bytes[..len as usize]);
        Ok(Self { elems, len })
    }

    fn as_bytes(&self) -> &[u8] {
        self.as_slice()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::crypto::tari_utilities::hex::Hex;
    use tari_crypto::tari_utilities::ByteArray;

    #[test]
    fn assert_size() {
        assert_eq!(std::mem::size_of::<FixedByteArray<MAX_ARR_SIZE>>(), MAX_ARR_SIZE + 1);
    }

    #[test]
    fn from_bytes() {
        let arr = FixedByteArray::from_bytes(&[1u8][..]).unwrap();
        assert_eq!(arr.len(), 1);
        assert!(arr.iter().all(|b| *b == 1));
        // Iterates only up to len
        let mut used = false;
        for _ in arr.iter() {
            assert!(!used);
            used = true;
        }
        assert!(used);

        let arr = FixedByteArray::from_bytes(&[1u8; 63][..]).unwrap();
        assert_eq!(arr.len(), 63);
        assert!(arr.iter().all(|b| *b == 1));

        FixedByteArray::from_bytes(&[1u8; 64][..]).unwrap_err();
    }

    #[test]
    fn serialize_deserialize() {
        let data = bincode::serialize(&FixedByteArray::from_hex("ffffffffffffffffffffffffff").unwrap()).unwrap();
        println!("{:?}", data);
        assert_eq!(data.len(), 64);
        let arr = bincode::deserialize::<FixedByteArray<MAX_ARR_SIZE>>(&data).unwrap();
        assert!(arr.iter().all(|b| *b == 0xff));
        assert_eq!(arr.len(), 13);
    }
}
