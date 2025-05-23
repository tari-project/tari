// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use tari_core::{
    base_node::{
        rpc::{query_service, BaseNodeWalletQueryService},
        StateMachineHandle,
    },
    chain_storage::{async_db::AsyncBlockchainDb, BlockchainBackend},
};
use tari_shutdown::ShutdownSignal;

pub mod handler;

pub mod server;

pub fn create_base_node_wallet_http_server<B: BlockchainBackend + 'static>(
    port: u16,
    db: AsyncBlockchainDb<B>,
    state_machine: StateMachineHandle,
    shutdown_signal: ShutdownSignal,
) -> server::Server<impl BaseNodeWalletQueryService> {
    server::Server::new(port, query_service::Service::new(db, state_machine), shutdown_signal)
}
