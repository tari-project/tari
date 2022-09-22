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

use tari_common_types::grpc_authentication::GrpcAuthentication;
use tonic::{
    codegen::http::header::AUTHORIZATION,
    metadata::{Ascii, MetadataValue},
    service::Interceptor,
    Request,
    Status,
};

use crate::authentication::{BasicAuthCredentials, BasicAuthError};

#[derive(Debug, Clone)]
pub struct ClientAuthenticationInterceptor {
    authorization_header: Option<MetadataValue<Ascii>>,
}

impl ClientAuthenticationInterceptor {
    pub fn create(auth: &GrpcAuthentication) -> Result<Self, BasicAuthError> {
        let authorization_header = match auth {
            GrpcAuthentication::None => None,
            GrpcAuthentication::Basic { username, password } => {
                Some(BasicAuthCredentials::generate_header(username, password.reveal())?)
            },
        };
        Ok(Self { authorization_header })
    }
}

impl Interceptor for ClientAuthenticationInterceptor {
    fn call(&mut self, mut request: Request<()>) -> Result<Request<()>, Status> {
        match self.authorization_header.clone() {
            Some(authorization_header) => {
                request
                    .metadata_mut()
                    .insert(AUTHORIZATION.as_str(), authorization_header);
                Ok(request)
            },
            None => Ok(request),
        }
    }
}
