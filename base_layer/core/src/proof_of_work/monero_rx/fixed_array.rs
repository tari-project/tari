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

use std::{io, io::Write, ops::Deref};

use borsh::{BorshDeserialize, BorshSerialize};
use tari_utilities::{hex::Hex, ByteArrayError};

const MAX_ARR_SIZE: usize = 63;

/// A fixed size byte array for RandomX that can be serialized and deserialized using Borsh.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FixedByteArray {
    elems: [u8; MAX_ARR_SIZE],
    len: u8,
}

impl BorshSerialize for FixedByteArray {
    fn serialize<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        self.len.serialize(writer)?;
        let data = self.as_slice();
        for item in data.iter().take(self.len as usize) {
            item.serialize(writer)?;
        }
        Ok(())
    }
}

impl BorshDeserialize for FixedByteArray {
    fn deserialize_reader<R>(reader: &mut R) -> Result<Self, io::Error>
    where R: io::Read {
        let len = u8::deserialize_reader(reader)? as usize;
        if len > MAX_ARR_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("length exceeded maximum of 63-bytes for FixedByteArray: {}", len),
            ));
        }
        let mut bytes = Vec::with_capacity(len);
        for _ in 0..len {
            bytes.push(u8::deserialize_reader(reader)?);
        }

        let mut elems = [0u8; MAX_ARR_SIZE];
        elems[..len].copy_from_slice(&bytes[..len]);
        Ok(Self { elems, len: len as u8 })
    }
}

impl FixedByteArray {
    /// Create a new FixedByteArray with the preset length. The array will be zeroed.
    pub fn new() -> Self {
        Default::default()
    }

    /// Returns the array as a slice of bytes.
    pub fn as_slice(&self) -> &[u8] {
        &self[..self.len()]
    }

    /// Returns true if the array is full.
    #[inline]
    pub fn is_full(&self) -> bool {
        self.len() == MAX_ARR_SIZE
    }

    /// Returns the length of the array.
    #[inline]
    pub fn len(&self) -> usize {
        self.len as usize
    }

    /// Returns true if the array is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn to_hex(&self) -> String {
        self.elems.to_hex()
    }

    pub fn from_hex(hex: &str) -> Result<Self, ByteArrayError> {
        let bytes = Vec::<u8>::from_hex(hex).map_err(|e| ByteArrayError::ConversionError {
            reason: format!("Invalid Hex:{}", e),
        })?;

        if bytes.len() > MAX_ARR_SIZE {
            return Err(ByteArrayError::IncorrectLength {});
        }

        let mut elems = [0u8; MAX_ARR_SIZE];
        elems[..bytes.len()].copy_from_slice(&bytes[..]);

        Ok(Self {
            elems,
            len: bytes.len() as u8,
        })
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

#[cfg(test)]
mod test {
    use std::convert::TryFrom;

    use super::*;

    #[test]
    fn assert_size() {
        assert_eq!(std::mem::size_of::<FixedByteArray>(), MAX_ARR_SIZE + 1);
    }

    #[test]
    fn capacity_overflow_does_not_panic() {
        let data = &[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7f];
        let _result = FixedByteArray::deserialize(&mut data.as_slice()).unwrap_err();
    }

    #[test]
    fn length_check() {
        let mut buf = [u8::try_from(MAX_ARR_SIZE).unwrap(); MAX_ARR_SIZE + 1];
        let fixed_byte_array = FixedByteArray::deserialize(&mut buf.as_slice()).unwrap();
        assert_eq!(fixed_byte_array.len(), MAX_ARR_SIZE);
        buf[0] += 1;
        FixedByteArray::deserialize(&mut buf.as_slice()).unwrap_err();
    }

    #[test]
    fn test_borsh_de_serialization() {
        let fixed_byte_array = FixedByteArray::from_hex("050607").unwrap();
        let mut buf = Vec::new();
        fixed_byte_array.serialize(&mut buf).unwrap();
        buf.extend_from_slice(&[1, 2, 3]);
        let buf = &mut buf.as_slice();
        assert_eq!(fixed_byte_array, FixedByteArray::deserialize(buf).unwrap());
        assert_eq!(buf, &[1, 2, 3]);
    }
}
