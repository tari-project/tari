// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{
    fmt::{Display, Formatter},
    ops::{Deref, DerefMut},
};

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, BorshSerialize, BorshDeserialize, Serialize, Deserialize,
)]
pub struct TreeHash([u8; 32]);

impl TreeHash {
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub const fn zero() -> Self {
        Self([0; 32])
    }

    pub const fn into_array(self) -> [u8; 32] {
        self.0
    }

    pub fn try_from_bytes(bytes: &[u8]) -> Result<Self, TreeHashSizeError> {
        if bytes.len() != 32 {
            return Err(TreeHashSizeError);
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(bytes);
        Ok(Self(arr))
    }
}

impl From<[u8; 32]> for TreeHash {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl Deref for TreeHash {
    type Target = [u8; 32];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for TreeHash {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T: AsRef<[u8]>> PartialEq<T> for TreeHash {
    fn eq(&self, other: &T) -> bool {
        self.0 == other.as_ref()
    }
}

impl Display for TreeHash {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        for b in self.0 {
            write!(f, "{:02x}", b)?;
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Invalid TreeHash byte size. Must be 32 bytes.")]
pub struct TreeHashSizeError;
