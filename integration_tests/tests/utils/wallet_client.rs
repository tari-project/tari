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
use tari_integration_tests::error::GrpcBaseNodeError;
use tari_wallet_grpc_client::Client as GrpcWallet;

use crate::utils::wallet::WalletClient;
// const LOG_TARGET: &str = "tari::validator_node::app";

type Client = GrpcWallet<tonic::transport::Channel>;

#[derive(Clone)]
pub struct GrpcWalletClient {
    endpoint: SocketAddr,
    client: Option<Client>,
}

impl GrpcWalletClient {
    #[allow(dead_code)]
    pub fn new(endpoint: SocketAddr) -> GrpcWalletClient {
        Self { endpoint, client: None }
    }

    #[allow(dead_code)]
    async fn connection(&mut self) -> Result<&mut Client, GrpcBaseNodeError> {
        if self.client.is_none() {
            let url = format!("http://{}", self.endpoint);
            let inner = Client::connect(url).await?;
            self.client = Some(inner);
        }

        Ok(self.client.as_mut().unwrap())

        // self.client
        // .as_mut()
        // .ok_or_else(|| DigitalAssetError::FatalError("no connection".into()))
    }

    // pub async fn register_validator_node(
    //     &mut self,
    //     validator_node_public_key: PublicKey,
    //     signature: Signature,
    //     fee_per_gram: MicroTari,
    //     message: String,
    // ) -> Result<RegisterValidatorNodeResponse, DigitalAssetError> {
    //     let inner = self.connection().await?;
    //     let signature = ValidatorNodeSignature::sign(node_identity.secret_key(), b"");
    //
    //     let request = RegisterValidatorNodeRequest {
    //         validator_node_public_key: validator_node_public_key.to_vec(),
    //         validator_node_signature: Some(signature.into()),
    //         fee_per_gram: fee_per_gram.as_u64(),
    //         message,
    //     };
    //
    //     let result = inner.register_validator_node(request).await?.into_inner();
    //
    //     if result.is_success {
    //         Ok(result)
    //     } else {
    //         // FIXME: add generic error for failed rpc calls
    //         // Err(DigitalAssetError::NodeRegistration(result.failure_message))
    //     }
    // }
    //
    // pub async fn register_template(
    //     &mut self,
    //     node_identity: &NodeIdentity,
    //     data: TemplateRegistrationRequest,
    // ) -> Result<CreateTemplateRegistrationResponse, DigitalAssetError> {
    //     let inner = self.connection().await?;
    //     let signature = sign_template_registration(node_identity.secret_key(), data.binary_sha.to_vec());
    //     let request = CreateTemplateRegistrationRequest {
    //         template_registration: Some(TemplateRegistration {
    //             author_public_key: node_identity.public_key().to_vec(),
    //             author_signature: Some(signature.into()),
    //             template_name: data.template_name,
    //             template_version: data.template_version.into(),
    //             // TODO: fill real abi_version
    //             template_type: Some(TemplateType {
    //                 template_type: Some(tari_app_grpc::tari_rpc::template_type::TemplateType::Wasm(WasmInfo {
    //                     abi_version: 1,
    //                 })),
    //             }),
    //             build_info: Some(BuildInfo {
    //                 repo_url: data.repo_url,
    //                 commit_hash: data.commit_hash,
    //             }),
    //             binary_sha: data.binary_sha,
    //             binary_url: data.binary_url,
    //         }),
    //         fee_per_gram: 1,
    //     };
    //     let result = inner.create_template_registration(request).await?.into_inner();
    //     Ok(result)
    // }
}

#[async_trait]
impl WalletClient for GrpcWalletClient {}
