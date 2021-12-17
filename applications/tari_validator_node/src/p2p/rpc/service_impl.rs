//  Copyright 2021, The Tari Project
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
use tari_common_types::types::PublicKey;
use tari_comms::protocol::rpc::{Request, Response, RpcStatus};
use tari_crypto::tari_utilities::ByteArray;
use tari_dan_core::{
    models::{Instruction, TemplateId},
    services::{AssetProcessor, MempoolService},
    storage::DbFactory,
};

use crate::p2p::{proto::validator_node as proto, rpc::ValidatorNodeRpcService};

pub struct ValidatorNodeRpcServiceImpl<TMempoolService, TDbFactory: DbFactory, TAssetProcessor> {
    mempool_service: TMempoolService,
    db_factory: TDbFactory,
    asset_processor: TAssetProcessor,
}

impl<
        TMempoolService: MempoolService + Clone,
        TDbFactory: DbFactory + Clone,
        TAssetProcessor: AssetProcessor + Clone,
    > ValidatorNodeRpcServiceImpl<TMempoolService, TDbFactory, TAssetProcessor>
{
    pub fn new(mempool_service: TMempoolService, db_factory: TDbFactory, asset_processor: TAssetProcessor) -> Self {
        Self {
            mempool_service,
            db_factory,
            asset_processor,
        }
    }
}

#[tari_comms::async_trait]
impl<
        TMempoolService: MempoolService + Clone,
        TDbFactory: DbFactory + Clone,
        TAssetProcessor: AssetProcessor + Clone,
    > ValidatorNodeRpcService for ValidatorNodeRpcServiceImpl<TMempoolService, TDbFactory, TAssetProcessor>
{
    async fn get_token_data(
        &self,
        request: Request<proto::GetTokenDataRequest>,
    ) -> Result<Response<proto::GetTokenDataResponse>, RpcStatus> {
        dbg!(&request);
        Err(RpcStatus::general("Not implemented"))
    }

    async fn invoke_read_method(
        &self,
        request: Request<proto::InvokeReadMethodRequest>,
    ) -> Result<Response<proto::InvokeReadMethodResponse>, RpcStatus> {
        dbg!(&request);
        let request = request.into_message();
        let asset_public_key = PublicKey::from_bytes(&request.asset_public_key)
            .map_err(|err| RpcStatus::bad_request(format!("Asset public key was not a valid public key:{}", err)))?;
        let state = self
            .db_factory
            .get_state_db(&asset_public_key)
            .map_err(|e| RpcStatus::general(format!("Could not create state db: {}", e)))?
            .ok_or_else(|| RpcStatus::not_found("This node does not process this asset".to_string()))?;
        let mut unit_of_work = state.new_unit_of_work();
        let response_bytes = self
            .asset_processor
            .invoke_read_method(
                TemplateId::from(request.template_id),
                request.method,
                &request.args,
                &mut unit_of_work,
            )
            .map_err(|e| RpcStatus::general(format!("Could not invoke read method: {}", e)))?;
        Ok(Response::new(proto::InvokeReadMethodResponse {
            result: response_bytes,
        }))
    }

    async fn invoke_method(
        &self,
        request: Request<proto::InvokeMethodRequest>,
    ) -> Result<Response<proto::InvokeMethodResponse>, RpcStatus> {
        dbg!(&request);
        let request = request.into_message();
        let instruction = Instruction::new(
            PublicKey::from_bytes(&request.asset_public_key)
                .map_err(|_err| RpcStatus::bad_request("asset_public_key was not a valid public key"))?,
            request.template_id.into(),
            request.method.clone(),
            request.args.clone(),
            /* TokenId(request.token_id.clone()),
             * TODO: put signature in here
             * ComSig::default()
             * create_com_sig_from_bytes(&request.signature)
             *     .map_err(|err| Status::invalid_argument("signature was not a valid comsig"))?, */
        );

        let mut mempool_service = self.mempool_service.clone();
        match mempool_service.submit_instruction(instruction).await {
            Ok(_) => {
                return Ok(Response::new(proto::InvokeMethodResponse {
                    result: None,
                    status: proto::Status::Accepted as i32,
                }))
            },
            Err(_) => {
                return Ok(Response::new(proto::InvokeMethodResponse {
                    result: None,
                    status: proto::Status::Errored as i32,
                }))
            },
        }
    }
}
