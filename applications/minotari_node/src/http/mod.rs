// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

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

mod cache_config;
pub mod server;
pub use cache_config::HttpCacheConfig;

pub fn create_base_node_wallet_http_server<B: BlockchainBackend + 'static>(
    port: u16,
    db: AsyncBlockchainDb<B>,
    state_machine: StateMachineHandle,
    mempool: MempoolHandle,
    shutdown_signal: ShutdownSignal,
    cache_cfg: HttpCacheConfig,
) -> server::Server<impl BaseNodeWalletQueryService> {
    server::Server::new(
        port,
        query_service::Service::new(db, state_machine, mempool.clone()),
        mempool,
        shutdown_signal,
        cache_cfg,
    )
}
