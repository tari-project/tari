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

use std::{convert::TryFrom, fmt, fmt::Formatter, str::FromStr};

use serde::{Deserialize, Serialize};
use tari_comms::tor;

#[derive(Clone, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub enum TorControlAuthentication {
    None,
    Password(String),
    /// Cookie authentication. The contents of the cookie file encoded as hex
    Cookie(String),
}

impl From<TorControlAuthentication> for tor::Authentication {
    fn from(auth: TorControlAuthentication) -> Self {
        match auth {
            TorControlAuthentication::None => tor::Authentication::None,
            TorControlAuthentication::Password(passwd) => tor::Authentication::HashedPassword(passwd),
            TorControlAuthentication::Cookie(cookie) => tor::Authentication::Cookie(cookie),
        }
    }
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

impl TryFrom<String> for TorControlAuthentication {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.as_str().parse()
    }
}

impl FromStr for TorControlAuthentication {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (auth_type, maybe_value) = parse_key_value(s, '=');
        match auth_type.as_str() {
            "none" => Ok(TorControlAuthentication::None),
            "password" => {
                let password = maybe_value.ok_or_else(|| {
                    "Invalid format for 'password' tor authentication type. It should be in the format \
                     'password=xxxxxx'."
                        .to_string()
                })?;
                Ok(TorControlAuthentication::Password(password.to_string()))
            },
            "cookie" => {
                let password = maybe_value.ok_or_else(|| {
                    "Invalid format for 'cookie' tor authentication type. It should be in the format 'cookie=xxxxxx'."
                        .to_string()
                })?;
                Ok(TorControlAuthentication::Cookie(password.to_string()))
            },
            s => Err(format!("Invalid tor auth type '{}'", s)),
        }
    }
}

impl fmt::Debug for TorControlAuthentication {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        use TorControlAuthentication::*;
        match self {
            None => write!(f, "None"),
            Password(_) => write!(f, "Password(...)"),
            Cookie(_) => write!(f, "Cookie(...)"),
        }
    }
}
