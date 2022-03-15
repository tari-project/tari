//  Copyright 2021, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that
// the  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the
// following  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED
// WARRANTIES,  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A
// PARTICULAR PURPOSE ARE  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY
// DIRECT, INDIRECT, INCIDENTAL,  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO,
// PROCUREMENT OF SUBSTITUTE GOODS OR  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
// CAUSED AND ON ANY THEORY OF LIABILITY,  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR
// OTHERWISE) ARISING IN ANY WAY OUT OF THE  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH
// DAMAGE.
use std::convert::{TryFrom, TryInto};

use log::*;
use tari_common_types::types::PublicKey;
use tari_comms::{
    protocol::rpc::{Request, Response, RpcStatus, Streaming},
    utils,
};
use tari_crypto::tari_utilities::ByteArray;
use tari_dan_core::{
    models::{Instruction, TreeNodeHash},
    services::{AssetProcessor, MempoolService},
    storage::{state::StateDbUnitOfWorkReader, DbFactory},
};
use tokio::{sync::mpsc, task};

const LOG_TARGET: &str = "vn::p2p::rpc";

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
impl<TMempoolService, TDbFactory, TAssetProcessor> ValidatorNodeRpcService
    for ValidatorNodeRpcServiceImpl<TMempoolService, TDbFactory, TAssetProcessor>
