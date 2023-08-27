// Copyright 2022, The Taiji Project
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

use std::{
    ops::{Deref, DerefMut},
    str::FromStr,
};

use tonic::{
    codegen::{http::uri::InvalidUri, InterceptedService},
    transport::Endpoint,
};

pub type Client<TTransport> = grpc::wallet_client::WalletClient<TTransport>;

// Re-export so that users don't have to depend on minotaiji_app_grpc
pub use minotaiji_app_grpc::{
    authentication::{BasicAuthError, ClientAuthenticationInterceptor},
    taiji_rpc as grpc,
};
pub use taiji_common_types::grpc_authentication::GrpcAuthentication;

#[derive(Debug, Clone)]
pub struct WalletGrpcClient<TTransport> {
    client: Client<InterceptedService<TTransport, ClientAuthenticationInterceptor>>,
}

impl WalletGrpcClient<tonic::transport::Channel> {
    pub async fn connect(addr: &str) -> Result<Self, WalletClientError> {
        let channel = Endpoint::from_str(addr)?.connect().await?;
        Ok(Self {
            client: Client::<tonic::transport::Channel>::with_interceptor(
                channel,
                ClientAuthenticationInterceptor::create(&GrpcAuthentication::None)?,
            ),
        })
    }

    pub async fn connect_with_auth(addr: &str, auth_config: &GrpcAuthentication) -> Result<Self, WalletClientError> {
        let channel = Endpoint::from_str(addr)?.connect().await?;
        Ok(Self {
            client: Client::<tonic::transport::Channel>::with_interceptor(
                channel,
                ClientAuthenticationInterceptor::create(auth_config)?,
            ),
        })
    }
}

impl<TTransport> Deref for WalletGrpcClient<TTransport> {
    type Target = Client<InterceptedService<TTransport, ClientAuthenticationInterceptor>>;

    fn deref(&self) -> &Self::Target {
        &self.client
    }
}

impl<TTransport> DerefMut for WalletGrpcClient<TTransport> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.client
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WalletClientError {
    #[error(transparent)]
    InvalidUri(#[from] InvalidUri),
    #[error(transparent)]
    TransportError(#[from] tonic::transport::Error),
    #[error(transparent)]
    AuthenticationError(#[from] BasicAuthError),
}
