// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use crate::MicroMinotari;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeePerGramStat {
    pub order: u64,
    pub min_fee_per_gram: MicroMinotari,
    pub avg_fee_per_gram: MicroMinotari,
    pub max_fee_per_gram: MicroMinotari,
}
