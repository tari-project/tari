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

use async_trait::async_trait;
use tari_common_types::types::{Commitment, FixedHash};
use tari_comms::NodeIdentity;
use tari_core::transactions::transaction_components::SignerSignature;

use crate::{models::AcceptanceChallenge, services::wallet_client::WalletClient, DigitalAssetError};

#[async_trait]
pub trait AcceptanceManager: Send + Sync {
    async fn publish_acceptance(
        &mut self,
        node_identity: &NodeIdentity,
        contract_id: &FixedHash,
    ) -> Result<u64, DigitalAssetError>;
}

#[derive(Clone)]
pub struct ConcreteAcceptanceManager<TWallet: WalletClient> {
    wallet: TWallet,
}

impl<TWallet: WalletClient> ConcreteAcceptanceManager<TWallet> {
    pub fn new(wallet: TWallet) -> Self {
        Self { wallet }
    }
}

#[async_trait]
impl<TWallet: WalletClient + Sync + Send> AcceptanceManager for ConcreteAcceptanceManager<TWallet> {
    async fn publish_acceptance(
        &mut self,
        node_identity: &NodeIdentity,
        contract_id: &FixedHash,
    ) -> Result<u64, DigitalAssetError> {
        // TODO: fetch the real contract constitution commitment from the base_node_client
        let constitution_commitment = Commitment::default();
        let public_key = node_identity.public_key();

        // build the acceptance signature
        let secret_key = node_identity.secret_key();
        let challenge = AcceptanceChallenge::new(&constitution_commitment, contract_id);
        let signature = SignerSignature::sign(secret_key, challenge).signature;

        // publish the acceptance
        self.wallet
            .submit_contract_acceptance(contract_id, public_key, &signature)
            .await
    }
}
