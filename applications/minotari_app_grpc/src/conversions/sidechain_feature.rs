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

use std::convert::{TryFrom, TryInto};

use tari_common_types::types::{PublicKey, Signature};
use tari_core::transactions::transaction_components::{
    BuildInfo,
    CodeTemplateRegistration,
    ConfidentialOutputData,
    SideChainFeature,
    TemplateType,
    ValidatorNodeRegistration,
    ValidatorNodeSignature,
};
use tari_max_size::MaxSizeString;
use tari_utilities::ByteArray;

use crate::tari_rpc as grpc;

//---------------------------------- SideChainFeature --------------------------------------------//
impl From<SideChainFeature> for grpc::SideChainFeature {
    fn from(value: SideChainFeature) -> Self {
        Self {
            side_chain_feature: Some(value.into()),
        }
    }
}

impl From<SideChainFeature> for grpc::side_chain_feature::SideChainFeature {
    fn from(value: SideChainFeature) -> Self {
        match value {
            SideChainFeature::ValidatorNodeRegistration(template_reg) => {
                grpc::side_chain_feature::SideChainFeature::ValidatorNodeRegistration(template_reg.into())
            },
            SideChainFeature::CodeTemplateRegistration(template_reg) => {
                grpc::side_chain_feature::SideChainFeature::TemplateRegistration(template_reg.into())
            },
            SideChainFeature::ConfidentialOutput(output_data) => {
                grpc::side_chain_feature::SideChainFeature::ConfidentialOutput(output_data.into())
            },
        }
    }
}

impl TryFrom<grpc::side_chain_feature::SideChainFeature> for SideChainFeature {
    type Error = String;

    fn try_from(features: grpc::side_chain_feature::SideChainFeature) -> Result<Self, Self::Error> {
        match features {
            grpc::side_chain_feature::SideChainFeature::ValidatorNodeRegistration(vn_reg) => {
                Ok(SideChainFeature::ValidatorNodeRegistration(vn_reg.try_into()?))
            },
            grpc::side_chain_feature::SideChainFeature::TemplateRegistration(template_reg) => {
                Ok(SideChainFeature::CodeTemplateRegistration(template_reg.try_into()?))
            },
            grpc::side_chain_feature::SideChainFeature::ConfidentialOutput(output_data) => {
                Ok(SideChainFeature::ConfidentialOutput(output_data.try_into()?))
            },
        }
    }
}

// -------------------------------- ValidatorNodeRegistration -------------------------------- //
impl TryFrom<grpc::ValidatorNodeRegistration> for ValidatorNodeRegistration {
    type Error = String;

    fn try_from(value: grpc::ValidatorNodeRegistration) -> Result<Self, Self::Error> {
        let public_key =
            PublicKey::from_canonical_bytes(&value.public_key).map_err(|e| format!("Invalid public key: {}", e))?;
        let claim_public_key = PublicKey::from_canonical_bytes(&value.claim_public_key)
            .map_err(|e| format!("Invalid claim public key: {}", e))?;

        let sidechain_id = if value.sidechain_id.is_empty() {
            None
        } else {
            Some(PublicKey::from_canonical_bytes(&value.sidechain_id).map_err(|e| format!("sidechain_id: {}", e))?)
        };
        let sidechain_id_knowledge_proof = value
            .sidechain_id_knowledge_proof
            .map(|v| Signature::try_from(v).map_err(|e| format!("sidechain_id_knowledge_proof: {}", e)))
            .transpose()?;

        Ok(ValidatorNodeRegistration::new(
            ValidatorNodeSignature::new(
                public_key,
                value
                    .signature
                    .map(Signature::try_from)
                    .ok_or("signature not provided")??,
            ),
            claim_public_key,
            sidechain_id,
            sidechain_id_knowledge_proof,
        ))
    }
}

impl From<ValidatorNodeRegistration> for grpc::ValidatorNodeRegistration {
    fn from(value: ValidatorNodeRegistration) -> Self {
        Self {
            public_key: value.public_key().to_vec(),
            signature: Some(value.signature().into()),
            claim_public_key: value.claim_public_key().to_vec(),
            sidechain_id: value.sidechain_id().map(|v| v.to_vec()).unwrap_or_default(),
            sidechain_id_knowledge_proof: value.sidechain_id_knowledge_proof().map(|v| v.into()),
        }
    }
}

// -------------------------------- TemplateRegistration -------------------------------- //
impl TryFrom<grpc::TemplateRegistration> for CodeTemplateRegistration {
    type Error = String;

