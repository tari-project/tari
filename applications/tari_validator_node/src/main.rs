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

mod cmd_args;
mod dan_layer;
mod digital_assets_error;
mod grpc;
mod p2p;
mod types;

use crate::grpc::validator_node_grpc_server::ValidatorNodeGrpcServer;
use anyhow;
use futures::FutureExt;
use log::*;
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    process,
};
use tari_shutdown::{Shutdown, ShutdownSignal};
use thiserror::Error;
use tokio::{runtime, task};
use tokio_stream::StreamExt;
use tonic::transport::Server;

use crate::{
    cmd_args::OperationMode,
    dan_layer::{
        dan_node::DanNode,
        services::{ConcreteMempoolService, MempoolService, MempoolServiceHandle},
    },
    grpc::validator_node_rpc::validator_node_server::ValidatorNodeServer,
};
use std::sync::{Arc, Mutex};
use tari_app_utilities::{initialization::init_configuration, utilities::ExitCodes};
use tari_common::{configuration::bootstrap::ApplicationType, GlobalConfig};
use tokio::runtime::Runtime;

const LOG_TARGET: &str = "dan_node::app";

fn main() {
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
    let (bootstrap, node_config, _) = init_configuration(ApplicationType::DanNode)?;

    // let operation_mode = cmd_args::get_operation_mode();
    // match operation_mode {
    //     OperationMode::Run => {
    let mut runtime = build_runtime()?;
    runtime.block_on(run_node(node_config))?;
    // }
    // }

    Ok(())
}

async fn run_node(config: GlobalConfig) -> Result<(), ExitCodes> {
    let shutdown = Shutdown::new();

    let mempool_service = MempoolServiceHandle::new(Arc::new(Mutex::new(ConcreteMempoolService::new())));

    let grpc_server = ValidatorNodeGrpcServer::new(mempool_service.clone());
    let grpc_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 18080);
    // task::spawn(run_grpc(grpc_server, grpc_addr,  shutdown.to_signal()));

    task::spawn(run_grpc(grpc_server, grpc_addr, shutdown.to_signal()));
    run_dan_node(shutdown.to_signal(), config, mempool_service).await?;
    Ok(())
}

fn build_runtime() -> Result<Runtime, ExitCodes> {
    let mut builder = runtime::Builder::new_multi_thread();
    builder.enable_all().build().map_err(|e| ExitCodes::UnknownError)
}

async fn run_dan_node<TMempoolService: MempoolService + Clone + Send>(
    shutdown_signal: ShutdownSignal,
    config: GlobalConfig,
    mempool_service: TMempoolService,
) -> Result<(), ExitCodes> {
    let node = DanNode::new(config);
    node.start(true, shutdown_signal, mempool_service).await
}

async fn run_grpc<TMempoolService: MempoolService + Clone + Sync + Send + 'static>(
    grpc_server: ValidatorNodeGrpcServer<TMempoolService>,
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
