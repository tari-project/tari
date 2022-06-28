// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

#[derive(Debug, Clone)]
pub struct DbKeyValue {
    pub schema: String,
    pub key: Vec<u8>,
    pub value: Vec<u8>,
}
