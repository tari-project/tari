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

use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4};

use bollard::container::Stats;
use futures::{
    channel::mpsc::{self, Sender},
    stream,
    Stream,
    StreamExt,
    TryStreamExt,
};
use log::*;
use tari_app_grpc::tari_rpc::{
    base_node_client::BaseNodeClient,
    wallet_client::WalletClient,
    Empty,
    GetBalanceRequest,
    GetBalanceResponse,
    GetIdentityRequest,
    GetIdentityResponse,
    NodeIdentity,
    TransactionEvent,
    TransactionEventRequest,
    TransactionEventResponse,
};
use tauri::{async_runtime::block_on, http::status};
use tokio::{
    task,
    time::{sleep, Duration},
};
use tonic::transport::Channel;

use super::{error::GrpcError, BlockStateInfo};
use crate::{
    docker::{DockerWrapperError, LaunchpadConfig, BASE_NODE_GRPC_ADDRESS_URL},
    error::LauncherError,
    grpc::{SyncProgress, SyncProgressInfo},
};

const LOG_TARGET: &str = "tari_launchpad::base_node_grpc";
type Inner = BaseNodeClient<tonic::transport::Channel>;

#[derive(Clone)]
pub struct GrpcBaseNodeClient {
    inner: Option<Inner>,
}

impl GrpcBaseNodeClient {
    pub fn new() -> GrpcBaseNodeClient {
        Self { inner: None }
    }

    pub async fn try_connect(&mut self) -> Result<&mut Inner, GrpcError> {
        if self.inner.is_none() {
            let inner = Inner::connect(BASE_NODE_GRPC_ADDRESS_URL).await?;
            self.inner = Some(inner);
        }
        self.inner
            .as_mut()
            .ok_or_else(|| GrpcError::FatalError("no connection".into()))
    }

    pub async fn wait_for_connection(&mut self) {
        loop {
            match self.try_connect().await {
                Ok(_) => {
                    info!(target: LOG_TARGET, "#### Connected....");
                    break;
                },
                Err(_) => {
                    sleep(Duration::from_secs(3)).await;
                    info!(target: LOG_TARGET, "---> Waiting for base node....");
                },
            }
        }
    }

    pub async fn stream(&mut self) -> Result<impl Stream<Item = SyncProgressInfo>, GrpcError> {
        let (mut sender, receiver) = mpsc::channel(100);
        let connection = self.try_connect().await?.clone();
        task::spawn(async move {
            let mut progress = SyncProgress::new(0, 100);
            loop {
                let request = Empty {};
                let response = match connection.clone().get_sync_progress(request).await {
                    Ok(response) => response.into_inner(),
                    Err(status) => {
                        error!(target: LOG_TARGET, "Failed reading progress from base node: {}", status);
                        return;
                    },
                };
                let done = matches!(response.state(), tari_app_grpc::tari_rpc::SyncState::Done);
                debug!(target: LOG_TARGET, "Response: {:?}", response);
                progress.update(response);
                if let Err(err) = sender.try_send(progress.progress_info()) {
                    warn!(
                        target: LOG_TARGET,
                        "Could not send progress to tokio_stream. {}",
                        err.to_string()
                    );
                }
                if done {
                    info!(target: LOG_TARGET, "Blockchain has synced.");
                    break;
                }
                sleep(Duration::from_secs(10)).await;
            }
            info!(target: LOG_TARGET, "Closing blockchain sync stream.");
        });
        Ok(receiver)
    }

    pub async fn identity(&mut self) -> Result<NodeIdentity, GrpcError> {
        let connection = self.try_connect().await?.clone();
        let request = Empty {};
        let identity = connection.clone().identify(request).await?;
        Ok(identity.into_inner())
    }
}
