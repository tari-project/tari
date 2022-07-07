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

use std::sync::Arc;

use tari_common::exit_codes::{ExitCode, ExitError};
use tari_comms::NodeIdentity;
use tari_dan_core::{
    services::{ConcreteAcceptanceManager, MempoolServiceHandle},
    storage::global::GlobalDb,
};
use tari_dan_storage_sqlite::{global::SqliteGlobalDbBackendAdapter, SqliteDbFactory};
use tari_p2p::comms_connector::SubscriptionFactory;
use tari_service_framework::ServiceHandles;
use tari_shutdown::ShutdownSignal;

use crate::{
    config::ValidatorNodeConfig,
    contract_worker_manager::ContractWorkerManager,
    grpc::services::{base_node_client::GrpcBaseNodeClient, wallet_client::GrpcWalletClient},
};

const _LOG_TARGET: &str = "tari::validator_node::app";

#[derive(Clone)]
pub struct DanNode {
    config: ValidatorNodeConfig,
    identity: Arc<NodeIdentity>,
    global_db: GlobalDb<SqliteGlobalDbBackendAdapter>,
}

impl DanNode {
    pub fn new(
        config: ValidatorNodeConfig,
        identity: Arc<NodeIdentity>,
        global_db: GlobalDb<SqliteGlobalDbBackendAdapter>,
    ) -> Self {
        Self {
            config,
            identity,
            global_db,
        }
    }

    pub async fn start(
        &self,
        shutdown: ShutdownSignal,
        mempool_service: MempoolServiceHandle,
        db_factory: SqliteDbFactory,
        handles: ServiceHandles,
        subscription_factory: SubscriptionFactory,
    ) -> Result<(), ExitError> {
        let base_node_client = GrpcBaseNodeClient::new(self.config.base_node_grpc_address);
        let wallet_client = GrpcWalletClient::new(self.config.wallet_grpc_address);
        let acceptance_manager = ConcreteAcceptanceManager::new(wallet_client, base_node_client.clone());
        let workers = ContractWorkerManager::new(
            self.config.clone(),
            self.identity.clone(),
            self.global_db.clone(),
            base_node_client,
            acceptance_manager,
            mempool_service,
            handles,
            subscription_factory,
            db_factory,
            shutdown.clone(),
        );

        workers
            .start()
            .await
            .map_err(|err| ExitError::new(ExitCode::DigitalAssetError, err))?;

        Ok(())
    }
}
