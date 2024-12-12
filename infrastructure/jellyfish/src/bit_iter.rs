//   Copyright 2024 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use std::ops::Range;

/// An iterator over a hash value that generates one bit for each iteration.
pub struct BitIterator<'a> {
    /// The reference to the bytes that represent the `HashValue`.
    bytes: &'a [u8],
    pos: Range<usize>,
    // invariant hash_bytes.len() == HashValue::LENGTH;
    // invariant pos.end == hash_bytes.len() * 8;
}

impl<'a> BitIterator<'a> {
    /// Constructs a new `BitIterator` using given `HashValue`.
    pub fn new(bytes: &'a [u8]) -> Self {
        BitIterator {
            bytes,
            pos: 0..bytes.len() * 8,
        }
    }

    /// Returns the `index`-th bit in the bytes.
    fn get_bit(&self, index: usize) -> bool {
        // MIRAI annotations - important?
        // assume!(index < self.pos.end); // assumed precondition
        // assume!(self.hash_bytes.len() == 32); // invariant
        // assume!(self.pos.end == self.hash_bytes.len() * 8); // invariant
        let pos = index / 8;
        let bit = 7 - index % 8;
        (self.bytes[pos] >> bit) & 1 != 0
    }
}

impl<'a> Iterator for BitIterator<'a> {
    type Item = bool;

    fn next(&mut self) -> Option<Self::Item> {
        self.pos.next().map(|x| self.get_bit(x))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.pos.size_hint()
    }
}

impl<'a> DoubleEndedIterator for BitIterator<'a> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.pos.next_back().map(|x| self.get_bit(x))
    }
}

impl<'a> ExactSizeIterator for BitIterator<'a> {}
