//   Copyright 2020, The Taiji Project
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

impl<Idx: PartialOrd + Copy> VecChunkIter<Idx> {
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
    current_end: Idx,
    end: Idx,
    size: usize,
}

impl<Idx: PartialOrd + Copy> NonOverlappingIntegerPairIter<Idx> {
    /// Create a new iterator that emits non-overlapping integers.
    ///
    /// ## Panics
    /// Panics if start > end_exclusive
    pub fn new(start: Idx, end_exclusive: Idx, chunk_size: usize) -> Self {
        assert!(start <= end_exclusive, "`start` must be less than `end`");
        Self {
            current: start,
            current_end: end_exclusive,
            end: end_exclusive,
            size: chunk_size,
        }
    }
}

macro_rules! non_overlapping_iter_impl {
    ($ty:ty) => {
        impl Iterator for NonOverlappingIntegerPairIter<$ty> {
            type Item = ($ty, $ty);

            fn next(&mut self) -> Option<Self::Item> {
                if self.size == 0 {
                    return None;
                }
                if self.current == <$ty>::MAX {
                    return None;
                }
                if self.current == self.end {
                    return None;
                }
                let size = self.size as $ty;
                match self.current.checked_add(size) {
                    Some(next) => {
                        let next = cmp::min(next, self.end);

                        if self.current == next {
                            return None;
                        }
                        let chunk = (self.current, next - 1);
                        self.current = next;
                        Some(chunk)
                    },
                    None => {
                        let chunk = (self.current, <$ty>::MAX - 1);
                        self.current = <$ty>::MAX;
                        Some(chunk)
                    },
                }
            }
        }
        impl DoubleEndedIterator for NonOverlappingIntegerPairIter<$ty> {
            fn next_back(&mut self) -> Option<Self::Item> {
                if self.size == 0 || self.current_end == 0 {
                    return None;
                }
                // Check if end will go beyond start
                if self.current_end == self.current {
                    return None;
                }

                let size = self.size as $ty;
                // Is this the first iteration?
                if self.end == self.current_end {
                    let rem = (self.end - self.current) % size;

                    // Would there be an overflow (if iterating from the forward to back)
                    if rem > 0 && self.current_end.saturating_sub(rem).checked_add(size).is_none() {
                        self.current_end = self.current_end.saturating_sub(rem);
                        let chunk = (self.current_end, <$ty>::MAX - 1);
                        return Some(chunk);
                    }

                    if rem > 0 {
                        self.current_end = self.end - rem;
                        let chunk = (self.current_end, self.end - 1);
                        return Some(chunk);
                    }
                }

                let next = self.current_end.saturating_sub(size);
                let chunk = (next, self.current_end - 1);
                self.current_end = next;
                Some(chunk)
            }
        }
    };
}

non_overlapping_iter_impl!(u8);
non_overlapping_iter_impl!(u16);
non_overlapping_iter_impl!(u32);
non_overlapping_iter_impl!(u64);
non_overlapping_iter_impl!(usize);

#[cfg(test)]
mod test {
    use rand::{rngs::OsRng, Rng};

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
        let mut iter = NonOverlappingIntegerPairIter::new(0u32, 10, 3);
        assert_eq!(iter.next().unwrap(), (0, 2));
        assert_eq!(iter.next().unwrap(), (3, 5));
        assert_eq!(iter.next().unwrap(), (6, 8));
        assert_eq!(iter.next().unwrap(), (9, 9));
        assert!(iter.next().is_none());

        let mut iter = VecChunkIter::new(0u32, 10, 3);
        assert_eq!(iter.next().unwrap(), vec![0, 1, 2]);
        assert_eq!(iter.next().unwrap(), vec![3, 4, 5]);
        assert_eq!(iter.next().unwrap(), vec![6, 7, 8]);
        assert_eq!(iter.next().unwrap(), vec![9]);
        assert!(iter.next().is_none());

