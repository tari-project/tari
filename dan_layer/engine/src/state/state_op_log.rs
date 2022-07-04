// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::str::FromStr;

use tari_common_types::types::FixedHash;

use crate::state::DbKeyValue;

#[derive(Debug)]
pub struct DbStateOpLogEntry {
    pub height: u64,
    pub merkle_root: Option<FixedHash>,
    pub operation: DbStateOperation,
    pub schema: String,
    pub key: Vec<u8>,
    pub value: Option<Vec<u8>>,
}

impl DbStateOpLogEntry {
    pub fn set_operation(height: u64, key_value: DbKeyValue) -> Self {
        Self {
            height,
            merkle_root: None,
            operation: DbStateOperation::Set,
            schema: key_value.schema,
            key: key_value.key,
            value: Some(key_value.value),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum DbStateOperation {
    Set,
    Delete,
}

impl DbStateOperation {
    pub fn as_op_str(&self) -> &str {
        use DbStateOperation::{Delete, Set};
        match self {
            Set => "S",
            Delete => "D",
        }
    }
}

impl FromStr for DbStateOperation {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use DbStateOperation::{Delete, Set};
        match s {
            "S" => Ok(Set),
            "D" => Ok(Delete),
            _ => Err(()),
        }
    }
}
