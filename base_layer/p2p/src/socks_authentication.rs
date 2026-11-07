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

use std::{
    fmt::{Debug, Formatter},
    str::FromStr,
};

use serde::{Deserialize, Serialize};
use tari_comms::socks;

#[derive(Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SocksAuthentication {
    #[default]
    None,
    UsernamePassword {
        username: String,
        password: String,
    },
}

fn parse_key_value(s: &str, split_chr: char) -> (String, Option<&str>) {
    let mut parts = s.splitn(2, split_chr);
    (
        parts
            .next()
            .expect("splitn always emits at least one part")
            .trim()
            .to_lowercase(),
        parts.next(),
    )
}

impl FromStr for SocksAuthentication {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (auth_type, maybe_value) = parse_key_value(s, '=');
        match auth_type.as_str() {
            "none" => {
                if maybe_value.is_some() {
                    return Err("invalid format for 'none' socks authentication type. It should be 'none'.".to_string());
                }
                Ok(SocksAuthentication::None)
            },
            "username_password" => {
                let (username, password) = maybe_value
                    .and_then(|value| value.split_once(':'))
                    .filter(|(username, _)| !username.is_empty())
                    .ok_or_else(|| {
                        "invalid format for 'username-password' socks authentication type. It should be in the format \
                         'username_password=my_username:xxxxxx'."
                            .to_string()
                    })?;
                Ok(SocksAuthentication::UsernamePassword {
                    username: username.to_string(),
                    password: password.to_string(),
                })
            },
            s => Err(format!("invalid SOCKS auth type: {s}")),
        }
    }
}

impl Debug for SocksAuthentication {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            SocksAuthentication::None => write!(f, "SocksAuthentication::None"),
            SocksAuthentication::UsernamePassword { username, .. } => write!(
                f,
                r#"SocksAuthentication::UsernamePassword {{ username: {username}, password: "..."}}"#
            ),
        }
    }
}

impl From<SocksAuthentication> for socks::Authentication {
    fn from(config: SocksAuthentication) -> Self {
        match config {
            SocksAuthentication::None => socks::Authentication::None,
            SocksAuthentication::UsernamePassword { username, password } => {
                socks::Authentication::Password { username, password }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socks_parser_valid_cases() {
        assert_eq!(SocksAuthentication::from_str("none").unwrap(), SocksAuthentication::None);
        assert_eq!(SocksAuthentication::from_str("NONE").unwrap(), SocksAuthentication::None);

        let auth = SocksAuthentication::from_str("username_password=Alice_User:Secret:Pass123").unwrap();
        assert_eq!(
            auth,
            SocksAuthentication::UsernamePassword {
                username: "Alice_User".to_string(),
                password: "Secret:Pass123".to_string(),
            }
        );
    }

    #[test]
    fn socks_parser_invalid_cases() {
        SocksAuthentication::from_str("").unwrap_err();
        SocksAuthentication::from_str("none=extra").unwrap_err();
        SocksAuthentication::from_str("username_password").unwrap_err();
        SocksAuthentication::from_str("username_password=alice").unwrap_err();
        SocksAuthentication::from_str("username_password=:secret").unwrap_err();
    }
}
