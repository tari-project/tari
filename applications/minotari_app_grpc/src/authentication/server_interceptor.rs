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
use tonic::{codegen::http::header::AUTHORIZATION, service::Interceptor, Request, Status};

use crate::authentication::BasicAuthCredentials;

const LOG_TARGET: &str = "applications::minotari_app_grpc::authentication";

#[derive(Debug)]
pub struct ServerAuthenticationInterceptor {
    auth: GrpcAuthentication,
}

impl ServerAuthenticationInterceptor {
    pub fn new(auth: GrpcAuthentication) -> Self {
        Self { auth }
    }

    fn handle_basic_auth(
        &self,
        req: Request<()>,
        valid_username: &str,
        valid_password: &[u8],
    ) -> Result<Request<()>, Status> {
        match req.metadata().get(AUTHORIZATION.as_str()) {
            Some(t) => {
                let val = t.to_str().map_err(unauthenticated)?;
                let credentials = BasicAuthCredentials::from_header(val).map_err(unauthenticated)?;
                credentials
                    .validate(valid_username, valid_password)
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
                password: hashed_password,
            } => self.handle_basic_auth(request, username, hashed_password.reveal()),
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
