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
    convert::TryFrom,
    fmt,
    fmt::Formatter,
    fs,
    io::{Error, ErrorKind},
    path::PathBuf,
    str::FromStr,
};

use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use tari_comms::tor;

const DEFAULT_TOR_COOKIE_PATH: &str = "/run/tor/control.authcookie";

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TorCookie {
    Hex(String),
    FilePath(String),
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(try_from = "String")]
pub enum TorControlAuthentication {
    Auto,
    None,
    Password(String),
    /// Cookie authentication using either hex-encoded cookie bytes or a cookie file path
    Cookie(TorCookie),
}

impl TorControlAuthentication {
    pub fn hex(data: String) -> Self {
        Self::Cookie(TorCookie::Hex(data))
    }

    pub fn make_tor_auth(self) -> Result<tor::Authentication, Error> {
        match self {
            TorControlAuthentication::Auto => Ok(tor::Authentication::Auto),
            TorControlAuthentication::None => Ok(tor::Authentication::None),
            TorControlAuthentication::Password(passwd) => Ok(tor::Authentication::HashedPassword(passwd)),
            TorControlAuthentication::Cookie(cookie) => match cookie {
                TorCookie::Hex(hex) => Ok(tor::Authentication::Cookie(hex)),
                TorCookie::FilePath(path) => {
                    let data = fs::read(&path)?;
                    if data.is_empty() {
                        return Err(Error::new(
                            ErrorKind::UnexpectedEof,
                            "tor control auth cookie file is empty",
                        ));
                    }
                    Ok(tor::Authentication::CookieFile(PathBuf::from(path)))
                },
            },
        }
    }
}

fn parse_key_value(s: &str, split_chr: char) -> (String, Option<&str>) {
    let mut parts = s.splitn(2, split_chr);
    (
        parts
            .next()
            .map(|s| s.trim())
            .expect("splitn always emits at least one part")
            .to_lowercase(),
        parts.next().map(|s| s.trim()),
    )
}

// Used by serde
impl TryFrom<String> for TorControlAuthentication {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.as_str().parse()
    }
}

impl FromStr for TorControlAuthentication {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (auth_type, maybe_value) = parse_key_value(s, '=');
        match auth_type.as_str() {
            "auto" => Ok(TorControlAuthentication::Auto),
            "none" => Ok(TorControlAuthentication::None),
            "password" => {
                let password = maybe_value.ok_or_else(|| {
                    anyhow!(
                        "Invalid format for 'password' tor authentication type. It should be in the format \
                         'password=xxxxxx'."
                    )
                })?;
                Ok(TorControlAuthentication::Password(password.to_string()))
            },
            "cookie" => {
                if let Some(value) = maybe_value {
                    if let Some(mut path) = value.strip_prefix('@') {
                        if path.is_empty() {
                            path = DEFAULT_TOR_COOKIE_PATH;
                        }
                        Ok(TorControlAuthentication::Cookie(TorCookie::FilePath(path.to_string())))
                    } else {
                        Ok(TorControlAuthentication::Cookie(TorCookie::Hex(value.to_string())))
                    }
                } else {
                    Err(anyhow!(
                        "Invalid format for 'cookie' tor authentication type. It should be in the format \
                         'cookie=xxxxxx'."
                    ))
                }
            },
            s => Err(anyhow!("Invalid tor auth type '{}'", s)),
        }
    }
}

impl fmt::Debug for TorControlAuthentication {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        #[allow(clippy::enum_glob_use)]
        use TorControlAuthentication::*;
        match self {
            Auto => write!(f, "Auto"),
            None => write!(f, "None"),
            Password(_) => write!(f, "Password(...)"),
            Cookie(_) => write!(f, "Cookie(...)"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tor_parser_valid_case() {
        let auth = TorControlAuthentication::from_str("auto").unwrap();
        assert_eq!(auth, TorControlAuthentication::Auto);

        let auth = TorControlAuthentication::from_str("none").unwrap();
        assert_eq!(auth, TorControlAuthentication::None);

        let auth = TorControlAuthentication::from_str("password=").unwrap();
        assert_eq!(auth, TorControlAuthentication::Password("".into()));

        let auth = TorControlAuthentication::from_str("password     =     123         ").unwrap();
        assert_eq!(auth, TorControlAuthentication::Password("123".into()));

        let auth = TorControlAuthentication::from_str("password=123").unwrap();
        assert_eq!(auth, TorControlAuthentication::Password("123".into()));

        let auth = TorControlAuthentication::from_str("cookie=").unwrap();
        assert_eq!(auth, TorControlAuthentication::hex("".into()));

        let auth = TorControlAuthentication::from_str("cookie=8b6f").unwrap();
        assert_eq!(auth, TorControlAuthentication::hex("8b6f".into()));

        let auth = TorControlAuthentication::from_str("cookie=@").unwrap();
        assert_eq!(
            auth,
            TorControlAuthentication::Cookie(TorCookie::FilePath(DEFAULT_TOR_COOKIE_PATH.into()))
        );

        let auth = TorControlAuthentication::from_str("cookie=@/path/to/file").unwrap();
        assert_eq!(
            auth,
            TorControlAuthentication::Cookie(TorCookie::FilePath("/path/to/file".into()))
        );
    }

    #[test]
    fn tor_parser_invalid_case() {
        TorControlAuthentication::from_str("").unwrap_err();
        TorControlAuthentication::from_str("not_valid").unwrap_err();
        TorControlAuthentication::from_str("cookie abcd").unwrap_err();
    }

    #[test]
    fn make_tor_auth_cookie_file_preserves_path_and_validates_readability() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cookie_path = temp_dir.path().join("control_auth_cookie");
        fs::write(&cookie_path, [0xaa; 32]).unwrap();

        let parsed = TorControlAuthentication::from_str(&format!("cookie=@{}", cookie_path.display())).unwrap();
        let tor_auth = parsed.make_tor_auth().unwrap();
        assert_eq!(tor_auth, tor::Authentication::CookieFile(cookie_path.clone()));

        let hex_auth = TorControlAuthentication::from_str("cookie=8b6f")
            .unwrap()
            .make_tor_auth()
            .unwrap();
        assert_eq!(hex_auth, tor::Authentication::Cookie("8b6f".to_string()));

        // Empty cookie file fails fast at initialization.
        fs::write(&cookie_path, []).unwrap();
        let empty_err = TorControlAuthentication::Cookie(TorCookie::FilePath(cookie_path.to_string_lossy().into()))
            .make_tor_auth()
            .unwrap_err();
        assert_eq!(empty_err.kind(), ErrorKind::UnexpectedEof);

        // Missing cookie file fails fast and maps to ExitCode::TorAuthUnreadableCookie via TorTransportConfig.
        let missing_path = temp_dir.path().join("missing_cookie");
        let cfg = crate::transport::TorTransportConfig {
            control_auth: TorControlAuthentication::Cookie(TorCookie::FilePath(missing_path.to_string_lossy().into())),
            ..Default::default()
        };
        let init_err = cfg.to_control_auth().unwrap_err();
        assert!(matches!(
            init_err.to_exit_error().exit_code,
            tari_common::exit_codes::ExitCode::TorAuthUnreadableCookie
        ));
    }
}
