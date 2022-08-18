// Copyright 2019, The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

//! Impls for sidechain_features proto

use std::convert::{TryFrom, TryInto};

use tari_common_types::types::{PublicKey, Signature};
use tari_utilities::ByteArray;

use crate::{
    consensus::MaxSizeString,
    proto,
    transactions::transaction_components::{BuildInfo, CodeTemplateRegistration, SideChainFeatures, TemplateType},
};

//---------------------------------- SideChainFeatures --------------------------------------------//
impl From<SideChainFeatures> for proto::types::SideChainFeatures {
    fn from(value: SideChainFeatures) -> Self {
        Self {
            template_registration: value.template_registration.map(Into::into),
        }
    }
}

impl TryFrom<proto::types::SideChainFeatures> for SideChainFeatures {
    type Error = String;

    fn try_from(features: proto::types::SideChainFeatures) -> Result<Self, Self::Error> {
        let template_registration = features
            .template_registration
            .map(CodeTemplateRegistration::try_from)
            .transpose()?;

        Ok(Self { template_registration })
    }
}

// -------------------------------- TemplateRegistration -------------------------------- //
impl TryFrom<proto::types::TemplateRegistration> for CodeTemplateRegistration {
    type Error = String;

    fn try_from(value: proto::types::TemplateRegistration) -> Result<Self, Self::Error> {
        Ok(Self {
            author_public_key: PublicKey::from_bytes(&value.author_public_key).map_err(|e| e.to_string())?,
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
        })
    }
}

impl From<CodeTemplateRegistration> for proto::types::TemplateRegistration {
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
        }
    }
}

// -------------------------------- TemplateType -------------------------------- //
impl TryFrom<proto::types::TemplateType> for TemplateType {
    type Error = String;

    fn try_from(value: proto::types::TemplateType) -> Result<Self, Self::Error> {
        let template_type = value.template_type.ok_or("Template type not provided")?;
        match template_type {
            proto::types::template_type::TemplateType::Wasm(wasm) => Ok(TemplateType::Wasm {
                abi_version: wasm.abi_version.try_into().map_err(|_| "abi_version overflowed")?,
            }),
        }
    }
}

impl From<TemplateType> for proto::types::TemplateType {
    fn from(value: TemplateType) -> Self {
        match value {
            TemplateType::Wasm { abi_version } => Self {
                template_type: Some(proto::types::template_type::TemplateType::Wasm(
                    proto::types::WasmInfo {
                        abi_version: abi_version.into(),
                    },
                )),
            },
        }
    }
}

// -------------------------------- BuildInfo -------------------------------- //

impl TryFrom<proto::types::BuildInfo> for BuildInfo {
    type Error = String;

    fn try_from(value: proto::types::BuildInfo) -> Result<Self, Self::Error> {
        Ok(Self {
            repo_url: value.repo_url.try_into().map_err(|_| "Invalid repo url")?,
            commit_hash: value.commit_hash.try_into().map_err(|_| "Invalid commit hash")?,
        })
    }
}

impl From<BuildInfo> for proto::types::BuildInfo {
    fn from(value: BuildInfo) -> Self {
        Self {
            repo_url: value.repo_url.into_string(),
            commit_hash: value.commit_hash.into_vec(),
        }
    }
}
