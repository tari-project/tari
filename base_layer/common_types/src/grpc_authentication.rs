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

use std::{fmt, ops::Deref, slice};

use serde::{
    Deserialize,
    Deserializer,
    Serialize,
    de::{self, MapAccess, SeqAccess, Visitor},
};
use tari_utilities::SafePassword;

/// A single `{ username, password }` gRPC Basic authentication credential pair.
///
/// The password is stored in plaintext inside [`SafePassword`] (which zeroizes on drop and redacts `Debug` output),
/// matching the credential model used by `minotari_node` and `minotari_console_wallet`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrpcCredential {
    pub username: String,
    #[serde(
        serialize_with = "serialize_redacted_password",
        deserialize_with = "deserialize_safe_password"
    )]
    pub password: SafePassword,
}

impl GrpcCredential {
    pub fn new(username: impl Into<String>, password: SafePassword) -> Self {
        Self {
            username: username.into(),
            password,
        }
    }

    pub fn username_password(&self) -> (&str, &SafePassword) {
        (&self.username, &self.password)
    }
}

/// Server-side list of per-client gRPC Basic authentication credentials (`grpc_credentials`).
///
/// When empty, gRPC authentication is disabled. When one or more entries are present, the server accepts any matching
/// `{ username, password }` entry and attributes the request to the matched `username`.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(transparent)]
pub struct GrpcCredentials(Vec<GrpcCredential>);

impl GrpcCredentials {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn from_credentials(credentials: Vec<GrpcCredential>) -> Self {
        Self(credentials)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn as_slice(&self) -> &[GrpcCredential] {
        &self.0
    }

    pub fn iter(&self) -> slice::Iter<'_, GrpcCredential> {
        self.0.iter()
    }

    pub fn into_vec(self) -> Vec<GrpcCredential> {
        self.0
    }
}

impl Deref for GrpcCredentials {
    type Target = [GrpcCredential];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<[GrpcCredential]> for GrpcCredentials {
    fn as_ref(&self) -> &[GrpcCredential] {
        &self.0
    }
}

impl From<Vec<GrpcCredential>> for GrpcCredentials {
    fn from(credentials: Vec<GrpcCredential>) -> Self {
        Self(credentials)
    }
}

impl From<GrpcCredential> for GrpcCredentials {
    fn from(credential: GrpcCredential) -> Self {
        Self(vec![credential])
    }
}

impl From<GrpcAuthentication> for GrpcCredentials {
    fn from(auth: GrpcAuthentication) -> Self {
        match auth {
            GrpcAuthentication::None => Self::new(),
            GrpcAuthentication::Basic { username, password } => Self(vec![GrpcCredential { username, password }]),
        }
    }
}

impl From<&GrpcAuthentication> for GrpcCredentials {
    fn from(auth: &GrpcAuthentication) -> Self {
        match auth {
            GrpcAuthentication::None => Self::new(),
            GrpcAuthentication::Basic { username, password } => Self(vec![GrpcCredential {
                username: username.clone(),
                password: password.clone(),
            }]),
        }
    }
}

impl IntoIterator for GrpcCredentials {
    type IntoIter = std::vec::IntoIter<GrpcCredential>;
    type Item = GrpcCredential;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a GrpcCredentials {
    type IntoIter = slice::Iter<'a, GrpcCredential>;
    type Item = &'a GrpcCredential;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<'de> Deserialize<'de> for GrpcCredentials {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: Deserializer<'de> {
        struct GrpcCredentialsVisitor;

        impl<'de> Visitor<'de> for GrpcCredentialsVisitor {
            type Value = GrpcCredentials;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str(
                    "a list of `{ username, password }` credential tables, a single credential table, or an \
                     environment override string",
                )
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E>
            where E: de::Error {
                Ok(GrpcCredentials::new())
            }

            fn visit_none<E>(self) -> Result<Self::Value, E>
            where E: de::Error {
                Ok(GrpcCredentials::new())
            }

            fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where D: Deserializer<'de> {
                GrpcCredentials::deserialize(deserializer)
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where A: SeqAccess<'de> {
                let mut entries = seq.size_hint().map(Vec::with_capacity).unwrap_or_default();
                while let Some(entry) = seq.next_element::<GrpcCredential>()? {
                    entries.push(entry);
                }
                Ok(GrpcCredentials(entries))
            }

            fn visit_map<M>(self, map: M) -> Result<Self::Value, M::Error>
            where M: MapAccess<'de> {
                let single = GrpcCredential::deserialize(de::value::MapAccessDeserializer::new(map))?;
                Ok(GrpcCredentials(vec![single]))
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where E: de::Error {
                parse_credentials_str(v).map_err(E::custom)
            }

            fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
            where E: de::Error {
                self.visit_str(&v)
            }
        }

        deserializer.deserialize_any(GrpcCredentialsVisitor)
    }
}

fn parse_credentials_str(raw: &str) -> Result<GrpcCredentials, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") {
        return Ok(GrpcCredentials::new());
    }

    if trimmed.starts_with('[') {
        let list = serde_json::from_str::<Vec<GrpcCredential>>(trimmed)
            .map_err(|e| format!("Invalid JSON credential array in grpc_credentials: {e}"))?;
        return Ok(GrpcCredentials(list));
    }

    let single = serde_json::from_str::<GrpcCredential>(trimmed)
        .map_err(|e| format!("Invalid JSON credential object in grpc_credentials: {e}"))?;
    Ok(GrpcCredentials(vec![single]))
}

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

