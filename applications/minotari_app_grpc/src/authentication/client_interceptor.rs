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

#[cfg(test)]
mod test {
    use tari_common_types::grpc_authentication::GrpcAuthentication;
    use tari_utilities::SafePassword;
    use tonic::{codegen::http::header::AUTHORIZATION, service::Interceptor, Request};

    use super::ClientAuthenticationInterceptor;
    use crate::authentication::ServerAuthenticationInterceptor;

    #[test]
    fn test_no_authentication_success() {
        // Set up the client and server interceptors using no authentication
        let auth = GrpcAuthentication::None;
        let mut client_interceptor = ClientAuthenticationInterceptor::create(&auth).unwrap();
        let mut server_interceptor = ServerAuthenticationInterceptor::new(auth).unwrap();

        // Create a dummy client request and process it through the client interceptor
        let client_request = client_interceptor.call(Request::new(())).unwrap();
        assert!(client_request.metadata().is_empty());

        // Process the request through the server interceptor
        assert!(server_interceptor.call(client_request).is_ok());
    }

    #[test]
    fn test_basic_authentication_success() {
        // Set up the client and server interceptors using basic authentication with matching credentials
        let auth = GrpcAuthentication::Basic {
            username: "foo".to_string(),
            password: SafePassword::from("bar"),
        };
        let mut client_interceptor = ClientAuthenticationInterceptor::create(&auth).unwrap();
        let mut server_interceptor = ServerAuthenticationInterceptor::new(auth).unwrap();

        // Create a dummy client request and process it through the client interceptor
        let client_request = client_interceptor.call(Request::new(())).unwrap();
        assert!(client_request.metadata().contains_key(AUTHORIZATION.as_str()));

        // Process the request through the server interceptor
        assert!(server_interceptor.call(client_request).is_ok());
    }

    #[test]
    fn test_basic_authentication_failure() {
        // Set up the client and server interceptors using basic authentication with mismatched credentials
        let client_auth = GrpcAuthentication::Basic {
            username: "foo".to_string(),
            password: SafePassword::from("evil"),
        };
        let server_auth = GrpcAuthentication::Basic {
            username: "foo".to_string(),
            password: SafePassword::from("bar"),
        };
        let mut client_interceptor = ClientAuthenticationInterceptor::create(&client_auth).unwrap();
        let mut server_interceptor = ServerAuthenticationInterceptor::new(server_auth).unwrap();

        // Create a dummy client request and process it through the client interceptor
        let client_request = client_interceptor.call(Request::new(())).unwrap();
        assert!(client_request.metadata().contains_key(AUTHORIZATION.as_str()));

        // Process the request through the server interceptor
        assert!(server_interceptor.call(client_request).is_err());
    }

    #[test]
    fn test_mixed_authentication_success() {
        // Set up the client and server interceptors using mismatched authentication
        let client_auth = GrpcAuthentication::Basic {
            username: "foo".to_string(),
            password: SafePassword::from("bar"),
        };
        let server_auth = GrpcAuthentication::None;
        let mut client_interceptor = ClientAuthenticationInterceptor::create(&client_auth).unwrap();
        let mut server_interceptor = ServerAuthenticationInterceptor::new(server_auth).unwrap();

        // Create a dummy client request and process it through the client interceptor
        let client_request = client_interceptor.call(Request::new(())).unwrap();
        assert!(client_request.metadata().contains_key(AUTHORIZATION.as_str()));

        // Process the request through the server interceptor; this mismatch is allowed
        assert!(server_interceptor.call(client_request).is_ok());
    }

    #[test]
    fn test_mixed_authentication_failure() {
        // Set up the client and server interceptors using mismatched authentication
        let client_auth = GrpcAuthentication::None;
        let server_auth = GrpcAuthentication::Basic {
            username: "foo".to_string(),
            password: SafePassword::from("bar"),
        };
        let mut client_interceptor = ClientAuthenticationInterceptor::create(&client_auth).unwrap();
        let mut server_interceptor = ServerAuthenticationInterceptor::new(server_auth).unwrap();

        // Create a dummy client request and process it through the client interceptor
        let client_request = client_interceptor.call(Request::new(())).unwrap();
        assert!(client_request.metadata().is_empty());

        // Process the request through the server interceptor; this mismatch is not allowed
        assert!(server_interceptor.call(client_request).is_err());
    }
}
