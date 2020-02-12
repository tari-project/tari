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

use crate::tor::{commands::TorCommand, error::TorClientError, parsers, response::ResponseLine};
use std::{borrow::Cow, marker::PhantomData};

/// The GET_CONF command.
///
/// This command is used to query the Tor proxy configuration.
pub struct GetConf<'a, 'b> {
    query_key: &'a str,
    _lifetime: PhantomData<&'b ()>,
}

impl<'a> GetConf<'a, '_> {
    pub fn new(query_key: &'a str) -> Self {
        Self {
            query_key,
            _lifetime: PhantomData,
        }
    }
}

impl<'a, 'b> TorCommand for GetConf<'a, 'b> {
    type Error = TorClientError;
    type Output = Vec<Cow<'b, str>>;

    fn to_command_string(&self) -> Result<String, Self::Error> {
        Ok(format!("GET_CONF {}", self.query_key))
    }

    fn parse_responses(&self, responses: Vec<ResponseLine<'_>>) -> Result<Self::Output, Self::Error> {
        if let Some(resp) = responses.iter().find(|v| v.is_err()) {
            return Err(TorClientError::TorCommandFailed(resp.value.to_string()));
        }

        let responses = responses
            .iter()
            .map(|resp| parsers::key_value(&resp.value))
            .collect::<Vec<_>>();

        // Return the first parse error if any
        if let Some(Err(err)) = responses.iter().find(|v| v.is_err()) {
            return Err(err.clone().into());
        }

        Ok(responses
            .iter()
            .filter_map(|r| r.as_ref().ok())
            .map(|(_, value)| Cow::from(value.clone().into_owned()))
            .collect())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn to_command_string() {
        let command = GetConf::new("HiddenServicePort");
        assert_eq!(command.to_command_string().unwrap(), "GET_CONF HiddenServicePort");
    }
}