        let mut iter = NonOverlappingIntegerPairIter::new(0u32, 16, 5);
        assert_eq!(iter.next().unwrap(), (0, 4));
        assert_eq!(iter.next().unwrap(), (5, 9));
        assert_eq!(iter.next().unwrap(), (10, 14));
        assert_eq!(iter.next().unwrap(), (15, 15));
        assert_eq!(iter.next(), None);
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

    #[test]
    fn overflow() {
        let mut iter = NonOverlappingIntegerPairIter::new(250u8, 255, 3);
        assert_eq!(iter.next().unwrap(), (250, 252));
        assert_eq!(iter.next().unwrap(), (253, 254));
        assert!(iter.next().is_none());
    }

    #[test]
    fn double_ended() {
        let mut iter = NonOverlappingIntegerPairIter::new(0u32, 9, 3).rev();
        assert_eq!(iter.next().unwrap(), (6, 8));
        assert_eq!(iter.next().unwrap(), (3, 5));
        assert_eq!(iter.next().unwrap(), (0, 2));
        assert!(iter.next().is_none());

        let mut iter = NonOverlappingIntegerPairIter::new(0u32, 10, 3).rev();
        assert_eq!(iter.next().unwrap(), (9, 9));
        assert_eq!(iter.next().unwrap(), (6, 8));
        assert_eq!(iter.next().unwrap(), (3, 5));
        assert_eq!(iter.next().unwrap(), (0, 2));
        assert!(iter.next().is_none());

        let mut iter = NonOverlappingIntegerPairIter::new(0u32, 16, 5).rev();
        assert_eq!(iter.next().unwrap(), (15, 15));
        assert_eq!(iter.next().unwrap(), (10, 14));
        assert_eq!(iter.next().unwrap(), (5, 9));
        assert_eq!(iter.next().unwrap(), (0, 4));
        assert!(iter.next().is_none());

        let mut iter = NonOverlappingIntegerPairIter::new(1001u32, 4000, 1000).rev();
        assert_eq!(iter.next().unwrap(), (3001, 3999));
        assert_eq!(iter.next().unwrap(), (2001, 3000));
        assert_eq!(iter.next().unwrap(), (1001, 2000));
        assert!(iter.next().is_none());

        let mut iter = NonOverlappingIntegerPairIter::new(254u8, u8::MAX, 1000).rev();
        assert_eq!(iter.next().unwrap(), (254, 254));
        assert!(iter.next().is_none());

        let mut iter = NonOverlappingIntegerPairIter::new(87u8, u8::MAX, 6).rev();
        assert_eq!(iter.next().unwrap(), (249, 254));
        assert_eq!(iter.next().unwrap(), (243, 248));
        for _ in 0..((255 - 87) / 6) - 2 {
            assert!(iter.next().is_some());
        }
        assert!(iter.next().is_none());

        let mut iter = NonOverlappingIntegerPairIter::new(255u8, u8::MAX, 1000).rev();
        assert!(iter.next().is_none());
    }

    #[test]
    fn iterator_symmetry() {
        let size = OsRng.gen_range(3usize..=10);
        let rand_start = OsRng.gen::<u8>();
        let rand_end = OsRng.gen::<u8>().saturating_add(rand_start);

        // If the iterator never ends, we have the params used
        eprintln!(
            "iterator_symmetry: rand_start = {}, rand_end = {}, size = {}",
            rand_start, rand_end, size
        );
        let iter_rev = NonOverlappingIntegerPairIter::<u8>::new(rand_start, rand_end, size).rev();
        let iter = NonOverlappingIntegerPairIter::<u8>::new(rand_start, rand_end, size);

        let collect1 = iter.take(1000).collect::<Vec<_>>();
        let collect2 = iter_rev
            .take(1000)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>();
        assert_eq!(
            collect1, collect2,
            "Failed with rand_start = {}, rand_end = {}, size = {}",
            rand_start, rand_end, size
        );
    }
}
