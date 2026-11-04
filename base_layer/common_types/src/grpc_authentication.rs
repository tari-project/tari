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

use serde::{Deserialize, Serialize};
use tari_utilities::SafePassword;

/// Represents a single username/password credential for gRPC authentication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrpcCredential {
    pub username: String,
    #[serde(deserialize_with = "deserialize_safe_password")]
    pub password: SafePassword,
}

impl GrpcCredential {
    pub fn new(username: impl Into<String>, password: SafePassword) -> Self {
        Self {
            username: username.into(),
            password,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GrpcAuthentication {
    #[default]
    None,
    Basic {
        username: String,
        #[serde(deserialize_with = "deserialize_safe_password")]
        password: SafePassword,
    },
}

impl GrpcAuthentication {
    pub fn username_password(&self) -> Option<(&str, &SafePassword)> {
        match self {
            GrpcAuthentication::Basic { username, password } => Some((username, password)),
            _ => None,
        }
    }

    pub fn to_credentials(&self) -> Vec<GrpcCredential> {
        match self {
            GrpcAuthentication::None => Vec::new(),
            GrpcAuthentication::Basic { username, password } => {
                vec![GrpcCredential::new(username.clone(), password.clone())]
            },
        }
    }
}

fn deserialize_safe_password<'de, D>(deserializer: D) -> Result<SafePassword, D::Error>
where D: serde::Deserializer<'de> {
    let password: String = Deserialize::deserialize(deserializer)?;
    Ok(SafePassword::from(password))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grpc_credential_serialization_and_to_credentials() {
        let cred = GrpcCredential::new("user1", "pass1".to_string().into());
        assert_eq!(cred.username, "user1");
        assert_eq!(cred.password.reveal(), b"pass1");

        let auth_basic = GrpcAuthentication::Basic {
            username: "user2".to_string(),
            password: "pass2".to_string().into(),
        };
        let creds = auth_basic.to_credentials();
        assert_eq!(creds.len(), 1);
        assert_eq!(creds[0].username, "user2");
        assert_eq!(creds[0].password.reveal(), b"pass2");

        let auth_none = GrpcAuthentication::None;
        assert!(auth_none.to_credentials().is_empty());
    }
}
