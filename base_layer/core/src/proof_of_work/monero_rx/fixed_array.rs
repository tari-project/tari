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
use monero::{
    consensus::{encode, Decodable, Encodable},
    VarInt,
};
use std::{convert::TryFrom, io, ops::Deref};
use tari_crypto::tari_utilities::ByteArray;

const MAX_ARR_SIZE: usize = 63;

#[derive(Clone, Debug)]
pub struct FixedByteArray {
    elems: [u8; MAX_ARR_SIZE],
    len: u8,
}

impl FixedByteArray {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn as_slice(&self) -> &[u8] {
        &self[..self.len()]
    }

    /// Pushes a byte to the end of the array.
    ///
    /// ## Panics
    ///
    /// Panics if the array is full.
    // NOTE: This should be a private function
    fn push(&mut self, elem: u8) {
        assert!(!self.is_full());
        self.elems[self.len()] = elem;
        self.len += 1;
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

impl Deref for FixedByteArray {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.elems[..self.len as usize]
    }
}

#[allow(clippy::derivable_impls)]
impl Default for FixedByteArray {
    fn default() -> Self {
        Self {
            elems: [0u8; MAX_ARR_SIZE],
            len: 0,
        }
    }
}

impl ByteArray for FixedByteArray {
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

impl Decodable for FixedByteArray {
    fn consensus_decode<D: io::Read>(d: &mut D) -> Result<Self, encode::Error> {
        let len = VarInt::consensus_decode(d)?.0 as usize;
        if len > MAX_ARR_SIZE {
            return Err(encode::Error::ParseFailed(
                "length exceeded maximum of 64-bytes for FixedByteArray",
            ));
        }
        let mut ret = FixedByteArray::new();
        for _ in 0..len {
            // PANIC: Cannot happen because len has been checked
            ret.push(Decodable::consensus_decode(d)?);
        }
        Ok(ret)
    }
}

impl Encodable for FixedByteArray {
    fn consensus_encode<E: io::Write>(&self, e: &mut E) -> Result<usize, io::Error> {
        self.as_slice().consensus_encode(e)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::crypto::tari_utilities::hex::Hex;
    use monero::consensus;

    #[test]
    fn assert_size() {
        assert_eq!(std::mem::size_of::<FixedByteArray>(), MAX_ARR_SIZE + 1);
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
        let data = consensus::serialize(&FixedByteArray::from_hex("ffffffffffffffffffffffffff").unwrap());
        assert_eq!(data.len(), 13 + 1);
        let arr = consensus::deserialize::<FixedByteArray>(&data).unwrap();
        assert!(arr.iter().all(|b| *b == 0xff));
    }

    #[test]
    fn length_check() {
        let mut buf = [0u8; MAX_ARR_SIZE + 1];
        buf[0] = 63;
        let arr = FixedByteArray::consensus_decode(&mut io::Cursor::new(buf)).unwrap();
        assert_eq!(arr.len(), MAX_ARR_SIZE);

        buf[0] = 64;
        let err = FixedByteArray::consensus_decode(&mut io::Cursor::new(buf)).unwrap_err();
        assert!(matches!(err, encode::Error::ParseFailed(_)));

        // VarInt encoding that doesnt terminate, but _would_ represent a number < MAX_ARR_SIZE
        buf[0] = 0b1000000;
        buf[1] = 0b1000000;
        let err = FixedByteArray::consensus_decode(&mut io::Cursor::new(buf)).unwrap_err();
        assert!(matches!(err, encode::Error::ParseFailed(_)));
    }

    #[test]
    fn capacity_overflow_does_not_panic() {
        let data = &[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7f];
        let _ = consensus::deserialize::<FixedByteArray>(data);
    }
}
