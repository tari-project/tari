// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{
    convert::TryFrom,
    io,
    ops::{Deref, DerefMut},
};

use integer_encoding::VarIntReader;
use serde::{Deserialize, Serialize};

use crate::{ConsensusDecoding, ConsensusEncoding, ConsensusEncodingSized};

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MaxSizeVec<T, const MAX: usize> {
    inner: Vec<T>,
}

impl<T, const MAX: usize> MaxSizeVec<T, MAX> {
    pub fn into_vec(self) -> Vec<T> {
        self.inner
    }

    pub fn try_from_iter<I: IntoIterator<Item = T>>(iter: I) -> Option<Self> {
        let iter = iter.into_iter();
        let (lower, upper) = iter.size_hint();
        if lower > MAX {
            return None;
        }

        let capacity = upper.filter(|u| *u <= MAX).unwrap_or(lower);
        let mut inner = Vec::with_capacity(capacity);
        for item in iter {
            if inner.len() + 1 > MAX {
                return None;
            }
            inner.push(item);
        }
        Some(Self { inner })
    }

    #[must_use = "resulting bool must be checked to ensure that the item was added"]
    pub fn push(&mut self, item: T) -> bool {
        if self.inner.len() + 1 > MAX {
            return false;
        }
        self.inner.push(item);
        true
    }
}

impl<T, const MAX: usize> Deref for MaxSizeVec<T, MAX> {
    type Target = Vec<T>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T, const MAX: usize> DerefMut for MaxSizeVec<T, MAX> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<T, const MAX: usize> From<MaxSizeVec<T, MAX>> for Vec<T> {
    fn from(value: MaxSizeVec<T, MAX>) -> Self {
        value.into_vec()
    }
}

impl<T, const MAX: usize> TryFrom<Vec<T>> for MaxSizeVec<T, MAX> {
    type Error = Vec<T>;

    fn try_from(value: Vec<T>) -> Result<Self, Self::Error> {
        if value.len() > MAX {
            return Err(value);
        }

        Ok(Self { inner: value })
    }
}

impl<T: ConsensusEncoding, const MAX: usize> ConsensusEncoding for MaxSizeVec<T, MAX> {
    fn consensus_encode<W: io::Write>(&self, writer: &mut W) -> Result<(), io::Error> {
        // We do not have to check the number of elements is correct, because it is not possible to construct MaxSizeVec
        // with more than MAX elements.
        self.inner.consensus_encode(writer)
    }
}

impl<T: ConsensusEncoding, const MAX: usize> ConsensusEncodingSized for MaxSizeVec<T, MAX> {}

impl<T: ConsensusDecoding, const MAX: usize> ConsensusDecoding for MaxSizeVec<T, MAX> {
    fn consensus_decode<R: io::Read>(reader: &mut R) -> Result<Self, io::Error> {
        let len = reader.read_varint()?;
        if len > MAX {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Vec size ({}) exceeded maximum ({})", len, MAX),
            ));
        }
        let mut elems = Vec::with_capacity(len);
        for _ in 0..len {
            let elem = T::consensus_decode(reader)?;
            elems.push(elem)
        }
        Ok(Self { inner: elems })
    }
}

impl<T: PartialEq, const MAX: usize> PartialEq<Vec<T>> for MaxSizeVec<T, MAX> {
    fn eq(&self, other: &Vec<T>) -> bool {
        self.inner.eq(other)
    }
}

#[cfg(test)]
mod tests {
    use std::iter;

    use super::*;

    #[test]
    fn try_from_iter() {
        MaxSizeVec::<_, 5>::try_from_iter(iter::repeat(1).take(5)).unwrap();
        assert!(MaxSizeVec::<_, 5>::try_from_iter(iter::repeat(1).take(6)).is_none());
    }
}
