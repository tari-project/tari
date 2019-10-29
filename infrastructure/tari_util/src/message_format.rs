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
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json;

#[derive(Debug, Error)]
pub enum MessageFormatError {
    // An error occurred serialising an object into binary
    #[error(no_from, no_std)]
    BinarySerializeError,
    // An error occurred deserialising binary data into an object
    #[error(no_from, no_std)]
    BinaryDeserializeError,
    // An error occurred de-/serialising an object from/into JSON
    JSONError(serde_json::error::Error),
    // An error occurred deserialising an object from Base64
    Base64DeserializeError(base64::DecodeError),
}

pub trait MessageFormat: Sized {
    fn to_binary(&self) -> Result<Vec<u8>, MessageFormatError>;
    fn to_json(&self) -> Result<String, MessageFormatError>;
    fn to_base64(&self) -> Result<String, MessageFormatError>;

    fn from_binary(msg: &[u8]) -> Result<Self, MessageFormatError>;
    fn from_json(msg: &str) -> Result<Self, MessageFormatError>;
    fn from_base64(msg: &str) -> Result<Self, MessageFormatError>;
}

impl<T> MessageFormat for T
where T: DeserializeOwned + Serialize
{
    fn to_binary(&self) -> Result<Vec<u8>, MessageFormatError> {
        bincode::serialize(self).map_err(|_| MessageFormatError::BinarySerializeError)
    }

    fn to_json(&self) -> Result<String, MessageFormatError> {
        serde_json::to_string(self).map_err(MessageFormatError::JSONError)
    }

    fn to_base64(&self) -> Result<String, MessageFormatError> {
        let val = self.to_binary()?;
        Ok(base64::encode(&val))
    }

    fn from_binary(msg: &[u8]) -> Result<Self, MessageFormatError> {
        bincode::deserialize(msg).map_err(|_| MessageFormatError::BinaryDeserializeError)
    }

    fn from_json(msg: &str) -> Result<Self, MessageFormatError> {
        let mut de = serde_json::Deserializer::from_reader(msg.as_bytes());
        Deserialize::deserialize(&mut de).map_err(MessageFormatError::JSONError)
    }

    fn from_base64(msg: &str) -> Result<Self, MessageFormatError> {
        let buf = base64::decode(msg)?;
        Self::from_binary(&buf)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use base64::DecodeError as Base64Error;
    use serde::{Deserialize, Serialize};

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
        assert_eq!(
            msg,
            b"\x06\x00\x00\x00\x00\x00\x00\x00\x74\x77\x65\x6e\x74\x79\x14\x00\x00\x00\x00\x00\x00\x00\x00"
        );
        let val2 = TestMessage::from_binary(&msg).unwrap();
        assert_eq!(val, val2);
    }

    #[test]
    fn base64_simple() {
        let val = TestMessage::new("twenty", 20);
        let msg = val.to_base64().unwrap();
        assert_eq!(msg, "BgAAAAAAAAB0d2VudHkUAAAAAAAAAAA=");
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
        assert_eq!(
            msg_base64,
            "CAAAAAAAAAB0b21vcnJvdzIAAAAAAAAAAQUAAAAAAAAAdG9kYXlkAAAAAAAAAAA="
        );

        let msg_bin = val.to_binary().unwrap();
        assert_eq!(
            msg_bin,
            b"\x08\x00\x00\x00\x00\x00\x00\x00\x74\x6f\x6d\x6f\x72\x72\x6f\x77\x32\x00\x00\x00\x00\x00\x00\x00\x01\x05\x00\x00\x00\x00\x00\x00\x00\x74\x6f\x64\x61\x79\x64\x00\x00\x00\x00\x00\x00\x00\x00".to_vec()
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
            MessageFormatError::JSONError(e) => {
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
            MessageFormatError::Base64DeserializeError(Base64Error::InvalidByte(offset, val)) => {
                assert_eq!(offset, 5);
                assert_eq!(val, b'$');
            },
            _ => panic!("Base64 conversion should fail"),
        };

        let err = TestMessage::from_base64("j6h0b21vcnJvdzKTpXRvZGF5ZMA=").err().unwrap();
        match err {
            MessageFormatError::BinaryDeserializeError => {},
            _ => panic!("Base64 conversion should fail"),
        };
    }

    #[test]
    fn fail_binary() {
        let err = TestMessage::from_binary(b"").err().unwrap();
        match err {
            MessageFormatError::BinaryDeserializeError => {},
            _ => {
                panic!("Base64 conversion should fail");
            },
        }
    }
}