where
    TMempoolService: MempoolService + Clone,
    TDbFactory: DbFactory + Clone,
    TAssetProcessor: AssetProcessor + Clone,
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

        let unit_of_work = state.reader();

        let instruction = Instruction::new(
            request
                .template_id
                .try_into()
                .map_err(|_| RpcStatus::bad_request("Invalid template_id"))?,
            request.method,
            request.args,
        );
        let response_bytes = self
            .asset_processor
            .invoke_read_method(&instruction, &unit_of_work)
            .map_err(|e| RpcStatus::general(format!("Could not invoke read method: {}", e)))?;

        Ok(Response::new(proto::InvokeReadMethodResponse {
            result: response_bytes.unwrap_or_default(),
        }))
    }

    async fn invoke_method(
        &self,
        request: Request<proto::InvokeMethodRequest>,
    ) -> Result<Response<proto::InvokeMethodResponse>, RpcStatus> {
        dbg!(&request);
        let request = request.into_message();
        let instruction = Instruction::new(
            request
                .template_id
                .try_into()
                .map_err(|_| RpcStatus::bad_request("Invalid template_id"))?,
            request.method.clone(),
            request.args.clone(),
            /* TokenId(request.token_id.clone()),
             * TODO: put signature in here
             * ComSig::default()
             * create_com_sig_from_bytes(&request.signature)
             *     .map_err(|err| Status::invalid_argument("signature was not a valid comsig"))?, */
        );
        debug!(target: LOG_TARGET, "Submitting instruction {} to mempool", instruction);
        let mut mempool_service = self.mempool_service.clone();
        match mempool_service.submit_instruction(instruction).await {
            Ok(_) => {
                debug!(target: LOG_TARGET, "Accepted instruction into mempool");
                return Ok(Response::new(proto::InvokeMethodResponse {
                    result: vec![],
                    status: proto::Status::Accepted as i32,
                }));
            },
            Err(err) => {
                debug!(target: LOG_TARGET, "Mempool rejected instruction: {}", err);
                return Ok(Response::new(proto::InvokeMethodResponse {
                    result: vec![],
                    status: proto::Status::Errored as i32,
                }));
            },
        }
    }

    async fn get_sidechain_blocks(
        &self,
        request: Request<proto::GetSidechainBlocksRequest>,
    ) -> Result<Streaming<proto::GetSidechainBlocksResponse>, RpcStatus> {
        let msg = request.into_message();

        let asset_public_key = PublicKey::from_bytes(&msg.asset_public_key)
            .map_err(|_| RpcStatus::bad_request("Invalid asset_public_key"))?;
        let start_hash =
            TreeNodeHash::try_from(msg.start_hash).map_err(|_| RpcStatus::bad_request("Invalid start hash"))?;

        let end_hash = Some(msg.end_hash)
            .filter(|h| !h.is_empty())
            .map(TreeNodeHash::try_from)
            .transpose()
            .map_err(|_| RpcStatus::bad_request("Invalid end_hash"))?;

        let db = self
            .db_factory
            .get_chain_db(&asset_public_key)
            .map_err(RpcStatus::log_internal_error(LOG_TARGET))?
            .ok_or_else(|| RpcStatus::not_found("Asset not found"))?;

        let start_block = db
            .find_sidechain_block_by_node_hash(&start_hash)
            .map_err(RpcStatus::log_internal_error(LOG_TARGET))?
            .ok_or_else(|| RpcStatus::not_found(format!("Block not found with start_hash '{}'", start_hash)))?;

        let end_block_exists = end_hash
            .as_ref()
            .map(|end_hash| db.sidechain_block_exists(end_hash))
            .transpose()
            .map_err(RpcStatus::log_internal_error(LOG_TARGET))?;

        if !end_block_exists.unwrap_or(true) {
            return Err(RpcStatus::not_found(format!(
                "Block not found with end_hash '{}'",
                end_hash.unwrap_or_else(TreeNodeHash::zero)
            )));
        }

        let (tx, rx) = mpsc::channel(2);

        task::spawn(async move {
            let mut current_block_hash = *start_block.node().hash();
            if tx
                .send(Ok(proto::GetSidechainBlocksResponse {
                    block: Some(start_block.into()),
                }))
                .await
                .is_err()
            {
                return;
            }
            loop {
                match db.find_sidechain_block_by_parent_node_hash(&current_block_hash) {
                    Ok(Some(block)) => {
                        current_block_hash = *block.node().hash();
                        if tx
                            .send(Ok(proto::GetSidechainBlocksResponse {
                                block: Some(block.into()),
                            }))
                            .await
                            .is_err()
                        {
                            return;
                        }
                        if end_hash.map(|h| h == current_block_hash).unwrap_or(false) {
                            return;
                        }
                    },
                    Ok(None) => return,
                    Err(err) => {
                        error!(target: LOG_TARGET, "Failure while streaming blocks: {}", err);
                        let _ = tx.send(Err(RpcStatus::general("Internal database failure"))).await;
                        return;
                    },
                }
            }
        });

        Ok(Streaming::new(rx))
    }

    async fn get_sidechain_state(
        &self,
        request: Request<proto::GetSidechainStateRequest>,
    ) -> Result<Streaming<proto::GetSidechainStateResponse>, RpcStatus> {
        let msg = request.into_message();

        let asset_public_key = PublicKey::from_bytes(&msg.asset_public_key)
            .map_err(|_| RpcStatus::bad_request("Invalid asset_public_key"))?;

        let db = self
            .db_factory
            .get_state_db(&asset_public_key)
            .map_err(RpcStatus::log_internal_error(LOG_TARGET))?
            .ok_or_else(|| RpcStatus::not_found("Asset not found"))?;

        let uow = db.reader();
        let data = uow.get_all_state().map_err(RpcStatus::log_internal_error(LOG_TARGET))?;
        let (tx, rx) = mpsc::channel(10);

        task::spawn(async move {
            for state in data {
                let schema = proto::GetSidechainStateResponse {
                    state: Some(proto::get_sidechain_state_response::State::Schema(state.name)),
                };

                if tx.send(Ok(schema)).await.is_err() {
                    return;
                }

                let key_values = state
                    .items
                    .into_iter()
                    .map(|kv| proto::get_sidechain_state_response::State::KeyValue(kv.into()))
                    .map(|state| Ok(proto::GetSidechainStateResponse { state: Some(state) }));

                if utils::mpsc::send_all(&tx, key_values).await.is_err() {
                    return;
                }
            }
        });

        Ok(Streaming::new(rx))
    }

    async fn get_op_logs(
        &self,
        request: Request<proto::GetStateOpLogsRequest>,
    ) -> Result<Response<proto::GetStateOpLogsResponse>, RpcStatus> {
        let msg = request.into_message();

        let asset_public_key = PublicKey::from_bytes(&msg.asset_public_key)
            .map_err(|_| RpcStatus::bad_request("Invalid asset_public_key"))?;

        let db = self
            .db_factory
            .get_state_db(&asset_public_key)
            .map_err(RpcStatus::log_internal_error(LOG_TARGET))?
            .ok_or_else(|| RpcStatus::not_found("Asset not found"))?;

        let reader = db.reader();
        let op_logs = reader
            .get_op_logs_for_height(msg.height)
            .map_err(RpcStatus::log_internal_error(LOG_TARGET))?;

        let resp = proto::GetStateOpLogsResponse {
            op_logs: op_logs.into_iter().map(Into::into).collect(),
        };

        Ok(Response::new(resp))
    }

    async fn get_tip_node(
        &self,
        request: Request<proto::GetTipNodeRequest>,
    ) -> Result<Response<proto::GetTipNodeResponse>, RpcStatus> {
        let msg = request.into_message();

        let asset_public_key = PublicKey::from_bytes(&msg.asset_public_key)
            .map_err(|_| RpcStatus::bad_request("Invalid asset_public_key"))?;

        let db = self
            .db_factory
            .get_chain_db(&asset_public_key)
            .map_err(RpcStatus::log_internal_error(LOG_TARGET))?
            .ok_or_else(|| RpcStatus::not_found("Asset not found"))?;

        let tip_node = db.get_tip_node().map_err(RpcStatus::log_internal_error(LOG_TARGET))?;

        let resp = proto::GetTipNodeResponse {
            tip_node: tip_node.map(Into::into),
        };

        Ok(Response::new(resp))
    }
}
