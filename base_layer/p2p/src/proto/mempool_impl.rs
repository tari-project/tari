// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use crate::proto::{common, mempool, mempool::NewTransaction};

impl From<Vec<u8>> for NewTransaction {
    fn from(tx_bytes: Vec<u8>) -> Self {
        Self {
            payload: Some(mempool::new_transaction::Payload::ExcessSig(tx_bytes)),
        }
    }
}

impl From<common::Transaction> for NewTransaction {
    fn from(tx: common::Transaction) -> Self {
        Self {
            payload: Some(mempool::new_transaction::Payload::Transaction(tx)),
        }
    }
}
