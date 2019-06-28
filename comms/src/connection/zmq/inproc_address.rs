//  Copyright 2019 The Tari Project
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

use crate::connection::zmq::{ZmqEndpoint, ZmqError};
use rand::{distributions::Alphanumeric, EntropyRng, Rng};
use std::{fmt, iter, str::FromStr};

const DEFAULT_INPROC: &'static str = "inproc://default";

/// Represents a zMQ inproc address. More information [here](http://api.zeromq.org/2-1:zmq-inproc).
#[derive(Eq, PartialEq, Debug, Clone)]
pub struct InprocAddress(String);

impl InprocAddress {
    /// Generate a random InprocAddress.
    pub fn random() -> Self {
        let mut rng = EntropyRng::new();
        let rand_str: String = iter::repeat(()).map(|_| rng.sample(Alphanumeric)).take(8).collect();
        Self(format!("inproc://{}", rand_str))
    }

    pub fn is_default(&self) -> bool {
        self.0 == DEFAULT_INPROC
    }
}

impl Default for InprocAddress {
    fn default() -> Self {
        Self(DEFAULT_INPROC.to_owned())
    }
}

impl FromStr for InprocAddress {
    type Err = ZmqError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() > 9 && s.starts_with("inproc://") {
            Ok(InprocAddress(s.to_owned()))
        } else {
            Err(ZmqError::MalformedInprocAddress)
        }
    }
}

impl fmt::Display for InprocAddress {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl ZmqEndpoint for InprocAddress {
    fn to_zmq_endpoint(&self) -> String {
        self.0.to_string()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn from_str() {
        let addr = "inproc://扩".parse::<InprocAddress>().unwrap();
        assert_eq!("inproc://扩", addr.to_zmq_endpoint());

        let result = "inporc://abc".parse::<InprocAddress>();
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert_eq!(ZmqError::MalformedInprocAddress, err);

        let result = "inproc://".parse::<InprocAddress>();
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert_eq!(ZmqError::MalformedInprocAddress, err);
    }

    #[test]
    fn default() {
        let addr = InprocAddress::default();
        assert!(addr.is_default());
        let addr = InprocAddress::random();
        assert!(!addr.is_default());
    }
}
