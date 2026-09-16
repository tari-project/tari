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

use borsh::{
    BorshDeserialize,
    BorshSerialize,
    error::ERROR_ZST_FORBIDDEN,
    io::{Error, ErrorKind},
};
use serde::{Deserialize, Deserializer, Serialize};

use crate::checked_de::{cautious_capacity, read_checked_len};

/// A vector that has a maximum size of `MAX_SIZE`.
///
/// The bound is enforced by every constructor *and* by deserialization (see the hand written
/// `BorshDeserialize`/`Deserialize` implementations below), so `len() <= MAX_SIZE` is a true
/// invariant even for values decoded from untrusted input.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, BorshSerialize)]
pub struct MaxSizeVec<T, const MAX_SIZE: usize> {
    vec: Vec<T>,
    _marker: PhantomData<T>,
}

/// Mirror of [`MaxSizeVec`] used only to decode the wire format before the bound is checked.
/// It must keep the exact same (serde) shape as `MaxSizeVec` so that the serialized
/// representation is unchanged.
#[derive(Deserialize)]
#[serde(rename = "MaxSizeVec")]
struct MaxSizeVecShadow<T> {
    vec: Vec<T>,
    _marker: PhantomData<T>,
}

impl<'de, T: Deserialize<'de>, const MAX_SIZE: usize> Deserialize<'de> for MaxSizeVec<T, MAX_SIZE> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let shadow = MaxSizeVecShadow::<T>::deserialize(deserializer)?;
        Self::try_from(shadow.vec).map_err(serde::de::Error::custom)
    }
}

impl<T: BorshDeserialize, const MAX_SIZE: usize> BorshDeserialize for MaxSizeVec<T, MAX_SIZE> {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> borsh::io::Result<Self> {
        // Matches borsh's own `Vec<T>` implementation, which refuses zero sized types.
        if size_of::<T>() == 0 {
            return Err(Error::new(ErrorKind::InvalidData, ERROR_ZST_FORBIDDEN));
        }
        // The length is validated before any element is read, so an oversized payload is rejected
        // up front instead of being decoded and silently accepted.
        let len = read_checked_len(reader, MAX_SIZE, "MaxSizeVec")?;
        let mut vec = Vec::with_capacity(cautious_capacity::<T>(len));
        for _ in 0..len {
            vec.push(T::deserialize_reader(reader)?);
        }
        Ok(Self {
            vec,
            _marker: PhantomData,
        })
    }
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
            vec: Vec::new(),
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
        let mut vec = Vec::new();
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

#[derive(Clone, Debug, thiserror::Error, Eq, PartialEq, Serialize, Deserialize)]
pub enum MaxSizeVecError {
    #[error("Invalid vector length: expected {expected}, got {actual}")]
    MaxSizeVecLengthError { expected: usize, actual: usize },
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAX: usize = 10;
    type Vec32 = MaxSizeVec<u32, MAX>;

    #[test]
    fn borsh_round_trips_a_valid_value() {
        let v = Vec32::try_from(vec![1u32, 2, 3]).unwrap();
        let encoded = borsh::to_vec(&v).unwrap();
        assert_eq!(Vec32::try_from_slice(&encoded).unwrap(), v);

        // The encoding is unchanged from the derived implementation, i.e. it is the plain borsh
        // encoding of the inner `Vec<T>` (`PhantomData` encodes to nothing)
        assert_eq!(encoded, borsh::to_vec(&vec![1u32, 2, 3]).unwrap());
    }

    #[test]
    fn borsh_accepts_exactly_max_and_rejects_max_plus_one() {
        let at_max = borsh::to_vec(&vec![0u32; MAX]).unwrap();
        assert_eq!(Vec32::try_from_slice(&at_max).unwrap().len(), MAX);

        let over_max = borsh::to_vec(&vec![0u32; MAX + 1]).unwrap();
        let err = Vec32::try_from_slice(&over_max).unwrap_err();
        assert!(err.to_string().contains("exceeds the maximum size"), "{}", err);
    }

    #[test]
    fn borsh_rejects_an_oversized_length_prefix_without_reading_the_body() {
        // A length prefix of 4 Gi elements and no data at all: this must fail on the length check
        // alone
        let payload = u32::MAX.to_le_bytes();
        let err = Vec32::try_from_slice(&payload).unwrap_err();
        assert!(err.to_string().contains("exceeds the maximum size"), "{}", err);
    }

    #[test]
    fn borsh_rejects_a_truncated_body() {
        let mut payload = borsh::to_vec(&vec![0u32; MAX]).unwrap();
        payload.pop();
        assert!(Vec32::try_from_slice(&payload).is_err());
    }

    #[test]
    fn borsh_rejects_zero_sized_types() {
        let payload = borsh::to_vec(&1u32).unwrap();
        assert!(MaxSizeVec::<(), MAX>::try_from_slice(&payload).is_err());
    }

    #[test]
    fn serde_round_trips_a_valid_value_without_changing_the_representation() {
        let v = Vec32::try_from(vec![1u32, 2, 3]).unwrap();
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, r#"{"vec":[1,2,3],"_marker":null}"#);
        assert_eq!(serde_json::from_str::<Vec32>(&json).unwrap(), v);
    }

    #[test]
    fn bincode_round_trips_a_valid_value_and_rejects_max_plus_one() {
        // bincode is the compact (non human readable) serde format used for the on-disk chain
        // storage, so the encoding must be unchanged
        let v = Vec32::try_from(vec![1u32, 2, 3]).unwrap();
        let encoded = bincode::serialize(&v).unwrap();
        assert_eq!(encoded, bincode::serialize(&vec![1u32, 2, 3]).unwrap());
        assert_eq!(bincode::deserialize::<Vec32>(&encoded).unwrap(), v);

        let at_max = bincode::serialize(&vec![0u32; MAX]).unwrap();
        assert_eq!(bincode::deserialize::<Vec32>(&at_max).unwrap().len(), MAX);

        let over_max = bincode::serialize(&vec![0u32; MAX + 1]).unwrap();
        let err = bincode::deserialize::<Vec32>(&over_max).unwrap_err();
        assert!(err.to_string().contains("Invalid vector length"), "{}", err);
    }

    #[test]
    fn serde_accepts_exactly_max_and_rejects_max_plus_one() {
        let at_max = serde_json::to_string(&vec![0u32; MAX]).unwrap();
        let at_max = format!(r#"{{"vec":{at_max},"_marker":null}}"#);
        assert_eq!(serde_json::from_str::<Vec32>(&at_max).unwrap().len(), MAX);

        let over_max = serde_json::to_string(&vec![0u32; MAX + 1]).unwrap();
        let over_max = format!(r#"{{"vec":{over_max},"_marker":null}}"#);
        let err = serde_json::from_str::<Vec32>(&over_max).unwrap_err();
        assert!(err.to_string().contains("Invalid vector length"), "{}", err);
    }
}
