// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::sync::Arc;

use axum::{routing::get, Extension, Router};
use log::{error, info};
use tari_core::{base_node::rpc::BaseNodeWalletQueryService, chain_storage::BlockchainBackend};
use tari_shutdown::ShutdownSignal;
use thiserror::Error;
use tokio::{io, net::TcpListener};

use crate::http::handler;

const LOG_TARGET: &str = "c::bn::rpc::http::server";

#[derive(Debug, Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    IO(#[from] io::Error),
}

pub struct Server<S> {
    port: u16,
    query_service: Arc<S>,
    shutdown_signal: ShutdownSignal,
}

impl<S: BaseNodeWalletQueryService> Server<S> {
    pub fn new(port: u16, query_service: S, shutdown_signal: ShutdownSignal) -> Self {
        Self {
            port,
            query_service: Arc::new(query_service),
            shutdown_signal,
        }
    }

    pub async fn start<B: BlockchainBackend + 'static>(&self) -> Result<(), Error> {
        let shutdown_signal = self.shutdown_signal.clone();
        let port = self.port;
        let router = Router::new()
            .route("/get_tip_info", get(handler::get_tip_info::handle::<B>))
            .route("/get_header_by_height", get(handler::get_header_by_height::handle::<B>))
            .route("/get_height_at_time", get(handler::get_height_at_time::handle::<B>))
            .layer(Extension(self.query_service.clone()));
        let listener = TcpListener::bind(format!("0.0.0.0:{port}")).await?;

        // spawn server
        tokio::spawn(async move {
            info!(target: LOG_TARGET, "Wallet query HTTP server listening at 0.0.0.0:{port}");
            if let Err(error) = axum::serve(listener, router)
                .with_graceful_shutdown(shutdown_signal)
                .await
            {
                error!(target: LOG_TARGET, "Wallet query HTTP server error: {}", error);
            }
        });

        Ok(())
    }
}
