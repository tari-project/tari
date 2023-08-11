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

#[derive(Clone, Serialize, Deserialize, Default)]
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
            .to_lowercase(),
        parts.next(),
    )
}

impl FromStr for SocksAuthentication {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (auth_type, maybe_value) = parse_key_value(s, '=');
        match auth_type.as_str() {
            "none" => Ok(SocksAuthentication::None),
            "username_password" => {
                let (username, password) = maybe_value
                    .and_then(|value| {
                        let (un, pwd) = parse_key_value(value, ':');
                        // If pwd is None, return None
                        pwd.map(|p| (un, p))
                    })
                    .ok_or_else(|| {
                        "invalid format for 'username-password' socks authentication type. It should be in the format \
                         'username_password=my_username:xxxxxx'."
                            .to_string()
                    })?;
                Ok(SocksAuthentication::UsernamePassword {
                    username,
                    password: password.to_string(),
                })
            },
            s => Err(format!("invalid SOCKS auth type: {}", s)),
        }
    }
}

impl Debug for SocksAuthentication {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            SocksAuthentication::None => write!(f, "SocksAuthentication::None"),
            SocksAuthentication::UsernamePassword { username, .. } => write!(
                f,
                r#"SocksAuthentication::UsernamePassword {{ username: {}, password: "..."}}"#,
                username
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
