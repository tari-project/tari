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

use crate::error::CollectiblesError;
use futures::StreamExt;
use log::debug;
use tari_app_grpc::tari_rpc as grpc;
use tari_common_types::types::PublicKey;
use tari_utilities::ByteArray;

const LOG_TARGET: &str = "collectibles::base";

pub struct BaseNodeClient {
  client: grpc::base_node_client::BaseNodeClient<tonic::transport::Channel>,
}

impl BaseNodeClient {
  pub async fn connect(endpoint: String) -> Result<Self, CollectiblesError> {
    let client = grpc::base_node_client::BaseNodeClient::connect(endpoint.clone())
      .await
      .map_err(|err| CollectiblesError::ClientConnection {
        client: "wallet",
        address: endpoint,
        error: err.to_string(),
      })?;

    Ok(Self { client })
  }

  pub async fn list_registered_assets(
    &mut self,
    offset: u64,
    count: u64,
  ) -> Result<Vec<grpc::ListAssetRegistrationsResponse>, CollectiblesError> {
    let client = self.client_mut();
    let request = grpc::ListAssetRegistrationsRequest { offset, count };
    let mut stream = client
      .list_asset_registrations(request)
      .await
      .map(|response| response.into_inner())
      .map_err(|source| CollectiblesError::ClientRequest {
        request: "list_asset_registrations".to_string(),
        source,
      })?;

    let mut assets = vec![];
    while let Some(result) = stream.next().await {
      let asset = result.map_err(|source| CollectiblesError::ClientRequest {
        request: "list_asset_registrations".to_string(),
        source,
      })?;
      assets.push(asset);
    }

    Ok(assets)
  }

  pub async fn get_asset_metadata(
    &mut self,
    asset_public_key: &PublicKey,
  ) -> Result<grpc::GetAssetMetadataResponse, CollectiblesError> {
    let client = self.client_mut();
    let request = grpc::GetAssetMetadataRequest {
      asset_public_key: Vec::from(asset_public_key.as_bytes()),
    };
    debug!(target: LOG_TARGET, "request {:?}", request);
    let response = client
      .get_asset_metadata(request)
      .await
      .map(|response| response.into_inner())
      .map_err(|s| CollectiblesError::ClientRequest {
        request: "get_asset_metadata".to_string(),
        source: s,
      })?;
    debug!(target: LOG_TARGET, "response {:?}", response);
    Ok(response)
  }

  // TODO: probably can get the full checkpoint instead
  pub async fn get_sidechain_committee(
    &mut self,
    asset_public_key: &PublicKey,
  ) -> Result<Vec<PublicKey>, String> {
    let client = self.client_mut();
    let request = grpc::GetTokensRequest {
      asset_public_key: Vec::from(asset_public_key.as_bytes()),
      unique_ids: vec![vec![3u8; 32]],
    };

    debug!(target: LOG_TARGET, "request {:?}", request);
    let mut stream = client
      .get_tokens(request)
      .await
      .map(|response| response.into_inner())
      .map_err(|_s| "Could not get asset sidechain checkpoint".to_string())?;
    let mut i = 0;
    // Could def do this better
    #[allow(clippy::never_loop)]
    while let Some(response) = stream.next().await {
      i += 1;
      if i > 10 {
        break;
      }
      debug!(target: LOG_TARGET, "response {:?}", response);
      let features = response
        .map_err(|status| format!("Got an error status from GRPC:{}", status))?
        .features;
      if let Some(sidechain) = features.and_then(|f| f.sidechain_checkpoint) {
        let pub_keys = sidechain
          .committee
          .iter()
          .map(|s| PublicKey::from_bytes(s).map_err(|e| format!("Not a valid public key:{}", e)))
          .collect::<Result<_, String>>()?;
        return Ok(pub_keys);
      } else {
        return Err("Found utxo but was missing sidechain data".to_string());
      }
    }
    Err(format!(
      "No side chain tokens were found out of {} streamed",
      i
    ))
  }

  fn client_mut(
    &mut self,
  ) -> &mut grpc::base_node_client::BaseNodeClient<tonic::transport::Channel> {
    &mut self.client
  }
}
