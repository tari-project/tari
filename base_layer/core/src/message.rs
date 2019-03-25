// Copyright 2018 The Tari Project
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

use base64;
use derive_error::Error;
use rmp_serde::{Deserializer, Serializer};
use serde::{Deserialize, Serialize};
use serde_json;
use std::fmt::Write;

#[derive(Debug, Error)]
pub enum MessageError {
    // An error occurred serialising an object into binary
    BinarySerializeError,
}

pub trait MessageFormat: Sized {
    fn to_binary(&self) -> Result<Vec<u8>, MessageError>;
    fn to_json(&self) -> Result<String, MessageError>;
    fn to_base64(&self) -> Result<String, MessageError>;

    fn from_binary(msg: &[u8]) -> Result<Self, MessageError>;
    fn from_json(msg: &str) -> Result<Self, MessageError>;
    fn from_base64(msg: &str) -> Result<Self, MessageError>;
}

impl<'a, T> MessageFormat for T
where T: Deserialize<'a> + Serialize
{
    fn to_binary(&self) -> Result<Vec<u8>, MessageError> {
        unimplemented!()
    }

    fn to_json(&self) -> Result<String, MessageError> {
        unimplemented!()
    }

    fn to_base64(&self) -> Result<String, MessageError> {
        unimplemented!()
    }

    fn from_binary(msg: &[u8]) -> Result<Self, MessageError> {
        unimplemented!()
    }

    fn from_json(msg: &str) -> Result<Self, MessageError> {
        unimplemented!()
    }

    fn from_base64(msg: &str) -> Result<Self, MessageError> {
        unimplemented!()
    }
}
