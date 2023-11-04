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

use std::{borrow::Cow, ops::Deref, string::FromUtf8Error};

use argon2::{password_hash::Encoding, Argon2, PasswordHash, PasswordVerifier};
use tari_utilities::SafePassword;
use tonic::metadata::{errors::InvalidMetadataValue, Ascii, MetadataValue};
use zeroize::{Zeroize, Zeroizing};

/// Implements [RFC 2617](https://www.ietf.org/rfc/rfc2617.txt#:~:text=The%20%22basic%22%20authentication%20scheme%20is,other%20realms%20on%20that%20server.)
/// Represents the username and password contained within a Authenticate header.
#[derive(Debug)]
pub struct BasicAuthCredentials {
    pub user_name: String,
    pub password: SafePassword,
}

impl BasicAuthCredentials {
    pub fn new(user_name: String, password: SafePassword) -> Self {
        Self { user_name, password }
    }

    /// Creates a `Credentials` instance from a base64 `String`
    /// which must encode user credentials as `username:password`
    pub fn decode(auth_header_value: &str) -> Result<Self, BasicAuthError> {
        let decoded = base64::decode(auth_header_value)?;
        let as_utf8 = Zeroizing::new(String::from_utf8(decoded)?);

        if let Some((user_name, password)) = as_utf8.split_once(':') {
            let credentials = Self::new(user_name.into(), password.to_string().into());
            return Ok(credentials);
        }

        Err(BasicAuthError::InvalidAuthorizationHeader)
    }

    /// Creates a `Credentials` instance from an HTTP Authorization header
    /// which schema is a valid `Basic` HTTP Authorization Schema.
    pub fn from_header(auth_header: &str) -> Result<BasicAuthCredentials, BasicAuthError> {
        // check if its a valid basic auth header
        let (auth_type, encoded_credentials) = auth_header
            .split_once(' ')
            .ok_or(BasicAuthError::InvalidAuthorizationHeader)?;

        if encoded_credentials.contains(' ') {
            // Invalid authorization token received
            return Err(BasicAuthError::InvalidAuthorizationHeader);
        }

        // Check the provided authorization header
        // to be a "Basic" authorization header
        if auth_type.to_lowercase() != "basic" {
            return Err(BasicAuthError::InvalidScheme(auth_type.to_string()));
        }

        let credentials = BasicAuthCredentials::decode(encoded_credentials)?;
        Ok(credentials)
    }

    pub fn validate(&self, username: &str, password: &[u8]) -> Result<(), BasicAuthError> {
        // We should always validate both username and passphrase to avoid leaking where a failure occurs
        let mut validated = true;

        // First check the username
        if self.user_name.as_bytes() != username.as_bytes() {
            validated = false;
        }

        // Now validate the passphrase
        // Note that it's safe to fail early on corrupt data
        let bytes = self.password.reveal().to_vec();
        let str_password =
            Zeroizing::new(String::from_utf8(bytes).map_err(|_| BasicAuthError::InvalidAuthorizationHeader)?);
        let header_password = PasswordHash::parse(&str_password, Encoding::B64)
            .map_err(|_| BasicAuthError::InvalidAuthorizationHeader)?;
        if Argon2::default().verify_password(password, &header_password).is_err() {
            validated = false;
        }

        // Now return whether the entire username/passphrase validation succeeded or failed
        if validated {
            Ok(())
        } else {
            Err(BasicAuthError::InvalidUsernameOrPassphrase)
        }
    }

    pub fn generate_header(username: &str, password: &[u8]) -> Result<MetadataValue<Ascii>, BasicAuthError> {
        let password_str = String::from_utf8_lossy(password);
        let token_str = Zeroizing::new(format!("{}:{}", username, password_str));
        let mut token = base64::encode(token_str.deref());
        let header = format!("Basic {}", token);
        token.zeroize();
        match password_str {
            Cow::Borrowed(_) => {},
            Cow::Owned(mut owned) => owned.zeroize(),
        }
        let header = header.parse()?;
        Ok(header)
    }
}

