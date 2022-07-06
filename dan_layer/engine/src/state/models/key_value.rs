// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct KeyValue {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
}
