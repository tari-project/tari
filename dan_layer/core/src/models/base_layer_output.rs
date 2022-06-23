//  Copyright 2021. The Tari Project
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

//! A trait to allow abstraction from a specific base layer output
use std::convert::TryFrom;

use tari_common_types::types::{FixedHash, PublicKey};
use tari_core::transactions::transaction_components::{OutputFeatures, OutputType};

use crate::models::ModelError;

#[derive(Debug)]
pub struct BaseLayerOutput {
    pub features: OutputFeatures,
    pub height: u64,
}

impl BaseLayerOutput {
    pub fn get_side_chain_committee(&self) -> Option<&[PublicKey]> {
        self.features
            .constitution_committee()
            .map(|committee| committee.members())
    }

    pub fn get_checkpoint_merkle_root(&self) -> Option<FixedHash> {
        self.features.sidechain_checkpoint.as_ref().map(|cp| cp.merkle_root)
    }

    pub fn get_parent_public_key(&self) -> Option<&PublicKey> {
        self.features.parent_public_key.as_ref()
    }

    pub fn contract_id(&self) -> Option<FixedHash> {
        self.features.contract_id()
    }
}

#[derive(Debug, Clone)]
pub struct CheckpointOutput {
    pub output_type: OutputType,
    pub contract_id: FixedHash,
    pub merkle_root: FixedHash,
}

impl TryFrom<BaseLayerOutput> for CheckpointOutput {
    type Error = ModelError;

    fn try_from(output: BaseLayerOutput) -> Result<Self, Self::Error> {
        if output.features.output_type != OutputType::SidechainCheckpoint {
            return Err(ModelError::NotCheckpointOutput);
        }

        let contract_id = output.contract_id().ok_or(ModelError::OutputMissingParentPublicKey)?;

        let merkle_root = output
            .get_checkpoint_merkle_root()
            .ok_or(ModelError::CheckpointOutputMissingCheckpointMerkleRoot)?;

        Ok(Self {
            output_type: output.features.output_type,
            contract_id,
            merkle_root,
        })
    }
}

#[derive(Debug, Clone)]
pub struct CommitteeOutput {
    pub flags: OutputType,
    pub parent_public_key: PublicKey,
    pub committee: Vec<PublicKey>,
}

impl TryFrom<BaseLayerOutput> for CommitteeOutput {
    type Error = ModelError;

    fn try_from(output: BaseLayerOutput) -> Result<Self, Self::Error> {
        if output.features.output_type != OutputType::CommitteeDefinition {
            return Err(ModelError::NotCommitteeDefinitionOutput);
        }

        let parent_public_key = output
            .get_parent_public_key()
            .cloned()
            .ok_or(ModelError::OutputMissingParentPublicKey)?;

        let committee = output
            .get_side_chain_committee()
            .ok_or(ModelError::CommitteeOutputMissingDefinition)?;

        Ok(Self {
            flags: output.features.output_type,
            parent_public_key,
            committee: committee.into(),
        })
    }
}
