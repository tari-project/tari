// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use tari_core::base_node::comms_interface::{ValidatorNodeChange, ValidatorNodeChangeState};
use tari_utilities::ByteArray;

impl From<&ValidatorNodeChange> for crate::tari_rpc::ValidatorNodeChange {
    fn from(node_change: &ValidatorNodeChange) -> Self {
        Self {
            public_key: node_change.public_key.to_vec(),
            state: match node_change.state {
                ValidatorNodeChangeState::ADD => crate::tari_rpc::ValidatorNodeChangeState::Add.into(),
                ValidatorNodeChangeState::REMOVE => crate::tari_rpc::ValidatorNodeChangeState::Remove.into(),
            },
            start_height: node_change.height,
            registration: match &node_change.registration {
                Some(registration) => Some(registration.into()),
                None => None,
            },
            minimum_value_promise: node_change.minimum_value_promise.into(),
        }
    }
}
