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
mod asset;
mod cli;
mod cmd_args;
mod comms;
mod config;
mod dan_node;
mod debug;
mod default_service_specification;
mod grpc;
mod monitoring;
mod p2p;

use std::{
    fs::File,
    io::BufReader,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::Path,
    process,
    sync::Arc,
};

use clap::Parser;
use futures::FutureExt;
use log::*;
use tari_app_grpc::tari_rpc::validator_node_server::ValidatorNodeServer;
use tari_app_utilities::identity_management::setup_node_identity;
use tari_common::{
    exit_codes::{ExitCode, ExitError},
    load_configuration,
};
use tari_comms::{peer_manager::PeerFeatures, NodeIdentity};
use tari_comms_dht::Dht;
use tari_dan_core::{
    models::AssetDefinition,
    services::{
        BaseLayerCommitteeManager,
        ConcreteAssetProcessor,
        ConcreteAssetProxy,
        MempoolServiceHandle,
        ServiceSpecification,
        StaticListCommitteeManager,
    },
};
use tari_dan_storage_sqlite::SqliteDbFactory;
use tari_p2p::comms_connector::SubscriptionFactory;
use tari_service_framework::ServiceHandles;
use tari_shutdown::{Shutdown, ShutdownSignal};
use tokio::{runtime, runtime::Runtime, task};
use tonic::transport::Server;

use crate::{
    cli::Cli,
    config::{ApplicationConfig, ValidatorNodeConfig},
    dan_node::{DanNode, RunningServiceSpecification},
    debug::{debug_definition::DebugDefinition, debug_service_specification::DebugServiceSpecification},
    default_service_specification::DefaultServiceSpecification,
    grpc::{services::base_node_client::GrpcBaseNodeClient, validator_node_grpc_server::ValidatorNodeGrpcServer},
    p2p::services::rpc_client::TariCommsValidatorNodeClientFactory,
};

const LOG_TARGET: &str = "tari::validator_node::app";

fn main() {
    // Uncomment to enable tokio tracing via tokio-console
    // console_subscriber::init();

    if let Err(err) = main_inner() {
        let exit_code = err.exit_code;
        eprintln!("{:?}", err);
        error!(
            target: LOG_TARGET,
            "Exiting with code ({}): {:?}", exit_code as i32, exit_code
        );
        process::exit(exit_code as i32);
    }
}

fn main_inner() -> Result<(), ExitError> {
    let cli = Cli::parse();
    let config_path = cli.common.config_path();
    let cfg = load_configuration(config_path, true, &cli.config_property_overrides())?;

    let config = ApplicationConfig::load_from(&cfg)?;
    let runtime = build_runtime()?;
    runtime.block_on(run_node(&config, cli))?;

    Ok(())
}

async fn run_node(config: &ApplicationConfig, cli: Cli) -> Result<(), ExitError> {
    let shutdown = Shutdown::new();

    let node_identity = setup_node_identity(
        &config.validator_node.identity_file,
        config.validator_node.public_address.as_ref(),
        true,
        PeerFeatures::NONE,
    )?;
    let db_factory = SqliteDbFactory::new(config.validator_node.data_dir.clone());
    let mempool_service = MempoolServiceHandle::default();

    if let Some(debug_file) = cli.debug_file.as_ref().map(|dd| dd.as_path()) {
        start_debug_mode(debug_file, node_identity, shutdown, db_factory, mempool_service, config).await?;
    } else {
        start_non_debug_mode(node_identity, shutdown, db_factory, mempool_service, config).await?;
    }

    Ok(())
}

