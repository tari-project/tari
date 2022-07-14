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
use tari_core::{
    chain_storage::UtxoMinedInfo,
    transactions::transaction_components::{
        ContractAcceptanceChallenge,
        ContractUpdateProposalAcceptanceChallenge,
        OutputType,
        SignerSignature,
        TransactionOutput,
    },
};
use tari_utilities::hex::Hex;

use super::BaseNodeClient;
use crate::{services::wallet_client::WalletClient, DigitalAssetError};

#[async_trait]
pub trait AcceptanceManager: Send + Sync {
    async fn publish_constitution_acceptance(
        &mut self,
        node_identity: &NodeIdentity,
        contract_id: &FixedHash,
    ) -> Result<u64, DigitalAssetError>;

    async fn publish_proposal_acceptance(
        &mut self,
        node_identity: &NodeIdentity,
        contract_id: &FixedHash,
        proposal_id: u64,
    ) -> Result<u64, DigitalAssetError>;
}

#[derive(Clone)]
pub struct ConcreteAcceptanceManager<TWallet, TBaseNode> {
    wallet: TWallet,
    base_node: TBaseNode,
}

impl<TWallet: WalletClient, TBaseNode: BaseNodeClient> ConcreteAcceptanceManager<TWallet, TBaseNode> {
    pub fn new(wallet: TWallet, base_node: TBaseNode) -> Self {
        Self { wallet, base_node }
    }
}

#[async_trait]
impl<TWallet: WalletClient + Sync + Send, TBaseNode: BaseNodeClient + Sync + Send> AcceptanceManager
    for ConcreteAcceptanceManager<TWallet, TBaseNode>
{
    async fn publish_constitution_acceptance(
        &mut self,
        node_identity: &NodeIdentity,
        contract_id: &FixedHash,
    ) -> Result<u64, DigitalAssetError> {
        let public_key = node_identity.public_key();

        // build the acceptance signature
        let secret_key = node_identity.secret_key();
        let constitution_commitment = self.fetch_constitution_commitment(contract_id).await?;
        let challenge = ContractAcceptanceChallenge::new(&constitution_commitment, contract_id);
        let signer_signature = SignerSignature::sign(secret_key, challenge);

        // publish the acceptance
        self.wallet
            .submit_contract_acceptance(contract_id, public_key, signer_signature.signature())
            .await
    }

    async fn publish_proposal_acceptance(
        &mut self,
        node_identity: &NodeIdentity,
        contract_id: &FixedHash,
        proposal_id: u64,
    ) -> Result<u64, DigitalAssetError> {
        let public_key = node_identity.public_key();

        // build the acceptance signature
        let secret_key = node_identity.secret_key();
        let proposal_commitment = self.fetch_proposal_commitment(contract_id, proposal_id).await?;
        let challenge = ContractUpdateProposalAcceptanceChallenge::new(&proposal_commitment, contract_id, proposal_id);
        let signer_signature = SignerSignature::sign(secret_key, challenge);

        // publish the acceptance
        self.wallet
            .submit_contract_update_proposal_acceptance(
                contract_id,
                proposal_id,
                public_key,
                signer_signature.signature(),
            )
            .await
    }
}

impl<TWallet: WalletClient + Sync + Send, TBaseNode: BaseNodeClient + Sync + Send>
    ConcreteAcceptanceManager<TWallet, TBaseNode>
{
    async fn fetch_constitution_commitment(
        &mut self,
        contract_id: &FixedHash,
    ) -> Result<Commitment, DigitalAssetError> {
        let outputs: Vec<UtxoMinedInfo> = self
            .base_node
            .get_current_contract_outputs(0, *contract_id, OutputType::ContractConstitution)
            .await?;
        let transaction_outputs: Vec<TransactionOutput> = outputs
            .into_iter()
            .filter_map(|utxo| utxo.output.into_unpruned_output())
            .collect();

        if transaction_outputs.is_empty() {
            return Err(DigitalAssetError::NotFound {
                entity: "constitution",
                id: contract_id.to_hex(),
            });
        }
        let constitution_commitment = transaction_outputs[0].commitment();

        Ok(constitution_commitment.clone())
    }

    async fn fetch_proposal_commitment(
        &mut self,
        contract_id: &FixedHash,
        proposal_id: u64,
    ) -> Result<Commitment, DigitalAssetError> {
        let outputs: Vec<UtxoMinedInfo> = self
            .base_node
            .get_current_contract_outputs(0, *contract_id, OutputType::ContractConstitutionProposal)
            .await?;
        let transaction_outputs: Vec<TransactionOutput> = outputs
            .into_iter()
            .filter_map(|utxo| utxo.output.into_unpruned_output())
            .filter(|output| output.features.contains_sidechain_proposal(contract_id, proposal_id))
            .collect();

        if transaction_outputs.is_empty() {
            return Err(DigitalAssetError::NotFound {
                entity: "update proposal",
                id: contract_id.to_hex(),
            });
        }
        let proposal_commitment = transaction_outputs[0].commitment();

        Ok(proposal_commitment.clone())
    }
}
