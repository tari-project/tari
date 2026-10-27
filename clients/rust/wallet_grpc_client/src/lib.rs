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
    tls::{GrpcTlsValidationError, validate_client_grpc_tls_and_auth},
};
pub use tari_common_types::grpc_authentication::GrpcAuthentication;

#[derive(Debug, Clone)]
pub struct WalletClientTlsConfig {
    pub ca_certificate_pem: Vec<u8>,
    pub domain_name: String,
}

impl WalletClientTlsConfig {
    pub fn new(ca_certificate_pem: impl Into<Vec<u8>>, domain_name: impl Into<String>) -> Self {
        Self {
            ca_certificate_pem: ca_certificate_pem.into(),
            domain_name: domain_name.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct WalletGrpcClient<TTransport> {
    client: Client<InterceptedService<TTransport, ClientAuthenticationInterceptor>>,
}

impl WalletGrpcClient<tonic::transport::Channel> {
    pub async fn connect(addr: &str) -> Result<Self, WalletClientError> {
        Self::connect_with_options(addr, &GrpcAuthentication::None, None, false).await
    }

    pub async fn connect_with_auth(addr: &str, auth_config: &GrpcAuthentication) -> Result<Self, WalletClientError> {
        Self::connect_with_options(addr, auth_config, None, false).await
    }

    pub async fn connect_with_tls(
        addr: &str,
        auth_config: &GrpcAuthentication,
        ca_certificate_pem: impl Into<Vec<u8>>,
        domain_name: &str,
    ) -> Result<Self, WalletClientError> {
        let tls_config = WalletClientTlsConfig::new(ca_certificate_pem, domain_name);
        Self::connect_with_options(addr, auth_config, Some(&tls_config), false).await
    }

    pub async fn connect_with_options(
        addr: &str,
        auth_config: &GrpcAuthentication,
        tls_config: Option<&WalletClientTlsConfig>,
        allow_insecure_auth: bool,
    ) -> Result<Self, WalletClientError> {
        let endpoint = Self::build_endpoint_with_tls_and_auth(addr, auth_config, tls_config, allow_insecure_auth)?;
        let interceptor = ClientAuthenticationInterceptor::create(auth_config)?;
        let channel = endpoint.connect().await?;
        Ok(Self {
            client: Client::<tonic::transport::Channel>::with_interceptor(channel, interceptor)
                .max_encoding_message_size(MAX_GRPC_MESSAGE_SIZE)
                .max_decoding_message_size(MAX_GRPC_MESSAGE_SIZE),
        })
    }

    pub fn build_endpoint_with_tls_and_auth(
        addr: &str,
        auth_config: &GrpcAuthentication,
        tls_config: Option<&WalletClientTlsConfig>,
        allow_insecure_auth: bool,
    ) -> Result<Endpoint, WalletClientError> {
        validate_client_grpc_tls_and_auth(
            addr,
            tls_config.map(|t| t.domain_name.as_str()),
            auth_config,
            allow_insecure_auth,
        )?;
        let mut endpoint = Endpoint::new(addr.to_string())?;
        if let Some(tls_cfg) = tls_config {
            let ca = Certificate::from_pem(tls_cfg.ca_certificate_pem.clone());
            let tls = ClientTlsConfig::new()
                .ca_certificate(ca)
                .domain_name(&tls_cfg.domain_name);
            endpoint = endpoint.tls_config(tls)?;
        }
        Ok(endpoint)
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
    TlsValidationError(#[from] GrpcTlsValidationError),
    #[error(transparent)]
    TransportError(#[from] tonic::transport::Error),
    #[error(transparent)]
    AuthenticationError(#[from] BasicAuthError),
}

#[cfg(test)]
mod tests {
    use minotari_app_grpc::tls::certs::generate_self_signed_certs;

    use super::*;

    #[test]
    fn can_construct_endpoint_with_ca_certificate_and_domain_name() {
        let (ca_pem, _server_cert, _server_key) = generate_self_signed_certs().unwrap();
        let tls_config = WalletClientTlsConfig::new(ca_pem, "localhost");
        WalletGrpcClient::build_endpoint_with_tls_and_auth(
            "https://127.0.0.1:18143",
            &GrpcAuthentication::None,
            Some(&tls_config),
            false,
        )
        .expect("https:// endpoint with CA certificate and domain name should build");
    }

    #[tokio::test]
    async fn connect_with_tls_rejects_http_scheme() {
        let (ca_pem, _server_cert, _server_key) = generate_self_signed_certs().unwrap();
        let err = WalletGrpcClient::connect_with_tls(
            "http://127.0.0.1:18143",
            &GrpcAuthentication::None,
            ca_pem,
            "localhost",
        )
        .await
        .expect_err("connect_with_tls on http:// must fail before dialing");
        assert!(err.to_string().contains("http://"));
        assert!(err.to_string().contains("https://"));
    }
}