/// Authorization Header Error
#[derive(Debug, thiserror::Error)]
pub enum BasicAuthError {
    #[error("Invalid username or passphrase")]
    InvalidUsernameOrPassphrase,
    #[error("The HTTP Authorization header value is invalid")]
    InvalidAuthorizationHeader,
    #[error("The HTTP Authorization header contains an invalid scheme {0} but only `Basic` is supported")]
    InvalidScheme(String),
    #[error("The value expected as a base64 encoded `String` is not encoded correctly: {0}")]
    InvalidBase64Value(#[from] base64::DecodeError),
    #[error("The provided binary is not a valid UTF-8 character: {0}")]
    InvalidUtf8Value(#[from] FromUtf8Error),
    #[error("Invalid header value: {0}")]
    InvalidMetadataValue(#[from] InvalidMetadataValue),
}

#[cfg(test)]
mod tests {
    use super::*;

    mod from_header {
        use super::*;

        #[test]
        fn it_decodes_from_well_formed_header() {
            let credentials = BasicAuthCredentials::from_header("Basic YWRtaW46c2VjcmV0").unwrap();
            assert_eq!(credentials.user_name, "admin");
            assert_eq!(credentials.password.reveal(), b"secret");
        }

        #[test]
        fn it_rejects_header_without_basic_scheme() {
            let err = BasicAuthCredentials::from_header(" YWRtaW46c2VjcmV0").unwrap_err();
            if let BasicAuthError::InvalidScheme(s) = err {
                assert_eq!(s, "");
            } else {
                panic!("Unexpected error: {:?}", err);
            };
            let err = BasicAuthCredentials::from_header("Cookie YWRtaW46c2VjcmV0").unwrap_err();
            if let BasicAuthError::InvalidScheme(s) = err {
                assert_eq!(s, "Cookie");
            } else {
                panic!("Unexpected error: {:?}", err);
            };
        }
    }

    mod validate {
        use super::*;
        use crate::authentication::salted_password::create_salted_hashed_password;

        #[test]
        fn it_validates_for_matching_credentials() {
            let hashed = create_salted_hashed_password(b"secret").unwrap();
            let credentials = BasicAuthCredentials::new("admin".to_string(), hashed.to_string().into());
            credentials.validate("admin", b"secret").unwrap();
        }

        #[test]
        fn it_rejects_for_mismatching_credentials() {
            // Incorrect username, matching passphrase
            let hashed = create_salted_hashed_password(b"password").unwrap();
            let credentials = BasicAuthCredentials::new("good".to_string(), hashed.to_string().into());
            let err = credentials.validate("evil", b"password").unwrap_err();
            assert!(matches!(err, BasicAuthError::InvalidUsernameOrPassphrase));

            // Matching username, incorrect passphrase
            let hashed = create_salted_hashed_password(b"good").unwrap();
            let credentials = BasicAuthCredentials::new("user".to_string(), hashed.to_string().into());
            let err = credentials.validate("user", b"evil").unwrap_err();
            assert!(matches!(err, BasicAuthError::InvalidUsernameOrPassphrase));

            // Incorrect username, incorrect passphrase
            let hashed = create_salted_hashed_password(b"good").unwrap();
            let credentials = BasicAuthCredentials::new("good".to_string(), hashed.to_string().into());
            let err = credentials.validate("evil", b"evil").unwrap_err();
            assert!(matches!(err, BasicAuthError::InvalidUsernameOrPassphrase));
        }
    }

    mod generate_header {
        use super::*;

        #[test]
        fn it_generates_a_valid_header() {
            let header = BasicAuthCredentials::generate_header("admin", b"secret").unwrap();
            let cred = BasicAuthCredentials::from_header(header.to_str().unwrap()).unwrap();
            assert_eq!(cred.user_name, "admin");
            assert_eq!(cred.password.reveal(), &b"secret"[..]);
        }
    }
}
