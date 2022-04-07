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
use tari_app_grpc::{tari_rpc as grpc, tari_rpc::CreateFollowOnAssetCheckpointRequest};
use tari_common_types::types::PublicKey;
use tari_comms::types::CommsPublicKey;
use tari_crypto::tari_utilities::ByteArray;
use tari_dan_core::{models::StateRoot, services::WalletClient, DigitalAssetError};

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
        asset_public_key: &PublicKey,
        checkpoint_unique_id: &[u8],
        state_root: &StateRoot,
        next_committee: Vec<CommsPublicKey>,
    ) -> Result<(), DigitalAssetError> {
        let inner = self.connection().await?;

        let request = CreateFollowOnAssetCheckpointRequest {
            asset_public_key: asset_public_key.as_bytes().to_vec(),
            unique_id: Vec::from(checkpoint_unique_id),
            merkle_root: state_root.as_bytes().to_vec(),
            next_committee: next_committee.into_iter().map(|c| c.as_bytes().to_vec()).collect(),
        };

        let res = inner
            .create_follow_on_asset_checkpoint(request)
            .await
            .map_err(|e| DigitalAssetError::FatalError(format!("Could not create checkpoint:{}", e)))?;

        println!("{:?}", res);
        Ok(())
    }
}
