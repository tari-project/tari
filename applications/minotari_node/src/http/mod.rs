// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use tari_comms::{peer_manager::NodeId, types::CommsPublicKey};
use tari_core::{
    base_node::{
        rpc::{query_service, BaseNodeWalletQueryService},
        StateMachineHandle,
    },
    chain_storage::{async_db::AsyncBlockchainDb, BlockchainBackend},
    mempool::service::MempoolHandle,
};
use tari_shutdown::ShutdownSignal;

pub mod handler;

pub mod server;

pub fn create_base_node_wallet_http_server<B: BlockchainBackend + 'static>(
    port: u16,
    db: AsyncBlockchainDb<B>,
    state_machine: StateMachineHandle,
    mempool: MempoolHandle,
    shutdown_signal: ShutdownSignal,
    node_id: NodeId,
    public_key: CommsPublicKey,
) -> server::Server<impl BaseNodeWalletQueryService> {
    server::Server::new(
        port,
        query_service::Service::new(db, state_machine, mempool.clone(), node_id, public_key),
        mempool,
        shutdown_signal,
    )
}
