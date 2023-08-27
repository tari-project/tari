// Copyright 2022 The Taiji Project
// SPDX-License-Identifier: BSD-3-Clause

use std::convert::TryFrom;

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, PartialOrd, BorshSerialize, BorshDeserialize)]
#[repr(u8)]
pub enum TransactionKernelVersion {
    V0 = 0,
}

impl TransactionKernelVersion {
    pub fn get_current_version() -> Self {
        Self::V0
    }

    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

impl Default for TransactionKernelVersion {
    fn default() -> Self {
        Self::get_current_version()
    }
}

impl TryFrom<u8> for TransactionKernelVersion {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(TransactionKernelVersion::V0),
            v => Err(format!("Unknown kernel version {}!", v)),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_try_from() {
        assert_eq!(TransactionKernelVersion::try_from(0), Ok(TransactionKernelVersion::V0));
        assert!(TransactionKernelVersion::try_from(1).is_err());
    }
}
