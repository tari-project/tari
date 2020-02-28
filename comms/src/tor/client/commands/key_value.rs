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

use crate::tor::client::{commands::TorCommand, error::TorClientError, parsers, response::ResponseLine};
use std::{borrow::Cow, marker::PhantomData};

/// The GETCONF command.
///
/// This command is used to query the Tor proxy configuration file.
pub fn get_conf(query: &str) -> KeyValueCommand<'_, '_> {
    KeyValueCommand::new("GETCONF", &[query])
}

/// The GETINFO command.
///
/// This command is used to retrieve Tor proxy configuration keys.
pub fn get_info(key_name: &str) -> KeyValueCommand<'_, '_> {
    KeyValueCommand::new("GETINFO", &[key_name])
}

pub struct KeyValueCommand<'a, 'b> {
    command: &'a str,
    args: Vec<&'b str>,
    _lifetime: PhantomData<&'b ()>,
}

impl<'a, 'b> KeyValueCommand<'a, 'b> {
    pub fn new(command: &'a str, args: &[&'b str]) -> Self {
        Self {
            command,
            args: args.to_vec(),
            _lifetime: PhantomData,
        }
    }
}

impl<'a, 'b> TorCommand for KeyValueCommand<'a, 'b> {
    type Error = TorClientError;
    type Output = Vec<Cow<'b, str>>;

    fn to_command_string(&self) -> Result<String, Self::Error> {
        Ok(format!("{} {}", self.command, self.args.join(" ")))
    }

    fn parse_responses(&self, mut responses: Vec<ResponseLine<'_>>) -> Result<Self::Output, Self::Error> {
        if let Some(resp) = responses.iter().find(|v| v.is_err()) {
            return Err(TorClientError::TorCommandFailed(resp.value.to_string()));
        }

        if let Some(last_line) = responses.last() {
            // Drop the last line if it's '250 OK' - some commands return it (GETINFO), some don't (GETCONF)
            if last_line.value == "OK" {
                let _ = responses.pop();
            }
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
        let command = KeyValueCommand::new("GETCONF", &["HiddenServicePort"]);
        assert_eq!(command.to_command_string().unwrap(), "GETCONF HiddenServicePort");

        let command = KeyValueCommand::new("GETINFO", &["net/listeners/socks"]);
        assert_eq!(command.to_command_string().unwrap(), "GETINFO net/listeners/socks");
    }

    // #[test]
    // fn parse_responses() {
    //     let command = KeyValueCommand::new("", &[]);
    //     command.parse_responses(vec!)
    // }
}
