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

#![allow(clippy::too_many_arguments)]
mod cmd_args;
mod comms;
mod dan_node;
mod default_service_specification;
mod grpc;
mod p2p;

use std::{
    fs,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    process,
    sync::Arc,
};

use futures::FutureExt;
use log::*;
use tari_app_grpc::tari_rpc::validator_node_server::ValidatorNodeServer;
use tari_app_utilities::{identity_management::setup_node_identity, initialization::init_configuration};
use tari_common::{configuration::bootstrap::ApplicationType, exit_codes::ExitCodes, GlobalConfig};
use tari_comms::{connectivity::ConnectivityRequester, peer_manager::PeerFeatures, NodeIdentity};
use tari_comms_dht::Dht;
use tari_dan_core::services::{ConcreteAssetProcessor, ConcreteAssetProxy, MempoolServiceHandle, ServiceSpecification};
use tari_dan_storage_sqlite::SqliteDbFactory;
use tari_p2p::comms_connector::SubscriptionFactory;
use tari_service_framework::ServiceHandles;
use tari_shutdown::{Shutdown, ShutdownSignal};
use tokio::{runtime, runtime::Runtime, task};
use tonic::transport::Server;

use crate::{
    dan_node::DanNode,
    default_service_specification::DefaultServiceSpecification,
    grpc::{services::base_node_client::GrpcBaseNodeClient, validator_node_grpc_server::ValidatorNodeGrpcServer},
    p2p::services::rpc_client::TariCommsValidatorNodeClientFactory,
};

const LOG_TARGET: &str = "tari::validator_node::app";

fn main() {
    // console_subscriber::init();
    if let Err(exit_code) = main_inner() {
        eprintln!("{:?}", exit_code);
        error!(
            target: LOG_TARGET,
            "Exiting with code ({}): {:?}",
            exit_code.as_i32(),
            exit_code
        );
        process::exit(exit_code.as_i32());
    }
}

fn main_inner() -> Result<(), ExitCodes> {
    let (bootstrap, config, _) = init_configuration(ApplicationType::ValidatorNode)?;

    // let _operation_mode = cmd_args::get_operation_mode();
    // match operation_mode {
    //     OperationMode::Run => {
    let runtime = build_runtime()?;
    runtime.block_on(run_node(config, bootstrap.create_id))?;
    // }
    // }

    Ok(())
}

async fn run_node(config: GlobalConfig, create_id: bool) -> Result<(), ExitCodes> {
    let shutdown = Shutdown::new();

    fs::create_dir_all(&config.peer_db_path).map_err(|err| ExitCodes::ConfigError(err.to_string()))?;
    let node_identity = setup_node_identity(
        &config.base_node_identity_file,
        &config.public_address,
        create_id,
        PeerFeatures::NONE,
    )?;
    let db_factory = SqliteDbFactory::new(&config);
    let mempool_service = MempoolServiceHandle::default();
    info!(
        target: LOG_TARGET,
        "Node starting with pub key: {}, node_id: {}",
        node_identity.public_key(),
        node_identity.node_id()
    );
    let (handles, subscription_factory) = comms::build_service_and_comms_stack(
        &config,
        shutdown.to_signal(),
        node_identity.clone(),
        mempool_service.clone(),
        db_factory.clone(),
        ConcreteAssetProcessor::default(),
    )
    .await?;

    let asset_processor = ConcreteAssetProcessor::default();
    let validator_node_client_factory = TariCommsValidatorNodeClientFactory::new(
        handles.expect_handle::<ConnectivityRequester>(),
        handles.expect_handle::<Dht>().discovery_service_requester(),
    );
    let asset_proxy: ConcreteAssetProxy<DefaultServiceSpecification> = ConcreteAssetProxy::new(
        GrpcBaseNodeClient::new(config.validator_node.clone().unwrap().base_node_grpc_address),
        validator_node_client_factory,
        5,
        mempool_service.clone(),
        db_factory.clone(),
    );

    let grpc_server: ValidatorNodeGrpcServer<DefaultServiceSpecification> =
        ValidatorNodeGrpcServer::new(db_factory.clone(), asset_processor, asset_proxy);
    let grpc_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 18144);

    task::spawn(run_grpc(grpc_server, grpc_addr, shutdown.to_signal()));
    run_dan_node(
        shutdown.to_signal(),
        config,
        mempool_service,
        db_factory,
        handles,
        subscription_factory,
        node_identity,
    )
    .await?;
    Ok(())
}

fn build_runtime() -> Result<Runtime, ExitCodes> {
    let mut builder = runtime::Builder::new_multi_thread();
    builder
        .enable_all()
        .build()
        .map_err(|e| ExitCodes::UnknownError(e.to_string()))
}

async fn run_dan_node(
    shutdown_signal: ShutdownSignal,
    config: GlobalConfig,
    mempool_service: MempoolServiceHandle,
    db_factory: SqliteDbFactory,
    handles: ServiceHandles,
    subscription_factory: SubscriptionFactory,
    node_identity: Arc<NodeIdentity>,
) -> Result<(), ExitCodes> {
    let node = DanNode::new(config);
    node.start(
        shutdown_signal,
        node_identity,
        mempool_service,
        db_factory,
        handles,
        subscription_factory,
    )
    .await
}

async fn run_grpc<TServiceSpecification: ServiceSpecification + 'static>(
    grpc_server: ValidatorNodeGrpcServer<TServiceSpecification>,
    grpc_address: SocketAddr,
    shutdown_signal: ShutdownSignal,
) -> Result<(), anyhow::Error> {
    info!(target: LOG_TARGET, "Starting GRPC on {}", grpc_address);

    Server::builder()
        .add_service(ValidatorNodeServer::new(grpc_server))
        .serve_with_shutdown(grpc_address, shutdown_signal.map(|_| ()))
        .await
        .map_err(|err| {
            error!(target: LOG_TARGET, "GRPC encountered an  error:{}", err);
            err
        })?;

    info!(target: LOG_TARGET, "Stopping GRPC");
    Ok(())
}
