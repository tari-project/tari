// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use tari_core::base_node::comms_interface::{ValidatorNodeChange, ValidatorNodeChangeState};
use tari_utilities::ByteArray;

impl From<&ValidatorNodeChange> for crate::tari_rpc::ValidatorNodeChange {
    fn from(node_change: &ValidatorNodeChange) -> Self {
        crate::tari_rpc::ValidatorNodeChange {
            public_key: node_change.public_key.to_vec(),
            state: match node_change.state {
                ValidatorNodeChangeState::ADD => crate::tari_rpc::ValidatorNodeChangeState::Add.into(),
                ValidatorNodeChangeState::REMOVE => crate::tari_rpc::ValidatorNodeChangeState::Remove.into(),
            },
            start_height: node_change.height,
            registration: match &node_change.registration {
                Some(value) => Some(crate::tari_rpc::ValidatorNodeRegistration {
                    public_key: value.public_key().to_vec(),
                    signature: Some(crate::tari_rpc::Signature {
                        public_nonce: value.signature().get_public_nonce().to_vec(),
                        signature: value.signature().get_signature().to_vec(),
                    }),
                    claim_public_key: value.claim_public_key().to_vec(),
                    sidechain_id: match value.sidechain_id() {
                        None => vec![],
                        Some(id) => id.to_vec(),
                    },
                    sidechain_id_knowledge_proof: value.sidechain_id_knowledge_proof().map(|signature| {
                        crate::tari_rpc::Signature {
                            public_nonce: signature.get_public_nonce().to_vec(),
                            signature: signature.get_signature().to_vec(),
                        }
                    }),
                }),
                None => None,
            },
            minimum_value_promise: node_change.minimum_value_promise.into(),
        }
    }
}
