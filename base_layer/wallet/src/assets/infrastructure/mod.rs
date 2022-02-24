// Copyright 2021. The Tari Project
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

mod asset_manager_service;
pub use asset_manager_service::AssetManagerService;
use tari_common_types::{
    transaction::TxId,
    types::{Commitment, FixedHash, PublicKey},
};
use tari_core::transactions::transaction_components::{OutputFeatures, TemplateParameter, Transaction};

use crate::assets::Asset;

pub mod initializer;

#[derive(Debug)]
pub enum AssetManagerRequest {
    ListOwned {},
    GetOwnedAsset {
        public_key: PublicKey,
    },
    CreateRegistrationTransaction {
        name: String,
        public_key: Box<PublicKey>,
        template_ids_implemented: Vec<u32>,
        description: Option<String>,
        image: Option<String>,
        template_parameters: Vec<TemplateParameter>,
    },
    CreateMintingTransaction {
        asset_public_key: Box<PublicKey>,
        asset_owner_commitment: Box<Commitment>,
        features: Vec<(Vec<u8>, Option<OutputFeatures>)>,
    },
    CreateInitialCheckpoint {
        asset_public_key: Box<PublicKey>,
        merkle_root: FixedHash,
        committee_public_keys: Vec<PublicKey>,
    },
    CreateFollowOnCheckpoint {
        asset_public_key: Box<PublicKey>,
        unique_id: Vec<u8>,
        merkle_root: FixedHash,
        committee_public_keys: Vec<PublicKey>,
    },
    CreateCommitteeCheckpoint {
        asset_public_key: Box<PublicKey>,
        committee_public_keys: Vec<PublicKey>,
        effective_sidechain_height: u64,
    },
}

pub enum AssetManagerResponse {
    ListOwned { assets: Vec<Asset> },
    GetOwnedAsset { asset: Box<Asset> },
    CreateRegistrationTransaction { transaction: Box<Transaction>, tx_id: TxId },
    CreateMintingTransaction { transaction: Box<Transaction>, tx_id: TxId },
    CreateInitialCheckpoint { transaction: Box<Transaction>, tx_id: TxId },
    CreateFollowOnCheckpoint { transaction: Box<Transaction>, tx_id: TxId },
    CreateCommitteeCheckpoint { transaction: Box<Transaction>, tx_id: TxId },
}
