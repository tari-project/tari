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
use tari_common_types::grpc_authentication::GrpcAuthentication;
use tari_utilities::SafePassword;
use tonic::{codegen::http::header::AUTHORIZATION, service::Interceptor, Request, Status};

use crate::authentication::{salted_password::create_salted_hashed_password, BasicAuthCredentials};

const LOG_TARGET: &str = "applications::minotari_app_grpc::authentication";

#[derive(Debug)]
pub struct ServerAuthenticationInterceptor {
    auth: GrpcAuthentication, // this contains a hashed PHC password in the case of basic authentication
}

impl ServerAuthenticationInterceptor {
    pub fn new(auth: GrpcAuthentication) -> Option<Self> {
        // The server password needs to be hashed for later client verification
        let processed_auth = match auth {
            GrpcAuthentication::None => auth,
            GrpcAuthentication::Basic { username, password } => GrpcAuthentication::Basic {
                username,
                password: create_salted_hashed_password(password.reveal()).ok()?.as_str().into(),
            },
        };

        Some(Self { auth: processed_auth })
    }

    fn handle_basic_auth(
        &self,
        req: Request<()>,
        valid_username: &str,
        valid_phc_password: &SafePassword,
    ) -> Result<Request<()>, Status> {
        match req.metadata().get(AUTHORIZATION.as_str()) {
            Some(t) => {
                // Parse the provided header
                let header = t.to_str().map_err(unauthenticated)?;
                let (header_username, header_password) =
                    BasicAuthCredentials::parse_header(header).map_err(unauthenticated)?;

                // Validate the header credentials against the valid credentials
                let valid_credentials =
                    BasicAuthCredentials::new(valid_username.to_owned(), valid_phc_password.clone())
                        .map_err(unauthenticated)?;
                valid_credentials
                    .constant_time_validate(&header_username, &header_password)
                    .map_err(unauthenticated)?;

                Ok(req)
            },
            _ => Err(unauthenticated("Missing authorization header")),
        }
    }
}

impl Interceptor for ServerAuthenticationInterceptor {
    fn call(&mut self, request: Request<()>) -> Result<Request<()>, Status> {
        match &self.auth {
            GrpcAuthentication::None => Ok(request),
            GrpcAuthentication::Basic {
                username,
                password: phc_password,
            } => self.handle_basic_auth(request, username, phc_password),
        }
    }
}

impl Clone for ServerAuthenticationInterceptor {
    fn clone(&self) -> Self {
        Self {
            auth: self.auth.clone(),
        }
    }
}

/// Standard unauthenticated response
fn unauthenticated<E: ToString>(err: E) -> Status {
    warn!(target: LOG_TARGET, "GRPC authentication failed: {}", err.to_string());
    Status::unauthenticated("Auth failed")
}
