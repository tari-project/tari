// Copyright 2022, The Tari Project
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

use std::ops::{Deref, DerefMut};

use tari_common::MAX_GRPC_MESSAGE_SIZE;
use tonic::{
    codegen::{InterceptedService, http::uri::InvalidUri},
    transport::{Certificate, ClientTlsConfig, Endpoint},
};

pub type Client<TTransport> = grpc::wallet_client::WalletClient<TTransport>;

// Re-export so that users don't have to depend on minotari_app_grpc
pub use minotari_app_grpc::{
    authentication::{BasicAuthError, ClientAuthenticationInterceptor},
    tari_rpc as grpc,
};
pub use tari_common_types::grpc_authentication::GrpcAuthentication;

#[derive(Debug, Clone)]
pub struct WalletGrpcClient<TTransport> {
    client: Client<InterceptedService<TTransport, ClientAuthenticationInterceptor>>,
}

#[derive(Debug, Clone, Default)]
pub struct WalletGrpcClientOptions {
    pub tls_domain_name: Option<String>,
    pub ca_certificate: Option<Vec<u8>>,
    pub allow_insecure_auth: bool,
}

impl WalletGrpcClient<tonic::transport::Channel> {
    pub async fn connect(addr: &str) -> Result<Self, WalletClientError> {
        Self::connect_with_options(addr, &GrpcAuthentication::None, WalletGrpcClientOptions::default()).await
    }

    pub async fn connect_with_options(
        addr: &str,
        auth_config: &GrpcAuthentication,
        options: WalletGrpcClientOptions,
    ) -> Result<Self, WalletClientError> {
        minotari_app_grpc::tls::validate_client_transport(
            addr,
            options.tls_domain_name.as_deref(),
            auth_config,
            options.allow_insecure_auth,
        )
        .map_err(WalletClientError::TransportConfiguration)?;
        let uri: tonic::codegen::http::Uri = addr
            .parse()
            .map_err(|e| WalletClientError::TransportConfiguration(format!("invalid address URI: {e}")))?;
        let is_http = uri.scheme_str().is_some_and(|s| s.eq_ignore_ascii_case("http"));
        if options.ca_certificate.is_some() && options.tls_domain_name.is_none() && is_http {
            return Err(WalletClientError::TransportConfiguration(
                "a CA certificate requires an https:// address; the address scheme is not rewritten".to_string(),
            ));
        }
        let mut endpoint = Endpoint::new(addr.to_string())?;
        if options.tls_domain_name.is_some() || options.ca_certificate.is_some() {
            let mut tls = ClientTlsConfig::new();
            if let Some(domain) = options.tls_domain_name {
                tls = tls.domain_name(domain);
            }
            if let Some(ca) = options.ca_certificate {
                tls = tls.ca_certificate(Certificate::from_pem(ca));
            }
            endpoint = endpoint.tls_config(tls)?;
        }
        let channel = endpoint.connect().await?;
        Ok(Self {
            client: Client::<tonic::transport::Channel>::with_interceptor(
                channel,
                ClientAuthenticationInterceptor::create(auth_config)?,
            )
            .max_encoding_message_size(MAX_GRPC_MESSAGE_SIZE)
            .max_decoding_message_size(MAX_GRPC_MESSAGE_SIZE),
        })
    }

    pub async fn connect_with_auth(addr: &str, auth_config: &GrpcAuthentication) -> Result<Self, WalletClientError> {
        Self::connect_with_options(addr, auth_config, WalletGrpcClientOptions::default()).await
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
    #[error("invalid gRPC client transport: {0}")]
    TransportConfiguration(String),
}
