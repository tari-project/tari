// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::fmt::Display;

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, BorshSerialize, BorshDeserialize)]
pub struct ShardGroup {
    pub start: u32,
    pub end_inclusive: u32,
}

impl Display for ShardGroup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}-{}]", self.start, self.end_inclusive)
    }
}