const REDACTED_PASSWORD_PLACEHOLDER: &str = "<redacted>";

fn serialize_redacted_password<S>(_password: &SafePassword, serializer: S) -> Result<S::Ok, S::Error>
where S: serde::Serializer {
    serializer.serialize_str(REDACTED_PASSWORD_PLACEHOLDER)
}

fn deserialize_safe_password<'de, D>(deserializer: D) -> Result<SafePassword, D::Error>
where D: serde::Deserializer<'de> {
    let password: String = Deserialize::deserialize(deserializer)?;
    if password == REDACTED_PASSWORD_PLACEHOLDER {
        return Err(serde::de::Error::custom(
            "password cannot be the '<redacted>' placeholder",
        ));
    }
    Ok(SafePassword::from(password))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializing_grpc_authentication_redacts_password_and_rejects_placeholder_round_trip() {
        let secret = "super_secret_grpc_password_123";
        let auth = GrpcAuthentication::Basic {
            username: "admin".to_string(),
            password: SafePassword::from(secret),
        };

        let json_out = serde_json::to_string(&auth).unwrap();
        assert!(!json_out.contains(secret));
        assert!(json_out.contains(REDACTED_PASSWORD_PLACEHOLDER));

        // Deserializing the redacted output back into GrpcAuthentication must fail rather than
        // silently authenticating against the literal "<redacted>" placeholder.
        assert!(serde_json::from_str::<GrpcAuthentication>(&json_out).is_err());

        let valid: GrpcAuthentication =
            serde_json::from_str(r#"{"username":"admin","password":"real_password"}"#).unwrap();
        let (user, pass) = valid.username_password().unwrap();
        assert_eq!(user, "admin");
        assert_eq!(pass.reveal(), b"real_password");
    }

    #[test]
    fn deserializes_grpc_credentials_from_seq_and_json_env_strings() {
        let json = r#"[{"username":"miner","password":"p1"},{"username":"monitor","password":"p2"}]"#;
        let creds: GrpcCredentials = serde_json::from_str(json).unwrap();
        let [c0, c1] = creds.as_slice() else {
            panic!("expected 2 credentials");
        };
        assert_eq!(c0.username, "miner");
        assert_eq!(c0.password.reveal(), b"p1");
        assert_eq!(c1.username, "monitor");
        assert_eq!(c1.password.reveal(), b"p2");

        let env_json_array =
            r#""[{\"username\":\"miner\",\"password\":\"p1\"},{\"username\":\"monitor\",\"password\":\"p2\"}]""#;
        let creds_from_env_str: GrpcCredentials = serde_json::from_str(env_json_array).unwrap();
        let [e0, e1] = creds_from_env_str.as_slice() else {
            panic!("expected 2 credentials");
        };
        assert_eq!(e0.username, "miner");
        assert_eq!(e0.password.reveal(), b"p1");
        assert_eq!(e1.username, "monitor");
        assert_eq!(e1.password.reveal(), b"p2");

        let env_json_single = r#""{\"username\":\"miner\",\"password\":\"p1\"}""#;
        let creds_single: GrpcCredentials = serde_json::from_str(env_json_single).unwrap();
        let [s0] = creds_single.as_slice() else {
            panic!("expected 1 credential");
        };
        assert_eq!(s0.username, "miner");
        assert_eq!(s0.password.reveal(), b"p1");

        let none_creds: GrpcCredentials = serde_json::from_str(r#""none""#).unwrap();
        assert!(none_creds.is_empty());
    }

    #[test]
    fn rejects_malformed_json_env_strings() {
        assert!(parse_credentials_str("not-json").is_err());
        assert!(parse_credentials_str(r#"[{"username": "miner"}]"#).is_err());
    }
}
