// Copyright 2020, The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{fmt, marker::PhantomData};

use crate::tor::control_client::{
    commands::TorCommand,
    error::TorClientError,
    parsers,
    parsers::ParseError,
    response::ResponseLine,
};

/// The PROTOCOLINFO command.
///
/// This command is used to inspect control port auth configuration.
pub struct ProtocolInfo<'a>(PhantomData<&'a ()>);

impl ProtocolInfo<'_> {
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

impl TorCommand for ProtocolInfo<'_> {
    type Error = TorClientError;
    type Output = ProtocolInfoResponse;

    fn to_command_string(&self) -> Result<String, Self::Error> {
        Ok("PROTOCOLINFO".to_string())
    }

    fn parse_responses(&self, responses: Vec<ResponseLine>) -> Result<Self::Output, Self::Error> {
        let mut resp = ProtocolInfoResponse::default();
        for response in responses {
            if response.is_err() {
                return Err(TorClientError::TorCommandFailed(response.value));
            }

            if !response.has_more {
                continue;
            }
            let mut kv = response.value.splitn(2, ' ');
            let key = match kv.next() {
                Some(k) => k,
                None => continue,
            };
            match key {
                "PROTOCOLINFO" => {
                    let value = kv.next().ok_or(TorClientError::KeyValueNoValue)?;
                    resp.protocol_info_version = value.parse().map_err(ParseError::from)?;
                },
                "AUTH" => {
                    let value = kv.next().ok_or(TorClientError::KeyValueNoValue)?;
                    let kv = parsers::multi_key_value(value)?;
                    resp.auth_methods = ProtocolAuthMethods {
                        methods: kv
                            .get("METHODS")
                            .and_then(|m| m.first())
                            .map(|s| s.split(',').map(ToString::to_string).collect())
                            .unwrap_or_else(|| vec!["NULL".to_string()]),
                        cookie_file: kv.get("COOKIEFILE").and_then(|m| m.first()).map(|v| v.to_string()),
                    };
                },
                "VERSION" => {
                    let value = kv.next().ok_or(TorClientError::KeyValueNoValue)?;
                    resp.tor_version = value.to_string();
                },
                _ => continue,
            }
        }

        Ok(resp)
    }
}

impl fmt::Display for ProtocolInfo<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PROTOCOLINFO")
    }
}

#[derive(Debug, Clone, Default)]
pub struct ProtocolInfoResponse {
    pub protocol_info_version: u16,
    pub auth_methods: ProtocolAuthMethods,
    pub tor_version: String,
}

#[derive(Debug, Clone, Default)]
pub struct ProtocolAuthMethods {
    pub methods: Vec<String>,
    pub cookie_file: Option<String>,
}
