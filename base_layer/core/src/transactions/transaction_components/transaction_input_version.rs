// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::convert::TryFrom;

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, PartialOrd, BorshSerialize, BorshDeserialize)]
#[repr(u8)]
#[borsh(use_discriminant = true)]
pub enum TransactionInputVersion {
    V0 = 0,
    /// Currently only used in tests, this can be used as the next version
    V1 = 1,
}

impl TransactionInputVersion {
    pub fn get_current_version() -> Self {
        Self::V0
    }

    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

impl TryFrom<u8> for TransactionInputVersion {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(TransactionInputVersion::V0),
            1 => Ok(TransactionInputVersion::V1),
            v => Err(format!("Unknown input version {}!", v)),
        }
    }
}

#[cfg(test)]
mod test {

    use super::*;

    #[test]
    fn test_as_u8() {
        assert_eq!(TransactionInputVersion::V0.as_u8(), 0);
        assert_eq!(TransactionInputVersion::V1.as_u8(), 1);
    }

    #[test]
    fn test_try_from() {
        assert_eq!(TransactionInputVersion::try_from(0), Ok(TransactionInputVersion::V0));
        assert_eq!(TransactionInputVersion::try_from(1), Ok(TransactionInputVersion::V1));
        assert!(TransactionInputVersion::try_from(2).is_err());
    }
}
