use std::{net::SocketAddr, sync::Arc};

use axum::{routing::get, Extension, Router};
use log::{error, info};
use tari_shutdown::ShutdownSignal;
use thiserror::Error;
use tokio::{io, net::TcpListener};

use crate::{
    base_node::rpc::{http::handler, BaseNodeWalletQueryService},
    chain_storage::BlockchainBackend,
};

const LOG_TARGET: &str = "c::bn::rpc::http::server";

#[derive(Debug, Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    IO(#[from] io::Error),
}

pub struct Server<S> {
    listen_address: SocketAddr,
    query_service: Arc<S>,
    shutdown_signal: ShutdownSignal,
}

impl<S: BaseNodeWalletQueryService> Server<S> {
    pub fn new(listen_address: SocketAddr, query_service: S, shutdown_signal: ShutdownSignal) -> Self {
        Self {
            listen_address,
            query_service: Arc::new(query_service),
            shutdown_signal,
        }
    }

    pub async fn start<B: BlockchainBackend + 'static>(&self) -> Result<(), Error> {
        let shutdown_signal = self.shutdown_signal.clone();
        let listen_address = self.listen_address;
        let router = Router::new()
            .route("/get_tip_info", get(handler::get_tip_info::handle::<B>))
            .route("/get_header_by_height", get(handler::get_header_by_height::handle::<B>))
            .layer(Extension(self.query_service.clone()));
        let listener = TcpListener::bind(self.listen_address).await?;

        // spawn server
        tokio::spawn(async move {
            info!(target: LOG_TARGET, "HTTP server listening at {}", listen_address);
            if let Err(error) = axum::serve(listener, router)
                .with_graceful_shutdown(shutdown_signal)
                .await
            {
                error!(target: LOG_TARGET, "HTTP server error: {}", error);
            }
        });

        Ok(())
    }
}
