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

use tari_app_grpc::{tari_rpc as grpc, tari_rpc::RegisterAssetRequest};
use tari_utilities::hex::Hex;

pub struct WalletClient {
  endpoint: String,
  inner: Option<grpc::wallet_client::WalletClient<tonic::transport::Channel>>,
}

impl WalletClient {
  pub fn new(endpoint: String) -> Self {
    Self {
      inner: None,
      endpoint,
    }
  }

  pub async fn connect(&mut self) -> Result<(), String> {
    let dst = format!("http://{}", self.endpoint);
    let client = grpc::wallet_client::WalletClient::connect(dst)
      .await
      .map_err(|err| {
        format!(
          "No connection to wallet. Is it running with grpc on '{}' ? Error: {}",
          self.endpoint, err
        )
      })?;

    self.inner = Some(client);
    Ok(())
  }

  pub async fn register_asset(
    &mut self,
    name: String,
    description: String,
    image: String,
    template_ids_implemented: Vec<u32>,
    template_parameters: Vec<grpc::TemplateParameter>,
  ) -> Result<String, String> {
    let inner = self.inner.as_mut().unwrap();
    let request = RegisterAssetRequest {
      name,
      template_ids_implemented,
      description,
      image,
      template_parameters,
    };
    let result = inner
      .register_asset(request)
      .await
      .map_err(|s| format!("Could not register asset: {}", s))?;
    dbg!(&result);
    Ok(result.into_inner().public_key.to_hex())
  }

  pub async fn list_owned_assets(&mut self) -> Result<grpc::GetOwnedAssetsResponse, String> {
    let inner = self.inner.as_mut().unwrap();
    let request = grpc::Empty {};
    let result = inner.get_owned_assets(request).await.unwrap();
    dbg!(&result);
    Ok(result.into_inner())
  }

  pub async fn create_initial_asset_checkpoint(
    &mut self,
    asset_public_key: String,
    merkle_root: Vec<u8>,
    committee: Vec<String>,
  ) -> Result<grpc::CreateInitialAssetCheckpointResponse, String> {
    let inner = self.inner.as_mut().unwrap();
    let request = grpc::CreateInitialAssetCheckpointRequest {
      asset_public_key: Vec::from_hex(&asset_public_key).unwrap(),
      merkle_root,
      committee: committee
        .iter()
        .map(|s| Vec::from_hex(s).unwrap())
        .collect(),
    };
    let result = inner
      .create_initial_asset_checkpoint(request)
      .await
      .unwrap();
    dbg!(&result);
    Ok(result.into_inner())
  }
}
