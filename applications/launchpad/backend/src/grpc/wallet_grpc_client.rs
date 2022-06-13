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

use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4},
    time::Duration,
};

use bollard::container::Stats;
use futures::{stream, Stream, StreamExt, TryStreamExt};
use log::info;
use tari_app_grpc::tari_rpc::{
    wallet_client::WalletClient,
    TransactionEvent,
    TransactionEventRequest,
    TransactionEventResponse, GetIdentityResponse, GetIdentityRequest,
};
use tonic::transport::Channel;

use super::error::GrpcError;
use crate::{
    docker::{DockerWrapperError, LaunchpadConfig, WALLET_GRPC_ADDRESS_URL},
    error::LauncherError,
};

type Inner = WalletClient<tonic::transport::Channel>;

pub struct GrpcWalletClient {
    inner: Option<Inner>,
}

impl GrpcWalletClient {
    pub fn new() -> GrpcWalletClient {
        Self { inner: None }
    }

    pub async fn connection(&mut self) -> Result<&mut Inner, GrpcError> {
        if self.inner.is_none() {
            let inner = Inner::connect(WALLET_GRPC_ADDRESS_URL).await?;
            self.inner = Some(inner);
        }
        self.inner
            .as_mut()
            .ok_or_else(|| GrpcError::FatalError("no connection".into()))
    }

    pub async fn stream(&mut self) -> Result<impl Stream<Item = TransactionEventResponse>, GrpcError> {
        let inner = self.connection().await?;
        let request = TransactionEventRequest {};
        let response = inner.stream_transaction_events(request).await.unwrap().into_inner();
        Ok(response.map(|e| e.unwrap()))
    }

    pub async fn identity(&mut self) -> Result<GetIdentityResponse, GrpcError> {
        let inner = self.connection().await?;
        let request = GetIdentityRequest {};
        let identity = inner.identify(request).await?;
        Ok(identity.into_inner())
    }
}
