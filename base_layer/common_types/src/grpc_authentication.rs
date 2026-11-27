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

use std::{collections::HashSet, fmt, marker::PhantomData};

use serde::{
    Deserialize,
    Deserializer,
    Serialize,
    de::{self, MapAccess, SeqAccess, Visitor},
};
use tari_utilities::SafePassword;

fn deserialize_safe_password<'de, D>(deserializer: D) -> Result<SafePassword, D::Error>
where D: Deserializer<'de> {
    let password = String::deserialize(deserializer)?;
    Ok(SafePassword::from(password))
}

fn serialize_safe_password<S>(password: &SafePassword, serializer: S) -> Result<S::Ok, S::Error>
where S: serde::Serializer {
    serializer.serialize_str(password.reveal())
}

/// A single gRPC basic authentication credential (`username` and `password`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrpcCredential {
    pub username: String,
    #[serde(deserialize_with = "deserialize_safe_password", serialize_with = "serialize_safe_password")]
    pub password: SafePassword,
}

impl GrpcCredential {
    pub fn new<U: Into<String>, P: Into<SafePassword>>(username: U, password: P) -> Self {
        Self {
            username: username.into(),
            password: password.into(),
        }
    }
}

/// Validates a list of `grpc_credentials` entries at configuration load time.
///
/// Rejects entries with an empty username, an empty password, a `:` in the username (which is invalid in HTTP Basic
/// authentication), or a duplicate username, naming the offending entry index in the error message.
pub fn validate_grpc_credentials(creds: &[GrpcCredential]) -> Result<(), String> {
    let mut seen_usernames = HashSet::with_capacity(creds.len());
    for (idx, cred) in creds.iter().enumerate() {
        if cred.username.is_empty() {
            return Err(format!("grpc_credentials[{idx}] has an empty username"));
        }
        if cred.password.reveal().is_empty() {
            return Err(format!(
                "grpc_credentials[{idx}] ('{}') has an empty password",
                cred.username
            ));
        }
        if cred.username.contains(':') {
            return Err(format!(
                "grpc_credentials[{idx}] ('{}') contains ':', which is not allowed in HTTP Basic authentication \
                 usernames",
                cred.username
            ));
        }
        if !seen_usernames.insert(cred.username.as_str()) {
            return Err(format!(
                "grpc_credentials[{idx}] has duplicate username '{}'",
                cred.username
            ));
        }
    }
    Ok(())
}

/// Deserializes `grpc_credentials` from a TOML sequence of `{ username, password }` tables, a single
/// `{ username, password }` table, `"none"` / `"[]"` (empty list), or a JSON array string from `-p` / environment
/// variable overrides.
pub fn deserialize_grpc_credentials<'de, D>(deserializer: D) -> Result<Vec<GrpcCredential>, D::Error>
where D: Deserializer<'de> {
    struct GrpcCredentialsVisitor;

    impl<'de> Visitor<'de> for GrpcCredentialsVisitor {
        type Value = Vec<GrpcCredential>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a list of { username, password } tables, a JSON array string, or 'none'")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where E: de::Error {
            Ok(Vec::new())
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where E: de::Error {
            Ok(Vec::new())
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where A: SeqAccess<'de> {
            let mut creds = Vec::new();
            while let Some(cred) = seq.next_element::<GrpcCredential>()? {
                creds.push(cred);
            }
            Ok(creds)
        }

        fn visit_map<M>(self, map: M) -> Result<Self::Value, M::Error>
        where M: MapAccess<'de> {
            let cred = GrpcCredential::deserialize(de::value::MapAccessDeserializer::new(map))?;
            Ok(vec![cred])
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where E: de::Error {
            let trimmed = v.trim();
            if trimmed.is_empty() {
                return Err(de::Error::custom(
                    "grpc_credentials must not be an empty string; use 'none' or '[]' for no authentication",
                ));
            }
            if trimmed.eq_ignore_ascii_case("none") {
                return Ok(Vec::new());
            }
            serde_json::from_str::<Vec<GrpcCredential>>(trimmed).map_err(|e| {
                de::Error::custom(format!(
                    "invalid grpc_credentials JSON array ({e}); expected [{{\"username\":\"...\",\"password\":\"...\"}}]",
                ))
            })
        }
    }

    let creds = deserializer.deserialize_any(GrpcCredentialsVisitor)?;
    validate_grpc_credentials(&creds).map_err(de::Error::custom)?;
    Ok(creds)
}

