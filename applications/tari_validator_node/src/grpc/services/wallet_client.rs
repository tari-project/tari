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

use std::net::SocketAddr;

use async_trait::async_trait;
use tari_app_grpc::{
    tari_rpc as grpc,
    tari_rpc::{
        CreateFollowOnAssetCheckpointRequest,
        CreateInitialAssetCheckpointRequest,
        SubmitContractAcceptanceRequest,
        SubmitContractUpdateProposalAcceptanceRequest,
    },
};
use tari_common_types::types::{FixedHash, PublicKey, Signature};
use tari_core::transactions::transaction_components::SignerSignature;
use tari_crypto::tari_utilities::ByteArray;
use tari_dan_core::{services::WalletClient, DigitalAssetError};
use tari_dan_engine::state::models::StateRoot;

const _LOG_TARGET: &str = "tari::dan::wallet_grpc";

type Inner = grpc::wallet_client::WalletClient<tonic::transport::Channel>;

#[derive(Clone)]
pub struct GrpcWalletClient {
    endpoint: SocketAddr,
    inner: Option<Inner>,
}

impl GrpcWalletClient {
    pub fn new(endpoint: SocketAddr) -> GrpcWalletClient {
        Self { endpoint, inner: None }
    }

    pub async fn connection(&mut self) -> Result<&mut Inner, DigitalAssetError> {
        if self.inner.is_none() {
            let url = format!("http://{}", self.endpoint);
            let inner = Inner::connect(url).await?;
            self.inner = Some(inner);
        }
        self.inner
            .as_mut()
            .ok_or_else(|| DigitalAssetError::FatalError("no connection".into()))
    }
}

#[async_trait]
impl WalletClient for GrpcWalletClient {
    async fn create_new_checkpoint(
        &mut self,
        contract_id: &FixedHash,
        state_root: &StateRoot,
        checkpoint_number: u64,
        checkpoint_signatures: &[SignerSignature],
    ) -> Result<(), DigitalAssetError> {
        let inner = self.connection().await?;
        let committee_signatures = grpc::CommitteeSignatures {
            signatures: checkpoint_signatures.iter().map(Into::into).collect(),
        };

        if checkpoint_number == 0 {
            let request = CreateInitialAssetCheckpointRequest {
                contract_id: contract_id.to_vec(),
                merkle_root: state_root.as_bytes().to_vec(),
                committee_signatures: Some(committee_signatures),
            };

            let _res = inner
                .create_initial_asset_checkpoint(request)
                .await
                .map_err(|e| DigitalAssetError::FatalError(format!("Could not create checkpoint:{}", e)))?;
        } else {
            let request = CreateFollowOnAssetCheckpointRequest {
                checkpoint_number,
                contract_id: contract_id.to_vec(),
                merkle_root: state_root.as_bytes().to_vec(),
                committee_signatures: Some(committee_signatures),
            };

            let _res = inner
                .create_follow_on_asset_checkpoint(request)
                .await
                .map_err(|e| DigitalAssetError::FatalError(format!("Could not create checkpoint:{}", e)))?;
        }

        Ok(())
    }

    async fn submit_contract_acceptance(
        &mut self,
        contract_id: &FixedHash,
        validator_node_public_key: &PublicKey,
        signature: &Signature,
    ) -> Result<u64, DigitalAssetError> {
        let inner = self.connection().await?;

        let request = SubmitContractAcceptanceRequest {
            contract_id: contract_id.as_bytes().to_vec(),
            validator_node_public_key: validator_node_public_key.as_bytes().to_vec(),
            signature: Some((*signature).clone().into()),
        };

        let res = inner
            .submit_contract_acceptance(request)
            .await
            .map_err(|e| DigitalAssetError::FatalError(format!("Could not submit contract acceptance: {}", e)))?;

        Ok(res.into_inner().tx_id)
    }

    async fn submit_contract_update_proposal_acceptance(
        &mut self,
        contract_id: &FixedHash,
        proposal_id: u64,
        validator_node_public_key: &PublicKey,
        signature: &Signature,
    ) -> Result<u64, DigitalAssetError> {
        let inner = self.connection().await?;

        let request = SubmitContractUpdateProposalAcceptanceRequest {
            contract_id: contract_id.as_bytes().to_vec(),
            proposal_id,
            validator_node_public_key: validator_node_public_key.as_bytes().to_vec(),
            signature: Some((*signature).clone().into()),
        };

        let res = inner
            .submit_contract_update_proposal_acceptance(request)
            .await
            .map_err(|e| {
                DigitalAssetError::FatalError(format!("Could not submit contract update proposal acceptance: {}", e))
            })?;

        Ok(res.into_inner().tx_id)
    }
}
