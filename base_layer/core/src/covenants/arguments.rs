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

use std::{
    fmt::{Display, Formatter},
    io,
};

use integer_encoding::VarIntWriter;
use tari_common_types::types::{Commitment, PublicKey};
use tari_script::TariScript;
use tari_utilities::hex::{to_hex, Hex};

use crate::{
    consensus::{ConsensusDecoding, ConsensusEncoding, MaxSizeBytes},
    covenants::{
        byte_codes,
        covenant::Covenant,
        decoder::{CovenantDecodeError, CovenentReadExt},
        encoder::CovenentWriteExt,
        error::CovenantError,
        fields::{OutputField, OutputFields},
    },
};

const MAX_COVENANT_ARG_SIZE: usize = 4096;
const MAX_BYTES_ARG_SIZE: usize = 4096;

pub type Hash = [u8; 32];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CovenantArg {
    Hash(Hash),
    PublicKey(PublicKey),
    Commitment(Commitment),
    TariScript(TariScript),
    Covenant(Covenant),
    Uint(u64),
    OutputField(OutputField),
    OutputFields(OutputFields),
    Bytes(Vec<u8>),
}

impl CovenantArg {
    pub fn is_valid_code(code: u8) -> bool {
        byte_codes::is_valid_arg_code(code)
    }

    pub fn read_from<R: io::Read>(reader: &mut R, code: u8) -> Result<Self, CovenantDecodeError> {
        use byte_codes::*;
        match code {
            ARG_HASH => {
                let mut hash = [0u8; 32];
                reader.read_exact(&mut hash)?;
                Ok(CovenantArg::Hash(hash))
            },
            ARG_PUBLIC_KEY => {
                let pk = PublicKey::consensus_decode(reader)?;
                Ok(CovenantArg::PublicKey(pk))
            },
            ARG_COMMITMENT => Ok(CovenantArg::Commitment(Commitment::consensus_decode(reader)?)),
            ARG_TARI_SCRIPT => {
                let script = TariScript::consensus_decode(reader)?;
                Ok(CovenantArg::TariScript(script))
            },
            ARG_COVENANT => {
                let buf = reader.read_variable_length_bytes(MAX_COVENANT_ARG_SIZE)?;
                // Do not use consensus_decoding here because the compiler infinitely recurses to resolve the R generic,
                // R becomes the reader of this call and so on. This impl has an arg limit anyway and so is safe
                // TODO: Impose a limit on depth of covenants within covenants
                let covenant = Covenant::from_bytes(&buf)?;
                Ok(CovenantArg::Covenant(covenant))
            },
            ARG_UINT => {
                let v = u64::consensus_decode(reader)?;
                Ok(CovenantArg::Uint(v))
            },
            ARG_OUTPUT_FIELD => {
                let v = reader
                    .read_next_byte_code()?
                    .ok_or(CovenantDecodeError::UnexpectedEof {
                        expected: "Output field byte code",
                    })?;
                let field = OutputField::from_byte(v)?;
                Ok(CovenantArg::OutputField(field))
            },
            ARG_OUTPUT_FIELDS => {
                // Each field code is a byte
                let fields = OutputFields::read_from(reader)?;
                Ok(CovenantArg::OutputFields(fields))
            },
            ARG_BYTES => {
                let buf = MaxSizeBytes::<MAX_BYTES_ARG_SIZE>::consensus_decode(reader)?;
                Ok(CovenantArg::Bytes(buf.into()))
            },

            _ => Err(CovenantDecodeError::UnknownArgByteCode { code }),
        }
    }

    pub fn write_to<W: io::Write>(&self, writer: &mut W) -> Result<usize, io::Error> {
        use byte_codes::*;
        use CovenantArg::{Bytes, Commitment, Covenant, Hash, OutputField, OutputFields, PublicKey, TariScript, Uint};

        let mut written = 0;
        match self {
            Hash(hash) => {
                written += writer.write_u8_fixed(ARG_HASH)?;
                written += hash.len();
                writer.write_all(&hash[..])?;
            },
            PublicKey(pk) => {
                written += writer.write_u8_fixed(ARG_PUBLIC_KEY)?;
                written += pk.consensus_encode(writer)?;
            },
            Commitment(commitment) => {
                written += writer.write_u8_fixed(ARG_COMMITMENT)?;
                written += commitment.consensus_encode(writer)?;
            },
            TariScript(script) => {
                written += writer.write_u8_fixed(ARG_TARI_SCRIPT)?;
                written += script.consensus_encode(writer)?;
            },
            Covenant(covenant) => {
                written += writer.write_u8_fixed(ARG_COVENANT)?;
                let len = covenant.get_byte_length();
                written += writer.write_varint(len)?;
                written += covenant.write_to(writer)?;
            },
            Uint(int) => {
                written += writer.write_u8_fixed(ARG_UINT)?;
                written += int.consensus_encode(writer)?;
            },
            OutputField(field) => {
                written += writer.write_u8_fixed(ARG_OUTPUT_FIELD)?;
                written += writer.write_u8_fixed(field.as_byte())?;
            },
            OutputFields(fields) => {
                written += writer.write_u8_fixed(ARG_OUTPUT_FIELDS)?;
                written += fields.write_to(writer)?;
            },
            Bytes(bytes) => {
                written += writer.write_u8_fixed(ARG_BYTES)?;
                written += bytes.consensus_encode(writer)?;
            },
        }

        Ok(written)
    }
}

