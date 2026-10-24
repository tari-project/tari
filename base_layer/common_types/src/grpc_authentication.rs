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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GrpcAuthentication {
    #[default]
    None,
    Basic {
        username: String,
        #[serde(
            serialize_with = "serialize_redacted_password",
            deserialize_with = "deserialize_safe_password"
        )]
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
}

fn serialize_redacted_password<S>(_password: &SafePassword, serializer: S) -> Result<S::Ok, S::Error>
where S: serde::Serializer {
    serializer.serialize_str("<redacted>")
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
    fn serializing_grpc_authentication_redacts_password() {
        let secret = "super_secret_grpc_password_123";
        let auth = GrpcAuthentication::Basic {
            username: "admin".to_string(),
            password: SafePassword::from(secret),
        };

        let toml_out = toml::to_string(&auth).unwrap();
        assert!(!toml_out.contains(secret));
        assert!(toml_out.contains("<redacted>"));

        let json_out = serde_json::to_string(&auth).unwrap();
        assert!(!json_out.contains(secret));
        assert!(json_out.contains("<redacted>"));
    }
}
