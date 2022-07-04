// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use serde::Deserialize;

#[derive(Debug, Default, Clone, Deserialize)]
pub struct Bucket {
    amount: u64,
    token_id: u64,
    asset_id: u64,
}

impl Bucket {
    pub fn new(amount: u64, token_id: u64, asset_id: u64) -> Self {
        Self {
            amount,
            token_id,
            asset_id,
        }
    }

    pub fn amount(&self) -> u64 {
        self.amount
    }

    pub fn token_id(&self) -> u64 {
        self.token_id
    }

    pub fn asset_id(&self) -> u64 {
        self.asset_id
    }
}
