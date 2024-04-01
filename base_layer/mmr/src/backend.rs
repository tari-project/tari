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

use std::cmp::min;

use crate::error::MerkleMountainRangeError;

/// A trait describing generic array-like behaviour, without imposing any specific details on how this is actually done.
pub trait ArrayLike {
    type Value;
    type Error: std::error::Error;

    /// Returns the number of hashes stored in the backend
    fn len(&self) -> Result<usize, Self::Error>;

    /// Returns if empty
    fn is_empty(&self) -> Result<bool, Self::Error>;

    /// Store a new item and return the index of the stored item
    fn push(&mut self, item: Self::Value) -> Result<usize, Self::Error>;

    /// Return the item at the given index
    fn get(&self, index: usize) -> Result<Option<Self::Value>, Self::Error>;

    /// Remove all stored items from the the backend.
    fn clear(&mut self) -> Result<(), Self::Error>;

    /// Finds the index of the specified stored item, it will return None if the object could not be found.
    fn position(&self, item: &Self::Value) -> Result<Option<usize>, Self::Error>;
}

pub trait ArrayLikeExt {
    type Value;

    /// Shortens the array, keeping the first len elements and dropping the rest.
    fn truncate(&mut self, _len: usize) -> Result<(), MerkleMountainRangeError>;

    /// Shift the array, by discarding the first n elements from the front.
    fn shift(&mut self, n: usize) -> Result<(), MerkleMountainRangeError>;

    /// Store a new item first in the array, previous items will be shifted up to make room.
    fn push_front(&mut self, item: Self::Value) -> Result<(), MerkleMountainRangeError>;

    /// Execute the given closure for each value in the array
    fn for_each<F>(&self, f: F) -> Result<(), MerkleMountainRangeError>
    where F: FnMut(Result<Self::Value, MerkleMountainRangeError>);
}

impl<T: Clone + PartialEq> ArrayLike for Vec<T> {
    type Error = MerkleMountainRangeError;
    type Value = T;

    fn len(&self) -> Result<usize, Self::Error> {
        Ok(Vec::len(self))
    }

    fn is_empty(&self) -> Result<bool, Self::Error> {
        Ok(Vec::is_empty(self))
    }

    fn push(&mut self, item: Self::Value) -> Result<usize, Self::Error> {
        Vec::push(self, item);
        Ok(self.len() - 1)
    }

    fn get(&self, index: usize) -> Result<Option<Self::Value>, Self::Error> {
        Ok((self as &[Self::Value]).get(index).cloned())
    }

    fn clear(&mut self) -> Result<(), Self::Error> {
        Vec::clear(self);
        Ok(())
    }

    fn position(&self, item: &Self::Value) -> Result<Option<usize>, Self::Error> {
        Ok(self.iter().position(|stored_item| stored_item == item))
    }
}

impl<T: Clone> ArrayLikeExt for Vec<T> {
    type Value = T;

    fn truncate(&mut self, len: usize) -> Result<(), MerkleMountainRangeError> {
        self.truncate(len);
        Ok(())
    }

    fn shift(&mut self, n: usize) -> Result<(), MerkleMountainRangeError> {
        let drain_n = min(n, self.len());
        self.drain(0..drain_n);
        Ok(())
    }

    fn push_front(&mut self, item: Self::Value) -> Result<(), MerkleMountainRangeError> {
        self.insert(0, item);
        Ok(())
    }

    fn for_each<F>(&self, f: F) -> Result<(), MerkleMountainRangeError>
    where F: FnMut(Result<Self::Value, MerkleMountainRangeError>) {
        self.iter().map(|v| Ok(v.clone())).for_each(f);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_empty() {
        let vec = Vec::<u32>::new();
        assert!(<Vec<u32> as ArrayLike>::is_empty(&vec).unwrap());
    }

    #[test]
    fn truncate() {
        let mut vec = vec![1, 2, 3, 4, 5];
        <Vec<u32> as ArrayLikeExt>::truncate(&mut vec, 3).unwrap();
        assert_eq!(vec, vec![1, 2, 3]);
    }

    #[test]
    fn for_each() {
        let vec = vec![1, 2, 3, 4, 5];
        let mut count = 0;
        <Vec<u32> as ArrayLikeExt>::for_each(&vec, |item| {
            count += 1;
            assert_eq!(item.unwrap(), count);
        })
        .unwrap();
    }
}