macro_rules! require_x_impl {
    ($name:ident, $output:ident, $expected: expr) => {
        #[allow(dead_code)]
        pub(super) fn $name(self) -> Result<$output, CovenantError> {
            match self {
                CovenantArg::$output(obj) => Ok(obj),
                got => Err(CovenantError::UnexpectedArgument {
                    expected: $expected,
                    got: got.to_string(),
                }),
            }
        }
    };
}

impl CovenantArg {
    require_x_impl!(require_hash, Hash, "hash");

    require_x_impl!(require_publickey, PublicKey, "publickey");

    require_x_impl!(require_commitment, Commitment, "commitment");

    require_x_impl!(require_tariscript, TariScript, "script");

    require_x_impl!(require_covenant, Covenant, "covenant");

    require_x_impl!(require_outputfield, OutputField, "outputfield");

    require_x_impl!(require_outputfields, OutputFields, "outputfields");

    pub fn require_bytes(self) -> Result<Vec<u8>, CovenantError> {
        match self {
            CovenantArg::Bytes(val) => Ok(val),
            got => Err(CovenantError::UnexpectedArgument {
                expected: "bytes",
                got: got.to_string(),
            }),
        }
    }

    pub fn require_uint(self) -> Result<u64, CovenantError> {
        match self {
            CovenantArg::Uint(val) => Ok(val),
            got => Err(CovenantError::UnexpectedArgument {
                expected: "uint",
                got: got.to_string(),
            }),
        }
    }
}

impl Display for CovenantArg {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use CovenantArg::{Bytes, Commitment, Covenant, Hash, OutputField, OutputFields, PublicKey, TariScript, Uint};
        match self {
            Hash(hash) => write!(f, "Hash({})", to_hex(&hash[..])),
            PublicKey(public_key) => write!(f, "PublicKey({})", public_key.to_hex()),
            Commitment(commitment) => write!(f, "Commitment({})", commitment.to_hex()),
            TariScript(_) => write!(f, "TariScript(...)"),
            Covenant(_) => write!(f, "Covenant(...)"),
            Uint(v) => write!(f, "Uint({})", v),
            OutputField(field) => write!(f, "OutputField({})", field.as_byte()),
            OutputFields(fields) => write!(f, "OutputFields({} field(s))", fields.len()),
            Bytes(bytes) => write!(f, "Bytes({} byte(s))", bytes.len()),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    mod require_x_impl {
        use super::*;

        #[test]
        fn test() {
            // This is mostly to remove unused function warnings
            let arg = CovenantArg::Uint(123);
            arg.clone().require_bytes().unwrap_err();
            let v = arg.clone().require_uint().unwrap();
            assert_eq!(v, 123);
            arg.clone().require_hash().unwrap_err();
            arg.clone().require_outputfield().unwrap_err();
            arg.clone().require_covenant().unwrap_err();
            arg.clone().require_commitment().unwrap_err();
            arg.clone().require_outputfields().unwrap_err();
            arg.clone().require_publickey().unwrap_err();
            arg.require_tariscript().unwrap_err();
        }
    }

    mod write_to {
        use tari_common_types::types::Commitment;
        use tari_script::script;
        use tari_utilities::hex::from_hex;

        use super::*;
        use crate::{covenant, covenants::byte_codes::*};

        fn test_case(arg: CovenantArg, expected: &[u8]) {
            let mut buf = Vec::new();
            arg.write_to(&mut buf).unwrap();
            assert_eq!(buf, expected);
        }

        #[test]
        fn test() {
            test_case(CovenantArg::Uint(2048), &[ARG_UINT, 0x80, 0x10][..]);
            test_case(
                CovenantArg::Covenant(covenant!(identity())),
                &[ARG_COVENANT, 0x01, 0x20][..],
            );
            test_case(
                CovenantArg::Bytes(vec![0x01, 0x02, 0xaa]),
                &[ARG_BYTES, 0x03, 0x01, 0x02, 0xaa][..],
            );
            test_case(
                CovenantArg::Commitment(Commitment::default()),
                &from_hex("030000000000000000000000000000000000000000000000000000000000000000").unwrap(),
            );
            test_case(
                CovenantArg::PublicKey(PublicKey::default()),
                &from_hex("020000000000000000000000000000000000000000000000000000000000000000").unwrap(),
            );
            test_case(
                CovenantArg::Hash([0u8; 32]),
                &from_hex("010000000000000000000000000000000000000000000000000000000000000000").unwrap(),
            );
            test_case(CovenantArg::TariScript(script!(Nop)), &[ARG_TARI_SCRIPT, 0x01, 0x73]);
            test_case(CovenantArg::OutputField(OutputField::Covenant), &[
                ARG_OUTPUT_FIELD,
                FIELD_COVENANT,
            ]);
            test_case(
                CovenantArg::OutputFields(OutputFields::from(vec![OutputField::Features, OutputField::Commitment])),
                &[ARG_OUTPUT_FIELDS, 0x02, FIELD_FEATURES, FIELD_COMMITMENT],
            );
        }
    }
}
