// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use tari_core::transactions::transaction_components::ValidatorNodeRegistration;
use tari_utilities::ByteArray;

impl From<&ValidatorNodeRegistration> for crate::tari_rpc::ValidatorNodeRegistration {
    fn from(registration: &ValidatorNodeRegistration) -> Self {
        Self {
            public_key: registration.public_key().to_vec(),
            signature: Some(crate::tari_rpc::Signature {
                public_nonce: registration.signature().get_public_nonce().to_vec(),
                signature: registration.signature().get_signature().to_vec(),
            }),
            claim_public_key: registration.claim_public_key().to_vec(),
            sidechain_id: match registration.sidechain_id() {
                None => vec![],
                Some(id) => id.to_vec(),
            },
            sidechain_id_knowledge_proof: registration.sidechain_id_knowledge_proof().map(|signature| {
                crate::tari_rpc::Signature {
                    public_nonce: signature.get_public_nonce().to_vec(),
                    signature: signature.get_signature().to_vec(),
                }
            }),
        }
    }
}
