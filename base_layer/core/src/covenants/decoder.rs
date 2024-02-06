//  Copyright 2021, The Tari Project
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

use std::io;

use integer_encoding::VarIntReader;
use tari_script::ScriptError;

use crate::covenants::token::CovenantToken;

/// Covenant Token decoder.
pub struct CovenantTokenDecoder<'a, R> {
    buf: &'a mut R,
    is_complete: bool,
}

impl<'a, R: io::Read> CovenantTokenDecoder<'a, R> {
    /// Given a read buffer, it creates a new instance of `CovenantTokenDecoder`.
    pub fn new(buf: &'a mut R) -> Self {
        Self {
            buf,
            is_complete: false,
        }
    }
}

impl Iterator for CovenantTokenDecoder<'_, &[u8]> {
    type Item = Result<CovenantToken, CovenantDecodeError>;

    /// Returns the next item in `CovenantTokenDecoder`'s buffer. If it is complete,
    /// it returns `None`.
    fn next(&mut self) -> Option<Self::Item> {
        if self.is_complete {
            return None;
        }

        match CovenantToken::read_from(self.buf) {
            Ok(Some(token)) => Some(Ok(token)),
            Ok(None) => {
                self.is_complete = true;
                None
            },
            Err(err) => {
                self.is_complete = true;
                Some(Err(err))
            },
        }
    }
}

#[derive(Debug, thiserror::Error)]
/// Error enum for covenant decoding possible failure scenarios.
pub enum CovenantDecodeError {
    #[error("Unknown filter byte code {code}")]
    UnknownFilterByteCode { code: u8 },
    #[error("Unknown arg byte code {code}")]
    UnknownArgByteCode { code: u8 },
    #[error("Unknown byte code {code}")]
    UnknownByteCode { code: u8 },
    #[error("Unexpected EoF, expected {expected}")]
    UnexpectedEof { expected: &'static str },
    #[error("Tari script error: {0}")]
    ScriptError(#[from] ScriptError),
    #[error("Covenant exceeded maximum bytes")]
    ExceededMaxBytes,
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// Trait `CovenantReadExt`. Contains two interface methods, `read_next_byte_code`
/// and `read_variable_length_bytes`.
pub(super) trait CovenantReadExt: io::Read {
    fn read_next_byte_code(&mut self) -> Result<Option<u8>, io::Error>;
    fn read_variable_length_bytes(&mut self, size: usize) -> Result<Vec<u8>, io::Error>;
}

impl<R: io::Read> CovenantReadExt for R {
    /// Reads next byte code
    #[allow(clippy::unused_io_amount)]
    fn read_next_byte_code(&mut self) -> Result<Option<u8>, io::Error> {
        let mut buf = [0u8; 1];
        loop {
            // This is what read_exact does, except that if we read 0 bytes, we return None instead of an UnexpectedEof
            // error
            match self.read(&mut buf) {
                Ok(0) => return Ok(None),
                Ok(1) => return Ok(Some(buf[0])),
                Ok(_) => unreachable!("buffer size is 1 but more bytes were read!?"),
                Err(ref err) if err.kind() == io::ErrorKind::Interrupted => {},
                Err(err) => return Err(err),
            }
        }
    }

    /// Reads a variable length byte array
    fn read_variable_length_bytes(&mut self, max_size: usize) -> Result<Vec<u8>, io::Error> {
        let len = self.read_varint::<u16>()? as usize;
        if len > max_size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "Received variable length bytes that exceed {} bytes (max: {})",
                    len, max_size
                ),
            ));
        }
        let mut buf = vec![0u8; len];
        self.read_exact(&mut buf)?;
        Ok(buf)
    }
}

#[cfg(test)]
mod test {
    use tari_common_types::types::FixedHash;
    use tari_test_utils::unpack_enum;
    use tari_utilities::hex::{to_hex, Hex};

    use super::*;
    use crate::{
        covenant,
        covenants::{
            arguments::CovenantArg,
            byte_codes::ARG_OUTPUT_FIELD,
            fields::OutputField,
            filters::CovenantFilter,
        },
    };

    #[test]
    fn it_immediately_ends_iterator_given_empty_bytes() {
        let buf = &[] as &[u8; 0];
        assert!(CovenantTokenDecoder::new(&mut &buf[..]).next().is_none());
    }

    #[test]
    fn it_ends_after_an_error() {
        let buf = &[0xffu8];
        let mut reader = &buf[..];
        let mut decoder = CovenantTokenDecoder::new(&mut reader);
        assert!(matches!(decoder.next(), Some(Err(_))));
        assert!(decoder.next().is_none());
    }

    #[test]
    fn it_returns_an_error_if_arg_expected() {
        let buf = &[ARG_OUTPUT_FIELD];
        let mut reader = &buf[..];
        let mut decoder = CovenantTokenDecoder::new(&mut reader);

        assert!(matches!(
            decoder.next(),
            Some(Err(CovenantDecodeError::UnexpectedEof { .. }))
        ));
        assert!(decoder.next().is_none());
    }

    #[test]
    fn it_decodes_from_well_formed_bytes() {
        let hash = FixedHash::from_hex("53563b674ba8e5166adb57afa8355bcf2ee759941eef8f8959b802367c2558bd").unwrap();
        let mut bytes = Vec::new();
        covenant!(fields_hashed_eq(
            @fields(@field::commitment),
            @hash(hash),
        ))
        .write_to(&mut bytes)
        .unwrap();
        let mut buf = bytes.as_slice();
        let mut decoder = CovenantTokenDecoder::new(&mut buf);
        let token = decoder.next().unwrap().unwrap();
        assert!(matches!(
            token,
            CovenantToken::Filter(CovenantFilter::FieldsHashedEq(_))
        ));
        let token = decoder.next().unwrap().unwrap();
        unpack_enum!(CovenantArg::OutputFields(fields) = token.as_arg().unwrap());
        assert_eq!(fields.fields(), &[OutputField::Commitment]);

        let token = decoder.next().unwrap().unwrap();
        unpack_enum!(CovenantArg::Hash(hash) = token.as_arg().unwrap());
        assert_eq!(
            to_hex(&hash[..]),
            "53563b674ba8e5166adb57afa8355bcf2ee759941eef8f8959b802367c2558bd"
        );

        assert!(decoder.next().is_none());
    }

    mod covenant_read_ext {
        use super::*;

        #[test]
        fn it_reads_bytes_with_length_prefix() {
            let data = vec![0x03u8, 0x01, 0x02, 0x03];
            let bytes = CovenantReadExt::read_variable_length_bytes(&mut data.as_slice(), 3).unwrap();
            assert_eq!(bytes, [1u8, 2, 3]);
        }

        #[test]
        fn it_errors_if_len_byte_exceeds_maximum() {
            let data = vec![0x02, 0x01];
            CovenantReadExt::read_variable_length_bytes(&mut data.as_slice(), 1).unwrap_err();
        }
    }
}
