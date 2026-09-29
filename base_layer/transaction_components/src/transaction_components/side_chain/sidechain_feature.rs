//  Copyright 2022. The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use tari_common_types::types::{CompressedPublicKey, CompressedSignature, PrivateKey};
use tari_crypto::ristretto::{CompressedRistrettoSchnorr, RistrettoSchnorr};

use crate::transaction_components::{
    CodeTemplateRegistration,
    ValidatorNodeRegistration,
    side_chain::{confidential_output::ConfidentialOutputData, validator_node_exit::ValidatorNodeExit},
};
// NOTE: tari_mining_helper_ffi makes use of borsh encoding (not serde/bincode), therefore we need to
// implement BorshDeserialize on all types

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, BorshSerialize, BorshDeserialize)]
pub struct SideChainFeature {
    pub data: SideChainFeatureData,
    pub sidechain_id: Option<SideChainId>,
}

impl SideChainFeature {
    pub fn is_sidechain_id_valid(&self) -> bool {
        let Some(sidechain_id) = self.sidechain_id.as_ref() else {
            return true;
        };

        match &self.data {
            SideChainFeatureData::ValidatorNodeRegistration(reg) => sidechain_id.is_valid(reg.sidechain_id_message()),
            SideChainFeatureData::CodeTemplateRegistration(reg) => sidechain_id.is_valid(reg.sidechain_id_message()),
            SideChainFeatureData::ConfidentialOutput(output) => sidechain_id.is_valid(output.sidechain_id_message()),
            SideChainFeatureData::ValidatorNodeExit(exit) => sidechain_id.is_valid(exit.sidechain_id_message()),
        }
    }

    pub fn data(&self) -> &SideChainFeatureData {
        &self.data
    }

    pub fn sidechain_id(&self) -> Option<&SideChainId> {
        self.sidechain_id.as_ref()
    }

    pub fn sidechain_public_key(&self) -> Option<&CompressedPublicKey> {
        self.sidechain_id.as_ref().map(|id| id.public_key())
    }

    pub fn code_template_registration(&self) -> Option<&CodeTemplateRegistration> {
        match &self.data {
            SideChainFeatureData::CodeTemplateRegistration(v) => Some(v),
            _ => None,
        }
    }

    pub fn validator_node_registration(&self) -> Option<&ValidatorNodeRegistration> {
        match &self.data {
            SideChainFeatureData::ValidatorNodeRegistration(v) => Some(v),
            _ => None,
        }
    }

    pub fn validator_node_exit(&self) -> Option<&ValidatorNodeExit> {
        match &self.data {
            SideChainFeatureData::ValidatorNodeExit(v) => Some(v),
            _ => None,
        }
    }

    pub fn confidential_output_data(&self) -> Option<&ConfidentialOutputData> {
        match &self.data {
            SideChainFeatureData::ConfidentialOutput(v) => Some(v),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, BorshSerialize, BorshDeserialize)]
pub enum SideChainFeatureData {
    ValidatorNodeRegistration(Box<ValidatorNodeRegistration>),
    CodeTemplateRegistration(CodeTemplateRegistration),
    ConfidentialOutput(ConfidentialOutputData),
    ValidatorNodeExit(ValidatorNodeExit),
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, BorshSerialize, BorshDeserialize)]
pub struct SideChainId {
    public_key: CompressedPublicKey,
    knowledge_proof: CompressedSignature,
}

impl SideChainId {
    pub fn new(public_key: CompressedPublicKey, knowledge_proof: CompressedSignature) -> Self {
        Self {
            public_key,
            knowledge_proof,
        }
    }

    pub fn public_key(&self) -> &CompressedPublicKey {
        &self.public_key
    }

    pub fn knowledge_proof(&self) -> &CompressedSignature {
        &self.knowledge_proof
    }

    pub fn sign<T: AsRef<[u8]>>(private_key: &PrivateKey, message: T) -> Self {
        let public_key = CompressedPublicKey::from_secret_key(private_key);
        Self {
            public_key,
            knowledge_proof: CompressedRistrettoSchnorr::new_from_schnorr(
                RistrettoSchnorr::sign(private_key, message, &mut rand::rng())
                    .expect("RistrettoSchnorr::sign is completely infallible"),
            ),
        }
    }

    pub fn is_valid<T: AsRef<[u8]>>(&self, message: T) -> bool {
        let Ok(signature) = self.knowledge_proof.to_schnorr_signature() else {
            return false;
        };

        let Ok(public_key) = self.public_key.to_public_key() else {
            return false;
        };

        signature.verify(&public_key, message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A burn output may carry side-chain data on any network, so the index of every variant a live network can
    /// hold is part of the output hash (Borsh) and the stored output (bincode). Only `ConfidentialOutput` can occur
    /// on a live network - validator registration is disabled there, so no exit can be valid - but the variants
    /// before it must keep their indexes for it to keep its own.
    #[test]
    fn the_confidential_output_variant_index_is_stable() {
        let data = SideChainFeatureData::ConfidentialOutput(ConfidentialOutputData {
            claim_public_key: CompressedPublicKey::default(),
        });

        let borsh_bytes = borsh::to_vec(&data).unwrap();
        assert_eq!(borsh_bytes.first(), Some(&2));
        assert_eq!(SideChainFeatureData::try_from_slice(&borsh_bytes).unwrap(), data);

        let bincode_bytes = bincode::serialize(&data).unwrap();
        assert_eq!(bincode_bytes.get(..4), Some(2u32.to_le_bytes().as_slice()));
        assert_eq!(
            bincode::deserialize::<SideChainFeatureData>(&bincode_bytes).unwrap(),
            data
        );
    }
}
