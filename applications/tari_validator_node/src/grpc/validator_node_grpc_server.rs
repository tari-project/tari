// Copyright 2021. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
use tari_app_grpc::tari_rpc as rpc;
use tari_common_types::types::PublicKey;
use tari_crypto::tari_utilities::ByteArray;
use tari_dan_core::{
    models::{Instruction, TemplateId},
    services::{AssetProcessor, AssetProxy, MempoolService},
    storage::DbFactory,
};
use tonic::{Request, Response, Status};

pub struct ValidatorNodeGrpcServer<
    TMempoolService: MempoolService,
    TDbFactory: DbFactory,
    TAssetProcessor: AssetProcessor,
    TAssetProxy: AssetProxy,
> {
    mempool: TMempoolService,
    db_factory: TDbFactory,
    asset_processor: TAssetProcessor,
    asset_proxy: TAssetProxy,
}

impl<
        TMempoolService: MempoolService,
        TDbFactory: DbFactory,
        TAssetProcessor: AssetProcessor,
        TAssetProxy: AssetProxy,
    > ValidatorNodeGrpcServer<TMempoolService, TDbFactory, TAssetProcessor, TAssetProxy>
{
    pub fn new(
        mempool: TMempoolService,
        db_factory: TDbFactory,
        asset_processor: TAssetProcessor,
        asset_proxy: TAssetProxy,
    ) -> Self {
        Self {
            mempool,
            db_factory,
            asset_processor,
            asset_proxy,
        }
    }
}

#[tonic::async_trait]
impl<TMempoolService, TDbFactory, TAssetProcessor, TAssetProxy> rpc::validator_node_server::ValidatorNode
    for ValidatorNodeGrpcServer<TMempoolService, TDbFactory, TAssetProcessor, TAssetProxy>
where
    TMempoolService: MempoolService + Clone + Sync + Send + 'static,
    TDbFactory: DbFactory + Sync + Send + 'static,
    TAssetProcessor: AssetProcessor + Sync + Send + 'static,
    TAssetProxy: AssetProxy + Sync + Send + 'static,
{
    async fn get_token_data(
        &self,
        request: tonic::Request<rpc::GetTokenDataRequest>,
    ) -> Result<tonic::Response<rpc::GetTokenDataResponse>, tonic::Status> {
        dbg!(&request);
        Err(Status::internal("Oh noes"))
    }

    async fn invoke_method(
        &self,
        request: Request<rpc::InvokeMethodRequest>,
    ) -> Result<Response<rpc::InvokeMethodResponse>, Status> {
        dbg!(&request);
        let request = request.into_inner();
        let instruction = Instruction::new(
            PublicKey::from_bytes(&request.asset_public_key)
                .map_err(|_err| Status::invalid_argument("asset_public_key was not a valid public key"))?,
            request.template_id.into(),
            request.method.clone(),
            request.args.clone(),
            /* TokenId(request.token_id.clone()),
             * TODO: put signature in here
             * ComSig::default()
             * create_com_sig_from_bytes(&request.signature)
             *     .map_err(|err| Status::invalid_argument("signature was not a valid comsig"))?, */
        );

        let mut mempool = self.mempool.clone();
        match mempool.submit_instruction(instruction).await {
            Ok(_) => {
                return Ok(Response::new(rpc::InvokeMethodResponse {
                    status: "Accepted".to_string(),
                    result: None,
                }))
            },
            Err(_) => {
                return Ok(Response::new(rpc::InvokeMethodResponse {
                    status: "Errored".to_string(),
                    result: None,
                }))
            },
        }
    }

    async fn get_metadata(
        &self,
        request: Request<rpc::GetMetadataRequest>,
    ) -> Result<Response<rpc::GetMetadataResponse>, Status> {
        dbg!(&request);
        // let db = self.db_factory.create();
        todo!()
        // let mut tx = db.new_unit_of_work();
        // let metadata = db.metadata.read(&mut tx);
        // // .map_err(|e| Status::internal(format!("Could not read metadata from storage:{}", e)))?;
        // Ok(Response::new(rpc::GetMetadataResponse {
        //     sidechains: vec![metadata.into()],
        // }))
    }

    async fn invoke_read_method(
        &self,
        request: Request<rpc::InvokeReadMethodRequest>,
    ) -> Result<Response<rpc::InvokeReadMethodResponse>, Status> {
        dbg!(&request);
        let request = request.into_inner();
        let asset_public_key = PublicKey::from_bytes(&request.asset_public_key)
            .map_err(|err| Status::invalid_argument(format!("Asset public key was not a valid public key:{}", err)))?;
        if let Some(state) = self
            .db_factory
            .get_state_db(&asset_public_key)
            .map_err(|e| Status::internal(format!("Could not create state db: {}", e)))?
        {
            let mut unit_of_work = state.new_unit_of_work();
            let response_bytes = self
                .asset_processor
                .invoke_read_method(
                    TemplateId::from(request.template_id),
                    request.method,
                    &request.args,
                    &mut unit_of_work,
                )
                .map_err(|e| Status::internal(format!("Could not invoke read method: {}", e)))?;
            Ok(Response::new(rpc::InvokeReadMethodResponse {
                result: response_bytes,
                authority: Some(rpc::Authority {
                    node_public_key: vec![],
                    signature: vec![],
                    proxied_by: None,
                }),
            }))
        } else {
            // Forward to proxy
            let response_bytes = self
                .asset_proxy
                .invoke_read_method(
                    &asset_public_key,
                    TemplateId::from(request.template_id),
                    request.method,
                    request.args,
                )
                .await
                .map_err(|err| Status::internal(format!("Error calling proxied method:{}", err)))?;
            // TODO: Populate authority
            Ok(Response::new(rpc::InvokeReadMethodResponse {
                result: response_bytes,
                authority: Some(rpc::Authority {
                    node_public_key: vec![],
                    signature: vec![],
                    proxied_by: Some(vec![]),
                }),
            }))
        }
    }
}
