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

use std::{sync::Arc, time::Duration};

use log::{error, info};
use tari_common::exit_codes::{ExitCode, ExitError};
use tari_common_types::types::Signature;
use tari_comms::NodeIdentity;
use tari_dan_core::services::{BaseNodeClient, WalletClient};
use tokio::{task, time};

use crate::{
    config::ValidatorNodeConfig,
    grpc::services::{base_node_client::GrpcBaseNodeClient, wallet_client::GrpcWalletClient},
};

const _LOG_TARGET: &str = "tari::validator_node::app";

#[derive(Clone)]
pub struct DanNode {
    config: ValidatorNodeConfig,
    identity: Arc<NodeIdentity>,
}

impl DanNode {
    pub fn new(config: ValidatorNodeConfig, identity: Arc<NodeIdentity>) -> Self {
        Self { config, identity }
    }

    pub async fn start(&self) -> Result<(), ExitError> {
        let mut base_node_client = GrpcBaseNodeClient::new(self.config.base_node_grpc_address);
        let mut last_tip = 0u64;
        let node = self.clone();

        if self.config.constitution_auto_accept {
            task::spawn(async move {
                loop {
                    if let Ok(metadata) = base_node_client.get_tip_info().await {
                        last_tip = metadata.height_of_longest_chain;
                    }

                    match node
                        .find_and_accept_constitutions(base_node_client.clone(), last_tip)
                        .await
                    {
                        Ok(()) => info!("Contracts accepted"),
                        Err(e) => error!("Contracts not accepted because {:?}", e),
                    }

                    time::sleep(Duration::from_secs(
                        node.config.constitution_management_polling_interval,
                    ))
                    .await;
                }
            });
        }

        loop {
            // other work here

            time::sleep(Duration::from_secs(120)).await;
        }
    }

    async fn find_and_accept_constitutions(
        &self,
        mut base_node_client: GrpcBaseNodeClient,
        last_tip: u64,
    ) -> Result<(), ExitError> {
        let mut wallet_client = GrpcWalletClient::new(self.config.wallet_grpc_address);

        let outputs = base_node_client
            .get_constitutions(self.identity.public_key().clone())
            .await
            .map_err(|e| ExitError::new(ExitCode::DigitalAssetError, &e))?;

        for output in outputs {
            if let Some(sidechain_features) = output.features.sidechain_features {
                let contract_id = sidechain_features.contract_id;
                // TODO: expect will crash the validator node if the base node misbehaves
                let constitution = sidechain_features.constitution.expect("Constitution wasn't present");

                if constitution.acceptance_requirements.acceptance_period_expiry < last_tip {
                    let signature = Signature::default();

                    match wallet_client
                        .submit_contract_acceptance(&contract_id, self.identity.public_key(), &signature)
                        .await
                    {
                        Ok(tx_id) => info!("Accepted with id={}", tx_id),
                        Err(_) => error!("Did not accept the contract acceptance"),
                    };
                };
            }
        }

        Ok(())
    }
}
