//   Copyright 2020, The Tari Project
//
//   Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//   following conditions are met:
//
//   1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//   disclaimer.
//
//   2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//   following disclaimer in the documentation and/or other materials provided with the distribution.
//
//   3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//   products derived from this software without specific prior written permission.
//
//   THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//   INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//   DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//   SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//   SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//   WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//   USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::cmp;

/// Iterator that produces non-overlapping integer vectors.
/// This is similar to `Vec::chunks` except it does not require a complete vector of integers to produce chunks
pub struct VecChunkIter<Idx> {
    inner: NonOverlappingIntegerPairIter<Idx>,
}

impl<Idx: PartialOrd> VecChunkIter<Idx> {
    pub fn new(start: Idx, end_exclusive: Idx, chunk_size: usize) -> Self {
        Self {
            inner: NonOverlappingIntegerPairIter::new(start, end_exclusive, chunk_size),
        }
    }
}

macro_rules! vec_chunk_impl {
    ($ty:ty) => {
        impl Iterator for VecChunkIter<$ty> {
            type Item = Vec<$ty>;

            fn next(&mut self) -> Option<Self::Item> {
                let (start, end) = self.inner.next()?;
                Some((start..=end).collect())
            }
        }
    };
}

vec_chunk_impl!(u32);
vec_chunk_impl!(u64);
vec_chunk_impl!(usize);

/// Iterator that produces non-overlapping integer pairs.
pub struct NonOverlappingIntegerPairIter<Idx> {
    current: Idx,
    end: Idx,
    size: usize,
}

impl<Idx: PartialOrd> NonOverlappingIntegerPairIter<Idx> {
    pub fn new(start: Idx, end_exclusive: Idx, chunk_size: usize) -> Self {
        assert!(start <= end_exclusive, "`start` must be less than `end`");
        Self {
            current: start,
            end: end_exclusive,
            size: chunk_size,
        }
    }
}

macro_rules! edge_chunk_impl {
    ($ty:ty) => {
        impl Iterator for NonOverlappingIntegerPairIter<$ty> {
            type Item = ($ty, $ty);

            fn next(&mut self) -> Option<Self::Item> {
                if self.size == 0 {
                    return None;
                }

                let next = cmp::min(self.current + self.size as $ty, self.end);

                if self.current == next {
                    return None;
                }
                let chunk = (self.current, next - 1);
                self.current = next;
                Some(chunk)
            }
        }
    };
}

edge_chunk_impl!(u32);
edge_chunk_impl!(u64);
edge_chunk_impl!(usize);

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn zero_size() {
        let mut iter = NonOverlappingIntegerPairIter::new(10u32, 10, 0);
        assert!(iter.next().is_none());
        let mut iter = VecChunkIter::new(10u32, 10, 0);
        assert!(iter.next().is_none());
    }

    #[test]
    fn start_equals_end() {
        let mut iter = NonOverlappingIntegerPairIter::new(10u32, 10, 10);
        assert!(iter.next().is_none());
        let mut iter = VecChunkIter::new(10u32, 10, 10);
        assert!(iter.next().is_none());
    }

    #[test]
    fn chunk_size_multiple_of_end() {
        let mut iter = NonOverlappingIntegerPairIter::new(0u32, 9, 3);
        assert_eq!(iter.next().unwrap(), (0, 2));
        assert_eq!(iter.next().unwrap(), (3, 5));
        assert_eq!(iter.next().unwrap(), (6, 8));
        assert!(iter.next().is_none());

        let mut iter = VecChunkIter::new(0u32, 9, 3);
        assert_eq!(iter.next().unwrap(), vec![0, 1, 2]);
        assert_eq!(iter.next().unwrap(), vec![3, 4, 5]);
        assert_eq!(iter.next().unwrap(), vec![6, 7, 8]);
        assert!(iter.next().is_none());
    }

    #[test]
    fn chunk_size_not_multiple_of_end() {
        let mut iter = NonOverlappingIntegerPairIter::new(0u32, 11, 3);
        assert_eq!(iter.next().unwrap(), (0, 2));
        assert_eq!(iter.next().unwrap(), (3, 5));
        assert_eq!(iter.next().unwrap(), (6, 8));
        assert_eq!(iter.next().unwrap(), (9, 10));
        assert!(iter.next().is_none());

        let mut iter = VecChunkIter::new(0u32, 11, 3);
        assert_eq!(iter.next().unwrap(), vec![0, 1, 2]);
        assert_eq!(iter.next().unwrap(), vec![3, 4, 5]);
        assert_eq!(iter.next().unwrap(), vec![6, 7, 8]);
        assert_eq!(iter.next().unwrap(), vec![9, 10]);
        assert!(iter.next().is_none());
    }

    #[test]
    fn non_zero_start() {
        let mut iter = NonOverlappingIntegerPairIter::new(1001u32, 4000, 1000);
        assert_eq!(iter.next().unwrap(), (1001, 2000));
        assert_eq!(iter.next().unwrap(), (2001, 3000));
        assert_eq!(iter.next().unwrap(), (3001, 3999));
        assert!(iter.next().is_none());

        let mut iter = VecChunkIter::new(10u32, 21, 3);
        assert_eq!(iter.next().unwrap(), vec![10, 11, 12]);
        assert_eq!(iter.next().unwrap(), vec![13, 14, 15]);
        assert_eq!(iter.next().unwrap(), vec![16, 17, 18]);
        assert_eq!(iter.next().unwrap(), vec![19, 20]);
        assert!(iter.next().is_none());
    }
}
