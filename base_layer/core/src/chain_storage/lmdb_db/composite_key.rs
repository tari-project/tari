//  Copyright 2022, The Tari Project
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

use std::{
    convert::TryFrom,
    fmt::{Display, Formatter},
    ops::Deref,
};

use lmdb_zero::traits::AsLmdbBytes;
use tari_common_types::types::FixedHash;

use crate::chain_storage::ChainStorageError;

#[derive(Debug, Clone)]
enum SmallBytes<const L: usize> {
    Stack([u8; L]),
    Heap(Box<[u8; L]>),
}

impl<const L: usize> SmallBytes<L> {
    pub fn as_slice(&self) -> &[u8] {
        match self {
            SmallBytes::Stack(b) => b.as_ref(),
            SmallBytes::Heap(b) => b.as_ref(),
        }
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        match self {
            SmallBytes::Stack(b) => b.as_mut(),
            SmallBytes::Heap(b) => b.as_mut(),
        }
    }
}

impl<const L: usize> Deref for SmallBytes<L> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        match self {
            SmallBytes::Stack(b) => b,
            SmallBytes::Heap(b) => &**b,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct CompositeKey<const L: usize> {
    bytes: SmallBytes<L>,
    len: usize,
}

impl<const L: usize> CompositeKey<L> {
    pub(super) fn new() -> Self {
        Self {
            bytes: Self::new_buf(),
            len: 0,
        }
    }

    pub fn try_from_parts<T: AsRef<[u8]>>(parts: &[T]) -> Result<Self, ChainStorageError> {
        let mut key = Self::new();
        for part in parts {
            if !key.push(part) {
                return Err(ChainStorageError::CompositeKeyLengthExceeded);
            }
        }
        Ok(key)
    }

    pub fn push<T: AsRef<[u8]>>(&mut self, bytes: T) -> bool {
        let b = bytes.as_ref();
        let new_len = self.len + b.len();
        if new_len > L {
            return false;
        }
        self.bytes.as_mut_slice()[self.len..new_len].copy_from_slice(b);
        self.len = new_len;
        true
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes.as_slice()[..self.len]
    }

    pub fn to_be_u64(&self, offset: usize) -> Result<u64, ChainStorageError> {
        if offset + 8 > self.len {
            return Err(ChainStorageError::CompositeKeyLengthExceeded);
        }
        let mut buf = [0u8; 8];
        buf.copy_from_slice(&self.bytes[offset..offset + 8]);
        Ok(u64::from_be_bytes(buf))
    }

    /// Returns a fixed 0-filled byte array.
    fn new_buf() -> SmallBytes<L> {
        if L <= 64 {
            return SmallBytes::Stack([0u8; L]);
        }
        SmallBytes::Heap(Box::new([0x0u8; L]))
    }

    pub fn section_iter<const SECTIONS: usize>(&self, sections: [usize; SECTIONS]) -> SectionIter<'_, SECTIONS> {
        SectionIter {
            sections,
            current: 0,
            pointer: 0,
            slice: self.as_bytes(),
        }
    }
}

impl<const L: usize> Display for CompositeKey<L> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        for b in self.as_bytes() {
            write!(f, "{:02x}", b)?;
        }
        Ok(())
    }
}

impl<const L: usize> Deref for CompositeKey<L> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.as_bytes()
    }
}

impl<const L: usize> AsRef<[u8]> for CompositeKey<L> {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl<const L: usize> AsLmdbBytes for CompositeKey<L> {
    fn as_lmdb_bytes(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl<const L: usize> TryFrom<&[u8]> for CompositeKey<L> {
    type Error = ChainStorageError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.len() > L {
            return Err(ChainStorageError::CompositeKeyLengthExceeded);
        }
        let mut key = Self::new();
        key.bytes.as_mut_slice()[..value.len()].copy_from_slice(value);
        key.len = value.len();
        Ok(key)
    }
}

#[derive(Debug, Clone)]
pub(super) struct OutputKey(pub(super) CompositeKey<68>);

impl OutputKey {
    pub fn new(header_hash: &FixedHash, utxo_hash: &FixedHash) -> Result<Self, ChainStorageError> {
        let com_key = CompositeKey::try_from_parts(&[header_hash.as_slice(), utxo_hash.as_slice()])?;
        Ok(Self(com_key))
    }

    pub(super) fn convert_to_comp_key(self) -> CompositeKey<68> {
        self.0
    }
}

#[derive(Debug, Clone)]
pub(super) struct InputKey(pub(super) CompositeKey<68>);

impl InputKey {
    pub fn new(header_hash: &FixedHash, txo_hash: &FixedHash) -> Result<Self, ChainStorageError> {
        let com_key = CompositeKey::try_from_parts(&[header_hash.as_slice(), txo_hash.as_slice()])?;
        Ok(Self(com_key))
    }

    pub(super) fn convert_to_comp_key(self) -> CompositeKey<68> {
        self.0
    }
}

pub(super) struct SectionIter<'a, const SECTIONS: usize> {
    sections: [usize; SECTIONS],
    current: usize,
    pointer: usize,
    slice: &'a [u8],
}

impl<const SECTIONS: usize> SectionIter<'_, SECTIONS> {
    /// Returns the next 8 bytes as a u64.
    ///
    /// # Panics
    /// Panics if the next section is not 8 bytes long.
    pub fn next_be_u64(&mut self) -> Option<u64> {
        let bytes = self.next()?;
        assert_eq!(
            bytes.len(),
            8,
            "Next section is not 8 bytes long. Section length: {}",
            bytes.len()
        );
        let mut buf = [0u8; 8];
        buf.copy_from_slice(bytes);
        Some(u64::from_be_bytes(buf))
    }
}

impl<'a, const SECTIONS: usize> Iterator for SectionIter<'a, SECTIONS> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        let cur_section = *self.sections.get(self.current)?;

        let lower = self.pointer;
        let upper = self.pointer + cur_section;
        self.current += 1;
        self.pointer = upper;

        if upper > self.slice.len() {
            return None;
        }

        Some(&self.slice[lower..upper])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod section_iter {
        use super::*;

        #[test]
        fn it_returns_section_slices() {
            let key = CompositeKey::<10>::try_from_parts(&[&[1, 3][..], &[6], &[7, 8, 9]]).unwrap();
            let mut iter = key.section_iter([2, 1, 3, 0]);
            assert_eq!(iter.next(), Some(&[1, 3][..]));
            assert_eq!(iter.next(), Some(&[6][..]));
            assert_eq!(iter.next(), Some(&[7, 8, 9][..]));
            assert_eq!(iter.next(), Some(&[][..]));
            assert_eq!(iter.next(), None);
        }

        #[test]
        fn it_returns_none_if_sections_dont_exist() {
            let key = CompositeKey::<10>::try_from_parts(&[&[1, 3][..]]).unwrap();
            let mut iter = key.section_iter([2, 1, 3]);
            assert_eq!(iter.next(), Some(&[1, 3][..]));
            assert_eq!(iter.next(), None);
            assert_eq!(iter.next(), None);
            assert_eq!(iter.next(), None);
        }

        #[test]
        fn it_returns_none_for_less_sections_than_len() {
            let key = CompositeKey::<10>::try_from_parts(&[&[1, 3][..], &[1, 1, 1]]).unwrap();
            let mut iter = key.section_iter([3]);
            assert_eq!(iter.next(), Some(&[1, 3, 1][..]));
            assert_eq!(iter.next(), None);
            assert_eq!(iter.next(), None);
        }
    }
}
