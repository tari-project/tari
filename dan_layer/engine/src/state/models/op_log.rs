// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use crate::state::{DbStateOpLogEntry, DbStateOperation};

#[derive(Debug)]
pub struct StateOpLogEntry {
    inner: DbStateOpLogEntry,
}

impl StateOpLogEntry {
    pub fn operation(&self) -> StateOperation {
        self.inner.operation.into()
    }

    pub fn into_inner(self) -> DbStateOpLogEntry {
        self.inner
    }
}

impl From<DbStateOpLogEntry> for StateOpLogEntry {
    fn from(inner: DbStateOpLogEntry) -> Self {
        Self { inner }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum StateOperation {
    Set,
    Delete,
}

impl StateOperation {
    pub fn as_op_str(&self) -> &str {
        use StateOperation::{Delete, Set};
        match self {
            Set => "S",
            Delete => "D",
        }
    }
}

impl From<DbStateOperation> for StateOperation {
    fn from(op: DbStateOperation) -> Self {
        match op {
            DbStateOperation::Set => StateOperation::Set,
            DbStateOperation::Delete => StateOperation::Delete,
        }
    }
}