async fn start_debug_mode(
    debug_file: &Path,
    node_identity: Arc<NodeIdentity>,
    shutdown: Shutdown,
    db_factory: SqliteDbFactory,
    mempool_service: MempoolServiceHandle,
    config: &ApplicationConfig,
) -> Result<(), ExitError> {
    info!(target: LOG_TARGET, "Debugging file definition");
    let file = File::open(debug_file).expect("File does not exist");
    let reader = BufReader::new(file);
    let definition: DebugDefinition = serde_json::from_reader(reader).expect("Not a valid definition");

    let (handles, subscription_factory) = comms::build_service_and_comms_stack(
        config,
        shutdown.to_signal(),
        node_identity.clone(),
        mempool_service.clone(),
        db_factory.clone(),
        ConcreteAssetProcessor::default(),
    )
    .await?;

    let committee_manager = StaticListCommitteeManager::new(definition.committee.clone(), AssetDefinition {
        public_key: definition.public_key,
        phase_timeout: 30,
        base_layer_confirmation_time: 1,
        checkpoint_unique_id: vec![],
        initial_state: Default::default(),
        template_parameters: vec![],
    });
    let asset_processor = ConcreteAssetProcessor::default();
    let validator_node_client_factory =
        TariCommsValidatorNodeClientFactory::new(handles.expect_handle::<Dht>().dht_requester());
    let asset_proxy: ConcreteAssetProxy<DebugServiceSpecification> = ConcreteAssetProxy::new(
        committee_manager.clone(),
        validator_node_client_factory,
        5,
        mempool_service.clone(),
        db_factory.clone(),
    );

    let grpc_server: ValidatorNodeGrpcServer<DebugServiceSpecification> = ValidatorNodeGrpcServer::new(
        node_identity.as_ref().clone(),
        db_factory.clone(),
        asset_processor,
        asset_proxy,
    );
    let grpc_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 18144);

    task::spawn(run_grpc(grpc_server, grpc_addr, shutdown.to_signal()));
    println!("🚀 Validator node started!");
    println!("{}", node_identity);
    run_dan_node::<DebugServiceSpecification>(
        shutdown.to_signal(),
        config.validator_node.clone(),
        mempool_service,
        committee_manager,
        db_factory,
        handles,
        subscription_factory,
        node_identity,
    )
    .await?;
    Ok(())
}

async fn start_non_debug_mode(
    node_identity: Arc<NodeIdentity>,
    shutdown: Shutdown,
    db_factory: SqliteDbFactory,
    mempool_service: MempoolServiceHandle,
    config: &ApplicationConfig,
) -> Result<(), ExitError> {
    info!(
        target: LOG_TARGET,
        "Node starting with pub key: {}, node_id: {}",
        node_identity.public_key(),
        node_identity.node_id()
    );
    let (handles, subscription_factory) = comms::build_service_and_comms_stack(
        config,
        shutdown.to_signal(),
        node_identity.clone(),
        mempool_service.clone(),
        db_factory.clone(),
        ConcreteAssetProcessor::default(),
    )
    .await?;

    let asset_processor = ConcreteAssetProcessor::default();
    let validator_node_client_factory =
        TariCommsValidatorNodeClientFactory::new(handles.expect_handle::<Dht>().dht_requester());
    let asset_proxy: ConcreteAssetProxy<DefaultServiceSpecification> = ConcreteAssetProxy::new(
        BaseLayerCommitteeManager::new(GrpcBaseNodeClient::new(config.validator_node.base_node_grpc_address)),
        validator_node_client_factory,
        5,
        mempool_service.clone(),
        db_factory.clone(),
    );

    let grpc_server: ValidatorNodeGrpcServer<DefaultServiceSpecification> = ValidatorNodeGrpcServer::new(
        node_identity.as_ref().clone(),
        db_factory.clone(),
        asset_processor,
        asset_proxy,
    );
    let grpc_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 18144);

    task::spawn(run_grpc(grpc_server, grpc_addr, shutdown.to_signal()));
    println!("🚀 Validator node started!");
    println!("{}", node_identity);

    let mut base_node_client = GrpcBaseNodeClient::new(config.validator_node.base_node_grpc_address);
    run_dan_node::<DefaultServiceSpecification>(
        shutdown.to_signal(),
        config.validator_node.clone(),
        mempool_service,
        BaseLayerCommitteeManager::new(base_node_client),
        db_factory,
        handles,
        subscription_factory,
        node_identity,
    )
    .await?;
    Ok(())
}

fn build_runtime() -> Result<Runtime, ExitError> {
    let mut builder = runtime::Builder::new_multi_thread();
    builder
        .enable_all()
        .build()
        .map_err(|e| ExitError::new(ExitCode::UnknownError, e))
}

async fn run_dan_node<TSpecification: RunningServiceSpecification>(
    shutdown_signal: ShutdownSignal,
    config: ValidatorNodeConfig,
    mempool_service: MempoolServiceHandle,
    committee_manager: TSpecification::CommitteeManager,
    db_factory: SqliteDbFactory,
    handles: ServiceHandles,
    subscription_factory: SubscriptionFactory,
    node_identity: Arc<NodeIdentity>,
) -> Result<(), ExitError> {
    let node = DanNode::new(config);
    node.start::<TSpecification>(
        shutdown_signal,
        node_identity,
        mempool_service,
        committee_manager,
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
            error!(target: LOG_TARGET, "GRPC encountered an error: {}", err);
            err
        })?;

    info!(target: LOG_TARGET, "Stopping GRPC");
    Ok(())
}
