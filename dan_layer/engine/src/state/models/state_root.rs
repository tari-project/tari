// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use tari_common_types::types::FixedHash;

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub struct StateRoot {
    root: FixedHash,
}

impl StateRoot {
    pub fn new(root: FixedHash) -> Self {
        Self { root }
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.root.as_slice()
    }

    pub fn initial() -> Self {
        Self {
            root: FixedHash::zero(),
        }
    }
}

impl From<StateRoot> for FixedHash {
    fn from(state_root: StateRoot) -> Self {
        state_root.root
    }
}
