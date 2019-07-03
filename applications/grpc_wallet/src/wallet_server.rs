// Copyright 2019. The Tari Project
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

use crate::grpc_interface::{wallet_rpc::server, WalletRPC};
use derive_error::Error;
use futures::{future::Future, stream::Stream};
use hyper::server::conn::Http;
use log::*;
use std::{net::AddrParseError, sync::Arc};
use tari_utilities::message_format::MessageFormatError;
use tari_wallet::Wallet;
use tokio::net::TcpListener;
use tower_hyper::Server;

const LOG_TARGET: &'static str = "applications::grpc_wallet";

#[derive(Debug, Error)]
pub enum WalletServerError {
    AddrParseError(AddrParseError),
    IoError(std::io::Error),
    MessageFormatError(MessageFormatError),
}

pub struct WalletServerConfig {
    pub port: u32,
}

impl Default for WalletServerConfig {
    fn default() -> Self {
        Self { port: 50051 }
    }
}

/// Instance of the Wallet RPC Server with a reference to the Wallet API and the config
pub struct WalletServer {
    // TODO some form of authentication
    config: WalletServerConfig,
    wallet: Arc<Wallet>,
}

impl WalletServer {
    pub fn new(config: Option<WalletServerConfig>, wallet: Arc<Wallet>) -> WalletServer {
        WalletServer {
            config: config.unwrap_or(WalletServerConfig::default()),
            wallet,
        }
    }

    pub fn start(self) -> Result<(), WalletServerError> {
        let new_service = server::WalletRpcServer::new(WalletRPC {
            wallet: self.wallet.clone(),
        });

        let mut server = Server::new(new_service);

        let http = Http::new().http2_only(true).clone();
        let addr = format!("127.0.0.1:{}", self.config.port);
        let bind = TcpListener::bind(&addr.clone().as_str().parse()?)?;
        let serve = bind
            .incoming()
            .for_each(move |sock| {
                if let Err(e) = sock.set_nodelay(true) {
                    return Err(e);
                }

                let serve = server.serve_with(sock, http.clone());
                tokio::spawn(serve.map_err(|e| error!("Error starting Hyper service: {:?}", e)));

                Ok(())
            })
            .map_err(|e| error!("Error accepting request: {:?}", e));
        info!(target: LOG_TARGET, "Starting Wallet gRPC Server at {}", addr);
        tokio::run(serve);
        Ok(())
    }
}