/// Resolves the effective gRPC credentials from the primary `grpc_credentials` list and the deprecated
/// `grpc_authentication` setting.
///
/// Returns the resolved credentials along with an optional deprecation warning message that callers should log at
/// `WARN` level.
pub fn resolve_grpc_credentials(
    grpc_credentials: &[GrpcCredential],
    legacy_auth: &GrpcAuthentication,
) -> (Vec<GrpcCredential>, Option<&'static str>) {
    if !grpc_credentials.is_empty() {
        let warning = match legacy_auth {
            GrpcAuthentication::None => None,
            GrpcAuthentication::Basic { .. } => Some(
                "Both 'grpc_credentials' and deprecated 'grpc_authentication' are configured; using \
                 'grpc_credentials' and ignoring 'grpc_authentication'",
            ),
        };
        return (grpc_credentials.to_vec(), warning);
    }

    match legacy_auth {
        GrpcAuthentication::None => (Vec::new(), None),
        GrpcAuthentication::Basic { username, password } => {
            let warning = if username.is_empty() || password.reveal().is_empty() {
                "Config key 'grpc_authentication' is deprecated and has an empty username or password; please migrate \
                 to 'grpc_credentials = [{ username = "...", password = "..." }]'"
            } else {
                "Config key 'grpc_authentication' is deprecated and will be removed in a future release; please \
                 migrate to 'grpc_credentials = [{ username = "...", password = "..." }]'"
            };
            (
                vec![GrpcCredential {
                    username: username.clone(),
                    password: password.clone(),
                }],
                Some(warning),
            )
        },
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GrpcAuthentication {
    #[default]
    None,
    Basic {
        username: String,
        #[serde(deserialize_with = "deserialize_safe_password", serialize_with = "serialize_safe_password")]
        password: SafePassword,
    },
}

pub fn deserialize_grpc_authentication<'de, D>(deserializer: D) -> Result<GrpcAuthentication, D::Error>
where D: Deserializer<'de> {
    struct GrpcAuthVisitor<T> {
        phantom: PhantomData<T>,
    }

    impl<'de> Visitor<'de> for GrpcAuthVisitor<GrpcAuthentication> {
        type Value = GrpcAuthentication;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("'none' or a map with 'username' and 'password' fields")
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where E: de::Error {
            match v.trim().to_lowercase().as_str() {
                "none" => Ok(GrpcAuthentication::None),
                "" => Err(de::Error::custom("Invalid choice for `grpc_authentication`: ``")),
                _ => Err(de::Error::custom(format!(
                    "Invalid choice for `grpc_authentication`: `{v}`"
                ))),
            }
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where E: de::Error {
            Ok(GrpcAuthentication::None)
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where D: Deserializer<'de> {
            deserializer.deserialize_any(GrpcAuthVisitor { phantom: PhantomData })
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where E: de::Error {
            Ok(GrpcAuthentication::None)
        }

        fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
        where M: MapAccess<'de> {
            let mut username = None;
            let mut password = None;
            while let Some((key, value)) = access.next_entry::<String, String>()? {
                match key.as_str() {
                    "username" => username = Some(value),
                    "password" => password = Some(value.into()),
                    _ => {},
                }
            }
            let username = username.ok_or_else(|| de::Error::missing_field("username"))?;
            let password = password.ok_or_else(|| de::Error::missing_field("password"))?;

            Ok(GrpcAuthentication::Basic { username, password })
        }
    }

    deserializer.deserialize_any(GrpcAuthVisitor { phantom: PhantomData })
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;

    #[derive(Debug, Deserialize)]
    struct Wrapper {
        #[serde(default, deserialize_with = "deserialize_grpc_credentials")]
        grpc_credentials: Vec<GrpcCredential>,
        #[serde(default, deserialize_with = "deserialize_grpc_authentication")]
        grpc_authentication: GrpcAuthentication,
    }

    #[test]
    fn deserializes_multiple_credentials_from_toml_array() {
        let toml_str = r#"
            grpc_credentials = [
                { username = "alice", password = "pw1" },
                { username = "bob", password = "pw2" },
            ]
        "#;
        let w: Wrapper = toml::from_str(toml_str).unwrap();
        assert_eq!(w.grpc_credentials.len(), 2);
        assert_eq!(w.grpc_credentials.first().unwrap().username, "alice");
        assert_eq!(w.grpc_credentials.first().unwrap().password.reveal(), "pw1");
        assert_eq!(w.grpc_credentials.get(1).unwrap().username, "bob");
        assert_eq!(w.grpc_credentials.get(1).unwrap().password.reveal(), "pw2");
    }

    #[test]
    fn deserializes_credentials_from_json_string_override_and_rejects_blank() {
        let toml_str = r#"
            grpc_credentials = '[{"username":"cli","password":"secret"}]'
        "#;
        let w: Wrapper = toml::from_str(toml_str).unwrap();
        assert_eq!(w.grpc_credentials.len(), 1);
        assert_eq!(w.grpc_credentials.first().unwrap().username, "cli");
        assert_eq!(w.grpc_credentials.first().unwrap().password.reveal(), "secret");

        let none_str = r#"grpc_credentials = "none""#;
        let w_none: Wrapper = toml::from_str(none_str).unwrap();
        assert!(w_none.grpc_credentials.is_empty());

        let blank_str = r#"grpc_credentials = "   ""#;
        assert!(toml::from_str::<Wrapper>(blank_str).is_err());
    }

    #[test]
    fn rejects_invalid_and_duplicate_entries_in_grpc_credentials() {
        let empty_user = r#"
            grpc_credentials = [{ username = "", password = "pw1" }]
        "#;
        let err = toml::from_str::<Wrapper>(empty_user).unwrap_err().to_string();
        assert!(err.contains("grpc_credentials[0] has an empty username"), "{err}");

        let empty_pass = r#"
            grpc_credentials = [{ username = "alice", password = "" }]
        "#;
        let err = toml::from_str::<Wrapper>(empty_pass).unwrap_err().to_string();
        assert!(
            err.contains("grpc_credentials[0] ('alice') has an empty password"),
            "{err}"
        );

        let colon_user = r#"
            grpc_credentials = [{ username = "ali:ce", password = "pw1" }]
        "#;
        let err = toml::from_str::<Wrapper>(colon_user).unwrap_err().to_string();
        assert!(err.contains("grpc_credentials[0] ('ali:ce') contains ':'"), "{err}");

        let duplicate_user = r#"
            grpc_credentials = [
                { username = "admin", password = "pw1" },
                { username = "admin", password = "pw2" },
            ]
        "#;
        let err = toml::from_str::<Wrapper>(duplicate_user).unwrap_err().to_string();
        assert!(
            err.contains("grpc_credentials[1] has duplicate username 'admin'"),
            "{err}"
        );

        let unknown_field = r#"
            grpc_credentials = [{ username = "admin", passwrod = "pw1" }]
        "#;
        let err = toml::from_str::<Wrapper>(unknown_field).unwrap_err().to_string();
        assert!(err.contains("unknown field"), "{err}");
    }

    #[test]
    fn legacy_grpc_authentication_falls_back_and_new_credentials_take_precedence() {
        let legacy_only: Wrapper = toml::from_str(
            r#"
            grpc_authentication = { username = "legacy", password = "old" }
        "#,
        )
        .unwrap();
        let (resolved, warning) =
            resolve_grpc_credentials(&legacy_only.grpc_credentials, &legacy_only.grpc_authentication);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved.first().unwrap().username, "legacy");
        assert!(warning.is_some());

        let legacy_empty_pass: Wrapper = toml::from_str(
            r#"
            grpc_authentication = { username = "legacy", password = "" }
        "#,
        )
        .unwrap();
        let (resolved_empty, warning_empty) = resolve_grpc_credentials(
            &legacy_empty_pass.grpc_credentials,
            &legacy_empty_pass.grpc_authentication,
        );
        assert_eq!(resolved_empty.len(), 1);
        assert!(warning_empty.unwrap().contains("empty username or password"));

        let both: Wrapper = toml::from_str(
            r#"
            grpc_credentials = [{ username = "modern", password = "new" }]
            grpc_authentication = { username = "legacy", password = "old" }
        "#,
        )
        .unwrap();
        let (resolved_both, warning_both) = resolve_grpc_credentials(&both.grpc_credentials, &both.grpc_authentication);
        assert_eq!(resolved_both.len(), 1);
        assert_eq!(resolved_both.first().unwrap().username, "modern");
        assert!(warning_both.is_some());
    }
}
