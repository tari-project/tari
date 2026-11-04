//  Copyright 2022. The Tari Project
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

use log::*;
use tari_common_types::grpc_authentication::{GrpcAuthentication, GrpcCredential};
use tari_utilities::SafePassword;
use tonic::{Request, Status, codegen::http::header::AUTHORIZATION, service::Interceptor};

use crate::authentication::{BasicAuthCredentials, salted_password::create_salted_hashed_password};

const LOG_TARGET: &str = "applications::minotari_app_grpc::authentication";

#[derive(Debug, Clone)]
pub struct ServerAuthenticationInterceptor {
    /// In-memory prepared credentials for constant-time comparison. Configured passwords are provided in plaintext
    /// and salted in-memory at startup for verification.
    credentials: Vec<BasicAuthCredentials>,
    is_none: bool,
}

impl ServerAuthenticationInterceptor {
    /// Creates a new interceptor from a list of credentials (configured as plaintext).
    pub fn from_credentials(credentials: Vec<GrpcCredential>) -> Option<Self> {
        let mut prepared = Vec::with_capacity(credentials.len());
        for cred in credentials {
            let phc_password: SafePassword = create_salted_hashed_password(cred.password.reveal())
                .ok()?
                .as_str()
                .into();
            let basic_cred = BasicAuthCredentials::new(cred.username, phc_password).ok()?;
            prepared.push(basic_cred);
        }

        Some(Self {
            is_none: prepared.is_empty(),
            credentials: prepared,
        })
    }

    /// Creates a new interceptor from legacy `GrpcAuthentication`.
    pub fn new(auth: GrpcAuthentication) -> Option<Self> {
        match auth {
            GrpcAuthentication::None => Some(Self {
                credentials: Vec::new(),
                is_none: true,
            }),
            GrpcAuthentication::Basic { .. } => Self::from_credentials(auth.to_credentials()),
        }
    }

    #[allow(clippy::result_large_err)]
    fn handle_basic_auth(&self, mut req: Request<()>) -> Result<Request<()>, Status> {
        match req.metadata().get(AUTHORIZATION.as_str()) {
            Some(t) => {
                // Parse the provided header
                let header = t.to_str().map_err(unauthenticated)?;
                let (header_username, header_password) =
                    BasicAuthCredentials::parse_header(header).map_err(unauthenticated)?;

                // Validate the header credentials across all configured credentials in constant time
                let matched_username = BasicAuthCredentials::constant_time_validate_multi(
                    &self.credentials,
                    &header_username,
                    &header_password,
                )
                .map_err(unauthenticated)?;

                req.extensions_mut().insert(matched_username);
                Ok(req)
            },
            _ => Err(unauthenticated("Missing authorization header")),
        }
    }
}

impl Interceptor for ServerAuthenticationInterceptor {
    fn call(&mut self, request: Request<()>) -> Result<Request<()>, Status> {
        if self.is_none {
            Ok(request)
        } else {
            self.handle_basic_auth(request)
        }
    }
}

/// Standard unauthenticated response
fn unauthenticated<E: ToString>(err: E) -> Status {
    warn!(target: LOG_TARGET, "GRPC authentication failed: {}", err.to_string());
    Status::unauthenticated("Auth failed")
}

#[cfg(test)]
mod tests {
    use tonic::service::Interceptor;

    use super::*;

    #[test]
    fn server_interceptor_accepts_multiple_credentials_and_inserts_username() {
        let creds = vec![
            GrpcCredential::new("miner", "miner_pass".to_string().into()),
            GrpcCredential::new("monitor", "monitor_pass".to_string().into()),
        ];
        let mut interceptor = ServerAuthenticationInterceptor::from_credentials(creds).unwrap();

        // Calling with miner credentials
        let header_miner = BasicAuthCredentials::generate_header("miner", b"miner_pass").unwrap();
        let mut req_miner = Request::new(());
        req_miner.metadata_mut().insert(AUTHORIZATION.as_str(), header_miner);

        let validated_req = interceptor.call(req_miner).unwrap();
        let username_ext = validated_req.extensions().get::<String>().unwrap();
        assert_eq!(username_ext, "miner");

        // Calling with monitor credentials
        let header_monitor = BasicAuthCredentials::generate_header("monitor", b"monitor_pass").unwrap();
        let mut req_monitor = Request::new(());
        req_monitor
            .metadata_mut()
            .insert(AUTHORIZATION.as_str(), header_monitor);

        let validated_req2 = interceptor.call(req_monitor).unwrap();
        let username_ext2 = validated_req2.extensions().get::<String>().unwrap();
        assert_eq!(username_ext2, "monitor");

        // Calling with wrong password
        let header_wrong = BasicAuthCredentials::generate_header("miner", b"wrong").unwrap();
        let mut req_wrong = Request::new(());
        req_wrong.metadata_mut().insert(AUTHORIZATION.as_str(), header_wrong);

        let err = interceptor.call(req_wrong).unwrap_err();
        assert_eq!(err.code(), tonic::Code::Unauthenticated);
    }

    #[test]
    fn server_interceptor_none_passes_through() {
        let mut interceptor = ServerAuthenticationInterceptor::new(GrpcAuthentication::None).unwrap();
        let req = Request::new(());
        assert!(interceptor.call(req).is_ok());
    }
}
