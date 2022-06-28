// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use serde::{Deserialize, Serialize};

use crate::state::models::KeyValue;

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct SchemaState {
    pub name: String,
    pub items: Vec<KeyValue>,
}

impl SchemaState {
    pub fn new(name: String, items: Vec<KeyValue>) -> Self {
        Self { name, items }
    }

    pub fn push_key_value(&mut self, key_value: KeyValue) -> &mut Self {
        self.items.push(key_value);
        self
    }
}
