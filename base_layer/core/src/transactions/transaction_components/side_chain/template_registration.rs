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

use blake2::Blake2b;
use borsh::{BorshDeserialize, BorshSerialize};
use digest::consts::U64;
use serde::{Deserialize, Serialize};
use tari_common_types::types::{PublicKey, Signature};
use tari_hashing::TransactionHashDomain;
use tari_utilities::ByteArray;

use crate::consensus::{DomainSeparatedConsensusHasher, MaxSizeBytes, MaxSizeString};

#[derive(Debug, Clone, Hash, PartialEq, Eq, Deserialize, Serialize, BorshSerialize, BorshDeserialize)]
pub struct CodeTemplateRegistration {
    pub author_public_key: PublicKey,
    pub author_signature: Signature,
    pub template_name: MaxSizeString<32>,
    pub template_version: u16,
    pub template_type: TemplateType,
    pub build_info: BuildInfo,
    pub binary_sha: MaxSizeBytes<32>,
    pub binary_url: MaxSizeString<255>,
    pub sidechain_id: Option<PublicKey>,
    pub sidechain_id_knowledge_proof: Option<Signature>,
}

impl CodeTemplateRegistration {
    pub fn create_challenge(&self, public_nonce: &PublicKey) -> [u8; 64] {
        DomainSeparatedConsensusHasher::<TransactionHashDomain, Blake2b<U64>>::new("template_registration")
            .chain(&self.author_public_key)
            .chain(public_nonce)
            .chain(&self.binary_sha)
            .chain(&self.sidechain_id.as_ref().map(|n| n.to_vec()).unwrap_or(vec![0u8; 32]))
            .finalize()
            .into()
    }

    pub fn create_challenge_from_components(
        author_public_key: &PublicKey,
        public_nonce: &PublicKey,
        binary_sha: &MaxSizeBytes<32>,
        sidechain_id: Option<&PublicKey>,
    ) -> [u8; 64] {
        DomainSeparatedConsensusHasher::<TransactionHashDomain, Blake2b<U64>>::new("template_registration")
            .chain(author_public_key)
            .chain(public_nonce)
            .chain(binary_sha)
            .chain(&sidechain_id.as_ref().map(|n| n.to_vec()).unwrap_or(vec![0u8; 32]))
            .finalize()
            .into()
    }
}

// -------------------------------- TemplateType -------------------------------- //

#[derive(Debug, Clone, Hash, PartialEq, Eq, Deserialize, Serialize, BorshSerialize, BorshDeserialize)]
pub enum TemplateType {
    /// Indicates that the template is a WASM module
    Wasm { abi_version: u16 },
    /// A flow template
    Flow,
    /// A manifest template
    Manifest,
}

// -------------------------------- BuildInfo -------------------------------- //

#[derive(Debug, Clone, Hash, PartialEq, Eq, Deserialize, Serialize, BorshSerialize, BorshDeserialize)]
pub struct BuildInfo {
    pub repo_url: MaxSizeString<255>,
    pub commit_hash: MaxSizeBytes<32>,
}
