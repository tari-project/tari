use std::convert::TryFrom;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum TransactionInputVersion {
    V1 = 0,
}

impl TransactionInputVersion {
    pub fn get_current_version() -> Self {
        Self::V1
    }
}

impl TryFrom<u8> for TransactionInputVersion {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(TransactionInputVersion::V1),
            _ => Err("Unknown version!".to_string()),
        }
    }
}
