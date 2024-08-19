// Copyright 2022 The Tari Project
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
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE

use std::{
    convert::TryFrom,
    iter::FromIterator,
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

/// A vector that has a maximum size of `MAX_SIZE`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize, BorshSerialize, BorshDeserialize)]
pub struct MaxSizeVec<T, const MAX_SIZE: usize> {
    vec: Vec<T>,
    _marker: PhantomData<T>,
}

impl<T, const MAX_SIZE: usize> Default for MaxSizeVec<T, MAX_SIZE> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, const MAX_SIZE: usize> MaxSizeVec<T, MAX_SIZE> {
    /// Creates a new `MaxSizeVec` with a capacity of `MAX_SIZE`.
    pub fn new() -> Self {
        Self {
            vec: Vec::with_capacity(MAX_SIZE),
            _marker: PhantomData,
        }
    }

    /// Creates a new `MaxSizeVec` with the given data.
    /// Returns an error if the data length exceeds `MAX_SIZE`.
    pub fn new_with_data(data: Vec<T>) -> Result<Self, MaxSizeVecError> {
        if data.len() > MAX_SIZE {
            Err(MaxSizeVecError::MaxSizeVecLengthError {
                expected: MAX_SIZE,
                actual: data.len(),
            })
        } else {
            Ok(Self {
                vec: data,
                _marker: PhantomData,
            })
        }
    }

    /// Consumes the `MaxSizeVec` and returns the inner `Vec<T>`.
    pub fn into_vec(self) -> Vec<T> {
        self.vec
    }

    /// Creates a `MaxSizeVec` from the given items.
    /// Returns `None` if the items length exceeds `MAX_SIZE`.
    pub fn from_items_checked(items: Vec<T>) -> Option<Self> {
        if items.len() > MAX_SIZE {
            None
        } else {
            Some(Self {
                vec: items,
                _marker: PhantomData,
            })
        }
    }

    /// Creates a `MaxSizeVec` from the given items, truncating if necessary.
    pub fn from_items_truncate(items: Vec<T>) -> Self {
        let len = std::cmp::min(items.len(), MAX_SIZE);
        Self {
            vec: items.into_iter().take(len).collect(),
            _marker: PhantomData,
        }
    }

    /// Returns the maximum size of the `MaxSizeVec`.
    pub fn max_size(&self) -> usize {
        MAX_SIZE
    }

    /// Pushes an item to the `MaxSizeVec`.
    pub fn push(&mut self, item: T) -> Result<(), MaxSizeVecError> {
        if self.vec.len() >= MAX_SIZE {
            return Err(MaxSizeVecError::MaxSizeVecLengthError {
                expected: MAX_SIZE,
                actual: self.vec.len(),
            });
        }
        self.vec.push(item);
        Ok(())
    }
}

impl<T, const MAX_SIZE: usize> From<MaxSizeVec<T, MAX_SIZE>> for Vec<T> {
    /// Converts a `MaxSizeVec` into a `Vec<T>`.
    fn from(value: MaxSizeVec<T, MAX_SIZE>) -> Self {
        value.vec
    }
}

impl<T, const MAX_SIZE: usize> TryFrom<Vec<T>> for MaxSizeVec<T, MAX_SIZE> {
    type Error = MaxSizeVecError;

    /// Tries to convert a `Vec<T>` into a `MaxSizeVec`.
    /// Returns an error if the length of the vector exceeds `MAX_SIZE`.
    fn try_from(value: Vec<T>) -> Result<Self, Self::Error> {
        if value.len() > MAX_SIZE {
            Err(MaxSizeVecError::MaxSizeVecLengthError {
                expected: MAX_SIZE,
                actual: value.len(),
            })
        } else {
            Ok(Self {
                vec: value,
                _marker: PhantomData,
            })
        }
    }
}

impl<T, const MAX_SIZE: usize> AsRef<[T]> for MaxSizeVec<T, MAX_SIZE> {
    /// Returns a reference to the inner slice of the `MaxSizeVec`.
    fn as_ref(&self) -> &[T] {
        &self.vec
    }
}

impl<T, const MAX_SIZE: usize> Deref for MaxSizeVec<T, MAX_SIZE> {
    type Target = [T];

    /// Dereferences the `MaxSizeVec` to a slice.
    fn deref(&self) -> &Self::Target {
        &self.vec
    }
}

impl<T, const MAX_SIZE: usize> DerefMut for MaxSizeVec<T, MAX_SIZE> {
    /// Mutably dereferences the `MaxSizeVec` to a slice.
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.vec
    }
}

impl<T, const MAX_SIZE: usize> Iterator for MaxSizeVec<T, MAX_SIZE> {
    type Item = T;

    /// Iterates over the `MaxSizeVec`.
    fn next(&mut self) -> Option<Self::Item> {
        if self.vec.is_empty() {
            None
        } else {
            Some(self.vec.remove(0))
        }
    }
}

impl<T, const MAX_SIZE: usize> FromIterator<T> for MaxSizeVec<T, MAX_SIZE> {
    /// Creates a `MaxSizeVec` from an iterator.
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut vec = Vec::with_capacity(MAX_SIZE);
        for item in iter {
            if vec.len() >= MAX_SIZE {
                break;
            }
            vec.push(item);
        }
        Self {
            vec,
            _marker: PhantomData,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MaxSizeVecError {
    #[error("Invalid vector length: expected {expected}, got {actual}")]
    MaxSizeVecLengthError { expected: usize, actual: usize },
}
