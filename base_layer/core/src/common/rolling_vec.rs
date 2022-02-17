//  Copyright 2020, The Tari Project
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

use std::ops::Deref;

/// A vector that contains up to a number of elements. As new elements are added to the end, the first elements are
/// removed.
#[derive(Debug)]
pub struct RollingVec<T>(Vec<T>);

impl<T> RollingVec<T> {
    pub fn new(capacity: usize) -> Self {
        Self(Vec::with_capacity(capacity))
    }

    /// Adds a new element to the RollingVec.
    /// If adding an element will cause the length to exceed the capacity, the first element is removed.
    pub fn push(&mut self, item: T) {
        if self.capacity() == 0 {
            return;
        }

        if self.is_full() {
            self.inner_mut().remove(0);
        }

        self.inner_mut().push(item);
    }

    pub fn insert(&mut self, index: usize, item: T) {
        assert!(index < self.capacity());
        assert!(index < self.len());

        if self.is_full() {
            self.inner_mut().remove(0);
        }

        self.inner_mut().insert(index, item);
    }

    #[inline]
    pub fn is_full(&self) -> bool {
        // len never exceeds capacity
        debug_assert!(self.inner().len() <= self.inner().capacity());
        self.len() == self.capacity()
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.inner().capacity()
    }

    #[inline]
    fn inner(&self) -> &Vec<T> {
        &self.0
    }

    #[inline]
    fn inner_mut(&mut self) -> &mut Vec<T> {
        &mut self.0
    }
}

impl<T> Extend<T> for RollingVec<T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        let iter = iter.into_iter();
        let (lower, _) = iter.size_hint();

        let skip = if lower > self.capacity() {
            // If the iterator will emit more than the capacity, skip over the first elements that will be pushed out of
            // the rolling window
            lower - self.capacity()
        } else {
            0
        };

        for item in iter.skip(skip) {
            self.push(item);
        }
    }
}

impl<T> Deref for RollingVec<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        self.inner()
    }
}

impl<T: Clone> Clone for RollingVec<T> {
    fn clone(&self) -> Self {
        let mut v = Vec::with_capacity(self.capacity());
        v.extend(self.0.clone());
        Self(v)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn it_is_always_empty_for_zero_capacity() {
        let mut subject = RollingVec::new(0);
        assert!(subject.is_empty());
        subject.push(123);
        assert!(subject.is_empty());
        assert_eq!(subject.len(), 0);
    }

    #[test]
    fn it_is_always_full_for_zero_capacity() {
        let mut subject = RollingVec::new(0);
        assert!(subject.is_full());
        subject.push(123);
        assert!(subject.is_full());
    }

    #[test]
    fn it_is_full_if_n_elements_are_added() {
        let mut subject = RollingVec::new(1);
        assert!(!subject.is_full());
        subject.push(1);
        assert!(subject.is_full());
    }

    #[test]
    fn it_rolls_over_as_elements_are_added() {
        let mut subject = RollingVec::new(1);
        subject.push(1);
        assert_eq!(subject.len(), 1);
        subject.push(2);
        assert_eq!(subject.len(), 1);
        assert_eq!(subject[0], 2);
    }

    #[test]
    fn it_extends_with_less_items_than_capacity() {
        let mut subject = RollingVec::new(5);
        let vec = (0..2).collect::<Vec<_>>();
        subject.extend(vec);

        assert_eq!(subject.len(), 2);
        assert!(!subject.is_full());

        assert_eq!(subject[0], 0);
        assert_eq!(subject[1], 1);
    }

    #[test]
    fn it_extends_without_exceeding_capacity() {
        let mut subject = RollingVec::new(5);
        let vec = (0..10).collect::<Vec<_>>();
        subject.extend(vec);

        assert_eq!(subject.len(), 5);
        assert!(subject.is_full());

        assert_eq!(subject[0], 5);
        assert_eq!(subject[1], 6);
        assert_eq!(subject[2], 7);
        assert_eq!(subject[3], 8);
        assert_eq!(subject[4], 9);
    }
}