    fn try_from(value: grpc::TemplateRegistration) -> Result<Self, Self::Error> {
        let sidechain_id = if value.sidechain_id.is_empty() {
            None
        } else {
            Some(PublicKey::from_canonical_bytes(&value.sidechain_id).map_err(|e| format!("sidechain_id: {}", e))?)
        };
        let sidechain_id_knowledge_proof = value
            .sidechain_id_knowledge_proof
            .map(|v| Signature::try_from(v).map_err(|e| format!("sidechain_id_knowledge_proof: {}", e)))
            .transpose()?;
        Ok(Self {
            author_public_key: PublicKey::from_canonical_bytes(&value.author_public_key).map_err(|e| e.to_string())?,
            author_signature: value
                .author_signature
                .map(Signature::try_from)
                .ok_or("author_signature not provided")??,
            template_name: MaxSizeString::try_from(value.template_name).map_err(|e| e.to_string())?,
            template_version: value
                .template_version
                .try_into()
                .map_err(|_| "Invalid template version")?,
            template_type: value
                .template_type
                .map(TryFrom::try_from)
                .ok_or("Template type not provided")??,
            build_info: value
                .build_info
                .map(TryFrom::try_from)
                .ok_or("Build info not provided")??,
            binary_sha: value.binary_sha.try_into().map_err(|_| "Invalid commit sha")?,
            binary_url: MaxSizeString::try_from(value.binary_url).map_err(|e| e.to_string())?,
            sidechain_id,
            sidechain_id_knowledge_proof,
        })
    }
}

impl From<CodeTemplateRegistration> for grpc::TemplateRegistration {
    fn from(value: CodeTemplateRegistration) -> Self {
        Self {
            author_public_key: value.author_public_key.to_vec(),
            author_signature: Some(value.author_signature.into()),
            template_name: value.template_name.to_string(),
            template_version: u32::from(value.template_version),
            template_type: Some(value.template_type.into()),
            build_info: Some(value.build_info.into()),
            binary_sha: value.binary_sha.to_vec(),
            binary_url: value.binary_url.to_string(),
            sidechain_id: value.sidechain_id.map(|v| v.to_vec()).unwrap_or_default(),
            sidechain_id_knowledge_proof: value.sidechain_id_knowledge_proof.map(|v| v.into()),
        }
    }
}

// -------------------------------- ConfidentialOutputData -------------------------------- //
impl TryFrom<grpc::ConfidentialOutputData> for ConfidentialOutputData {
    type Error = String;

    fn try_from(value: grpc::ConfidentialOutputData) -> Result<Self, Self::Error> {
        let sidechain_id = if value.sidechain_id.is_empty() {
            None
        } else {
            Some(PublicKey::from_canonical_bytes(&value.sidechain_id).map_err(|e| format!("sidechain_id: {}", e))?)
        };
        let sidechain_id_knowledge_proof = value
            .sidechain_id_knowledge_proof
            .map(|v| Signature::try_from(v).map_err(|e| format!("sidechain_id_knowledge_proof: {}", e)))
            .transpose()?;
        Ok(ConfidentialOutputData {
            claim_public_key: PublicKey::from_canonical_bytes(&value.claim_public_key).map_err(|e| e.to_string())?,
            sidechain_id,
            sidechain_id_knowledge_proof,
        })
    }
}

impl From<ConfidentialOutputData> for grpc::ConfidentialOutputData {
    fn from(value: ConfidentialOutputData) -> Self {
        Self {
            claim_public_key: value.claim_public_key.to_vec(),
            sidechain_id: value.sidechain_id.map(|v| v.to_vec()).unwrap_or_default(),
            sidechain_id_knowledge_proof: value.sidechain_id_knowledge_proof.map(|v| v.into()),
        }
    }
}

// -------------------------------- TemplateType -------------------------------- //
impl TryFrom<grpc::TemplateType> for TemplateType {
    type Error = String;

    fn try_from(value: grpc::TemplateType) -> Result<Self, Self::Error> {
        let template_type = value.template_type.ok_or("Template type not provided")?;
        match template_type {
            grpc::template_type::TemplateType::Wasm(wasm) => Ok(TemplateType::Wasm {
                abi_version: wasm.abi_version.try_into().map_err(|_| "abi_version overflowed")?,
            }),
            grpc::template_type::TemplateType::Flow(_flow) => Ok(TemplateType::Flow {}),
            grpc::template_type::TemplateType::Manifest(_manifest) => Ok(TemplateType::Manifest {}),
        }
    }
}

impl From<TemplateType> for grpc::TemplateType {
    fn from(value: TemplateType) -> Self {
        match value {
            TemplateType::Wasm { abi_version } => Self {
                template_type: Some(grpc::template_type::TemplateType::Wasm(grpc::WasmInfo {
                    abi_version: abi_version.into(),
                })),
            },
            TemplateType::Flow => Self {
                template_type: Some(grpc::template_type::TemplateType::Flow(grpc::FlowInfo {})),
            },
            TemplateType::Manifest => Self {
                template_type: Some(grpc::template_type::TemplateType::Manifest(grpc::ManifestInfo {})),
            },
        }
    }
}

// -------------------------------- BuildInfo -------------------------------- //

impl TryFrom<grpc::BuildInfo> for BuildInfo {
    type Error = String;

    fn try_from(value: grpc::BuildInfo) -> Result<Self, Self::Error> {
        Ok(Self {
            repo_url: value.repo_url.try_into().map_err(|_| "Invalid repo url")?,
            commit_hash: value.commit_hash.try_into().map_err(|_| "Invalid commit hash")?,
        })
    }
}

impl From<BuildInfo> for grpc::BuildInfo {
    fn from(value: BuildInfo) -> Self {
        Self {
            repo_url: value.repo_url.into_string(),
            commit_hash: value.commit_hash.into_vec(),
        }
    }
}
