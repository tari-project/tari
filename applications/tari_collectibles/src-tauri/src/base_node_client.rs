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

use futures::StreamExt;
use tari_app_grpc::tari_rpc as grpc;

pub struct BaseNodeClient {
  client: grpc::base_node_client::BaseNodeClient<tonic::transport::Channel>,
}

impl BaseNodeClient {
  pub async fn connect(endpoint: String) -> Result<Self, String> {
    let client = grpc::base_node_client::BaseNodeClient::connect(endpoint.clone())
      .await
      .map_err(|err| {
        format!(
          "No connection to wallet. Is it running with grpc on '{}' ? Error: {}",
          endpoint, err
        )
      })?;

    Ok(Self { client })
  }

  pub async fn list_registered_assets(
    &mut self,
    offset: u64,
    count: u64,
  ) -> Result<Vec<grpc::ListAssetRegistrationsResponse>, String> {
    let client = self.client_mut();
    let request = grpc::ListAssetRegistrationsRequest { offset, count };
    let mut stream = client
      .list_asset_registrations(request)
      .await
      .map(|response| response.into_inner())
      .map_err(|s| format!("Could not get register assets: {}", s))?;

    let mut assets = vec![];
    while let Some(result) = stream.next().await {
      let asset = result.map_err(|err| err.to_string())?;
      assets.push(asset);
    }

    Ok(assets)
  }

  fn client_mut(
    &mut self,
  ) -> &mut grpc::base_node_client::BaseNodeClient<tonic::transport::Channel> {
    &mut self.client
  }
}
