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

use crate::tor::client::{commands::TorCommand, error::TorClientError, response::ResponseLine};

/// The DEL_ONION command.
///
/// This instructs Tor to delete a hidden service.
pub struct DelOnion<'a> {
    service_id: &'a str,
}

impl<'a> DelOnion<'a> {
    pub fn new(service_id: &'a str) -> Self {
        Self { service_id }
    }
}

impl<'a> TorCommand for DelOnion<'a> {
    type Error = TorClientError;
    type Output = ();

    fn to_command_string(&self) -> Result<String, Self::Error> {
        Ok(format!("DEL_ONION {}", self.service_id))
    }

    fn parse_responses(&self, mut responses: Vec<ResponseLine<'_>>) -> Result<Self::Output, Self::Error> {
        let last_response = responses.pop().ok_or(TorClientError::UnexpectedEof)?;
        if let Some(err) = last_response.err() {
            return Err(TorClientError::TorCommandFailed(err.into_owned()));
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn to_command_string() {
        let command = DelOnion::new("some-random-key");
        assert_eq!(command.to_command_string().unwrap(), "DEL_ONION some-random-key");
    }
}
