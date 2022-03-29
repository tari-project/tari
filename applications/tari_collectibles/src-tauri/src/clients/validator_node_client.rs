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
use log::{debug, error};
use tari_app_grpc::tari_rpc as grpc;
use tari_common_types::types::PublicKey;
use tari_utilities::ByteArray;

const LOG_TARGET: &str = "collectibles::validator_node";

pub trait ValidatorNodeClient {}

pub struct GrpcValidatorNodeClient {
  client: grpc::validator_node_client::ValidatorNodeClient<tonic::transport::Channel>,
}

impl GrpcValidatorNodeClient {
  pub async fn connect(endpoint: String) -> Result<Self, CollectiblesError> {
    let s = Self {
      client: grpc::validator_node_client::ValidatorNodeClient::connect(endpoint.clone())
        .await
        .map_err(|e| CollectiblesError::ClientConnection {
          client: "validator_node",
          address: endpoint,
          error: e.to_string(),
        })?,
    };
    Ok(s)
  }

  pub async fn invoke_read_method(
    &mut self,
    asset_public_key: PublicKey,
    template_id: u32,
    method: String,
    args: Vec<u8>,
  ) -> Result<Vec<u8>, CollectiblesError> {
    let req = grpc::InvokeReadMethodRequest {
      asset_public_key: Vec::from(asset_public_key.as_bytes()),
      template_id,
      method,
      args,
    };
    debug!(target: LOG_TARGET, "req {:?}", req);
    let response = self
      .client
      .invoke_read_method(req)
      .await
      .map(|resp| resp.into_inner())
      .map_err(|e| {
        error!(target: LOG_TARGET, "{}", e);

        CollectiblesError::ClientRequest {
          source: e,
          request: "invoke_read_method".to_string(),
        }
      })?;
    debug!(target: LOG_TARGET, "response {:?}", response);
    Ok(response.result)
  }

  pub async fn invoke_method(
    &mut self,
    asset_public_key: PublicKey,
    template_id: u32,
    method: String,
    args: Vec<u8>,
  ) -> Result<Vec<u8>, CollectiblesError> {
    let req = grpc::InvokeMethodRequest {
      asset_public_key: Vec::from(asset_public_key.as_bytes()),
      template_id,
      method,
      args,
    };
    debug!(target: LOG_TARGET, "req {:?}", req);
    let response = self
      .client
      .invoke_method(req)
      .await
      .map(|resp| resp.into_inner())
      .map_err(|e| {
        error!(target: LOG_TARGET, "{}", e);

        CollectiblesError::ClientRequest {
          source: e,
          request: "invoke_method".to_string(),
        }
      })?;
    debug!(target: LOG_TARGET, "response {:?}", response);
    Ok(response.result)
  }
}
