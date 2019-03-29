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
use rmp_serde;
use serde::{Deserialize, Serialize};
use serde_json;

#[derive(Debug, Error)]
pub enum MessageError {
    // An error occurred serialising an object into binary
    BinarySerializeError(rmp_serde::encode::Error),
    // An error occurred deserialising binary data into an object
    BinaryDeserializeError(rmp_serde::decode::Error),
    // An error occurred de-/serialising an object from/into JSON
    JSONError(serde_json::error::Error),
    // An error occurred deserialising an object from Base64
    Base64DeserializeError(base64::DecodeError),
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
        let mut buf = Vec::new();
        self.serialize(&mut rmp_serde::Serializer::new(&mut buf))
            .map_err(|e| MessageError::BinarySerializeError(e))?;
        Ok(buf.to_vec())
    }

    fn to_json(&self) -> Result<String, MessageError> {
        serde_json::to_string(self).map_err(|e| MessageError::JSONError(e))
    }

    fn to_base64(&self) -> Result<String, MessageError> {
        let val = self.to_binary()?;
        Ok(base64::encode(&val))
    }

    fn from_binary(msg: &[u8]) -> Result<Self, MessageError> {
        let mut de = rmp_serde::Deserializer::new(msg);
        Deserialize::deserialize(&mut de).map_err(|e| MessageError::BinaryDeserializeError(e))
    }

    fn from_json(msg: &str) -> Result<Self, MessageError> {
        let mut de = serde_json::Deserializer::from_reader(msg.as_bytes());
        Deserialize::deserialize(&mut de).map_err(|e| MessageError::JSONError(e))
    }

    fn from_base64(msg: &str) -> Result<Self, MessageError> {
        let buf = base64::decode(msg)?;
        Self::from_binary(&buf)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use base64::DecodeError as Base64Error;
    use rmp_serde::decode::Error as RMPError;
    use serde_derive::{Deserialize, Serialize};
    use serde_json::error::{Category, ErrorCode};
    use std::{error::Error, io::ErrorKind};

    #[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
    struct TestMessage {
        key: String,
        value: u64,
        sub_message: Option<Box<TestMessage>>,
    }

    impl TestMessage {
        pub fn new(key: &str, value: u64) -> TestMessage {
            TestMessage {
                key: key.to_string(),
                value,
                sub_message: None,
            }
        }

        pub fn set_sub_message(&mut self, msg: TestMessage) {
            self.sub_message = Some(Box::new(msg));
        }
    }

    #[test]
    fn binary_simple() {
        let val = TestMessage::new("twenty", 20);
        let msg = val.to_binary().unwrap();
        assert_eq!(msg, b"\x93\xA6\x74\x77\x65\x6E\x74\x79\x14\xC0");
        let val2 = TestMessage::from_binary(&msg).unwrap();
        assert_eq!(val, val2);
    }

    #[test]
    fn base64_simple() {
        let val = TestMessage::new("twenty", 20);
        let msg = val.to_base64().unwrap();
        assert_eq!(msg, "k6Z0d2VudHkUwA==");
        let val2 = TestMessage::from_base64(&msg).unwrap();
        assert_eq!(val, val2);
    }

    #[test]
    fn json_simple() {
        let val = TestMessage::new("twenty", 20);
        let msg = val.to_json().unwrap();
        assert_eq!(msg, "{\"key\":\"twenty\",\"value\":20,\"sub_message\":null}");
        let val2 = TestMessage::from_json(&msg).unwrap();
        assert_eq!(val, val2);
    }

    #[test]
    fn nested_message() {
        let inner = TestMessage::new("today", 100);
        let mut val = TestMessage::new("tomorrow", 50);
        val.set_sub_message(inner);

        let msg_json = val.to_json().unwrap();
        assert_eq!(
            msg_json,
            "{\"key\":\"tomorrow\",\"value\":50,\"sub_message\":{\"key\":\"today\",\"value\":100,\"sub_message\":\
             null}}"
        );

        let msg_base64 = val.to_base64().unwrap();
        assert_eq!(msg_base64, "k6h0b21vcnJvdzKTpXRvZGF5ZMA=");

        let msg_bin = val.to_binary().unwrap();
        assert_eq!(
            msg_bin,
            b"\x93\xA8\x74\x6F\x6D\x6F\x72\x72\x6F\x77\x32\x93\xA5\x74\x6F\x64\x61\x79\x64\xC0"
        );

        let val2 = TestMessage::from_json(&msg_json).unwrap();
        assert_eq!(val, val2);

        let val2 = TestMessage::from_base64(&msg_base64).unwrap();
        assert_eq!(val, val2);

        let val2 = TestMessage::from_binary(&msg_bin).unwrap();
        assert_eq!(val, val2);
    }

    #[test]
    fn fail_json() {
        let err = TestMessage::from_json("{\"key\":5}").err().unwrap();
        match err {
            MessageError::JSONError(e) => {
                assert_eq!(e.line(), 1);
                assert_eq!(e.column(), 9);
                assert!(e.is_data());
            },
            _ => panic!("JSON conversion should fail"),
        };
    }

    #[test]
    fn fail_base64() {
        let err = TestMessage::from_base64("aaaaa$aaaaa").err().unwrap();
        match err {
            MessageError::Base64DeserializeError(Base64Error::InvalidByte(offset, val)) => {
                assert_eq!(offset, 5);
                assert_eq!(val, '$' as u8);
            },
            _ => panic!("Base64 conversion should fail"),
        };

        let err = TestMessage::from_base64("j6h0b21vcnJvdzKTpXRvZGF5ZMA=").err().unwrap();
        match err {
            MessageError::BinaryDeserializeError(RMPError::Syntax(s)) => {
                assert_eq!(s, "invalid type: sequence, expected field identifier");
            },
            _ => panic!("Base64 conversion should fail"),
        };
    }

    #[test]
    fn fail_binary() {
        let err = TestMessage::from_binary(b"").err().unwrap();
        match err {
            MessageError::BinaryDeserializeError(RMPError::InvalidMarkerRead(e)) => {
                assert_eq!(e.kind(), ErrorKind::UnexpectedEof, "Unexpected error type: {:?}", e);
            },
            _ => {
                panic!("Base64 conversion should fail");
            },
        }
    }
}
