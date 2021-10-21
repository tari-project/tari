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
use crate::{
    dan_layer::{
        models::{Instruction, TokenId},
        services::MempoolService,
    },
    grpc::validator_node_rpc as rpc,
    types::{ComSig, PublicKey},
};

use tari_crypto::tari_utilities::ByteArray;

use tonic::{Request, Response, Status};

pub struct ValidatorNodeGrpcServer<TMempoolService: MempoolService> {
    mempool_service: TMempoolService,
}

impl<TMempoolService: MempoolService> ValidatorNodeGrpcServer<TMempoolService> {
    pub fn new(mempool_service: TMempoolService) -> Self {
        Self { mempool_service }
    }
}

#[tonic::async_trait]
impl<TMempoolService: MempoolService + Clone + Sync + Send + 'static> rpc::validator_node_server::ValidatorNode
    for ValidatorNodeGrpcServer<TMempoolService>
{
    async fn get_token_data(
        &self,
        request: tonic::Request<rpc::GetTokenDataRequest>,
    ) -> Result<tonic::Response<rpc::GetTokenDataResponse>, tonic::Status> {
        dbg!(&request);
        Err(Status::internal("Oh noes"))
    }

    async fn execute_instruction(
        &self,
        request: Request<rpc::ExecuteInstructionRequest>,
    ) -> Result<Response<rpc::ExecuteInstructionResponse>, Status> {
        dbg!(&request);
        let request = request.into_inner();
        let instruction = Instruction::new(
            PublicKey::from_bytes(&request.asset_public_key)
                .map_err(|_err| Status::invalid_argument("asset_public_key was not a valid public key"))?,
            request.method.clone(),
            request.args.clone(),
            TokenId(request.token_id.clone()),
            // TODO: put signature in here
            ComSig::default()
            // create_com_sig_from_bytes(&request.signature)
            //     .map_err(|err| Status::invalid_argument("signature was not a valid comsig"))?,
        );

        let mut mempool_service = self.mempool_service.clone();
        match mempool_service.submit_instruction(instruction) {
            Ok(_) => {
                return Ok(Response::new(rpc::ExecuteInstructionResponse {
                    status: "Accepted".to_string(),
                }))
            },
            Err(_) => {
                return Ok(Response::new(rpc::ExecuteInstructionResponse {
                    status: "Errored".to_string(),
                }))
            },
        }
    }
}
