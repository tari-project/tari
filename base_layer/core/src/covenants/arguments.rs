//  Copyright 2021, The Taiji Project
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

use borsh::{BorshDeserialize, BorshSerialize};
use integer_encoding::VarIntWriter;
use taiji_common_types::types::{Commitment, FixedHash, PublicKey};
use taiji_script::TaijiScript;
use tari_utilities::{
    hex::{to_hex, Hex},
    ByteArray,
};

use super::decoder::CovenantDecodeError;
use crate::{
    consensus::MaxSizeBytes,
    covenants::{
        byte_codes,
        covenant::Covenant,
        decoder::CovenantReadExt,
        encoder::CovenentWriteExt,
        error::CovenantError,
        fields::{OutputField, OutputFields},
    },
    transactions::transaction_components::OutputType,
};

const MAX_COVENANT_ARG_SIZE: usize = 4096;
const MAX_BYTES_ARG_SIZE: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
/// Covenant arguments
pub enum CovenantArg {
    Hash(FixedHash),
    PublicKey(PublicKey),
    Commitment(Commitment),
    TaijiScript(TaijiScript),
    Covenant(Covenant),
    OutputType(OutputType),
    Uint(u64),
    OutputField(OutputField),
    OutputFields(OutputFields),
    Bytes(Vec<u8>),
}

impl CovenantArg {
    /// Checks if a stream of bytes results in valid argument code
    pub fn is_valid_code(code: u8) -> bool {
        byte_codes::is_valid_arg_code(code)
    }

    /// Reads a `CovenantArg` from a buffer of bytes
    pub fn read_from(reader: &mut &[u8], code: u8) -> Result<Self, CovenantDecodeError> {
        use byte_codes::*;
        match code {
            ARG_HASH => {
                // let mut hash = [0u8; 32];
                let hash: [u8; 32] = BorshDeserialize::deserialize(reader)?;
                Ok(CovenantArg::Hash(hash.into()))
            },
            ARG_PUBLIC_KEY => {
                let pk = PublicKey::deserialize(reader)?;
                Ok(CovenantArg::PublicKey(pk))
            },
            ARG_COMMITMENT => Ok(CovenantArg::Commitment(Commitment::deserialize(reader)?)),
            ARG_TARI_SCRIPT => {
                let script = TaijiScript::deserialize(reader)?;
                Ok(CovenantArg::TaijiScript(script))
            },
            ARG_COVENANT => {
                let buf = reader.read_variable_length_bytes(MAX_COVENANT_ARG_SIZE)?;
                // Do not use consensus_decoding here because the compiler infinitely recurses to resolve the R generic,
                // R becomes the reader of this call and so on. This impl has an arg limit anyway and so is safe
                let covenant = Covenant::from_bytes(&mut buf.as_bytes())?;
                Ok(CovenantArg::Covenant(covenant))
            },
            ARG_OUTPUT_TYPE => {
                let output_type = OutputType::deserialize(reader)?;
                Ok(CovenantArg::OutputType(output_type))
            },
            ARG_UINT => {
                let v = u64::deserialize(reader)?;
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
                let buf = MaxSizeBytes::<MAX_BYTES_ARG_SIZE>::deserialize(reader)?;
                Ok(CovenantArg::Bytes(buf.into()))
            },

            _ => Err(CovenantDecodeError::UnknownArgByteCode { code }),
        }
    }

    /// Parses the `CovenantArg` data to bytes and writes it to an IO writer
    pub fn write_to<W: io::Write>(&self, writer: &mut W) -> Result<(), io::Error> {
        use byte_codes::*;
        #[allow(clippy::enum_glob_use)]
        use CovenantArg::*;

        match self {
            Hash(hash) => {
                writer.write_u8_fixed(ARG_HASH)?;
                writer.write_all(&hash[..])?;
            },
            PublicKey(pk) => {
                writer.write_u8_fixed(ARG_PUBLIC_KEY)?;
                pk.serialize(writer)?;
            },
            Commitment(commitment) => {
                writer.write_u8_fixed(ARG_COMMITMENT)?;
                commitment.serialize(writer)?;
            },
            TaijiScript(script) => {
                writer.write_u8_fixed(ARG_TARI_SCRIPT)?;
                script.serialize(writer)?;
            },
            Covenant(covenant) => {
                writer.write_u8_fixed(ARG_COVENANT)?;
                let len = covenant.get_byte_length();
                writer.write_varint(len)?;
                covenant.write_to(writer)?;
            },
            OutputType(output_type) => {
                writer.write_u8_fixed(ARG_OUTPUT_TYPE)?;
                output_type.serialize(writer)?;
            },
            Uint(int) => {
                writer.write_u8_fixed(ARG_UINT)?;
                int.serialize(writer)?;
            },
            OutputField(field) => {
                writer.write_u8_fixed(ARG_OUTPUT_FIELD)?;
                writer.write_u8_fixed(field.as_byte())?;
            },
            OutputFields(fields) => {
                writer.write_u8_fixed(ARG_OUTPUT_FIELDS)?;
                fields.write_to(writer)?;
            },
            Bytes(bytes) => {
                writer.write_u8_fixed(ARG_BYTES)?;
                bytes.serialize(writer)?;
            },
        }

        Ok(())
    }
}

/// `require_x_impl!` is a helper macro that generates an implementation of a function with a specific signature
/// based on the provided input parameters. Functionality:
/// The macro expects to receive either three or four arguments.
///     $name, represents the name of the function to be generated.
///     $output, represents the name of the enum variant that the function will match against.
///     $expected, represents an expression that will be used in the error message when the provided argument
///         does not match the expected variant.
///     (optional) $output_type, represents the type that the function will return. If
///         not provided, it defaults to the same as $output.
macro_rules! require_x_impl {
    ($name:ident, $output:ident, $expected: expr, $output_type:ty) => {
        #[allow(dead_code)]
        pub(super) fn $name(self) -> Result<$output_type, CovenantError> {
            match self {
                CovenantArg::$output(obj) => Ok(obj),
                got => Err(CovenantError::UnexpectedArgument {
                    expected: $expected,
                    got: got.to_string(),
                }),
            }
        }
    };
    ($name:ident, $output:ident, $expected:expr) => {
        require_x_impl!($name, $output, $expected, $output);
    };
}

impl CovenantArg {
    require_x_impl!(require_hash, Hash, "hash", FixedHash);

    require_x_impl!(require_publickey, PublicKey, "publickey");

    require_x_impl!(require_commitment, Commitment, "commitment");

    require_x_impl!(require_taijiscript, TaijiScript, "script");

    require_x_impl!(require_covenant, Covenant, "covenant");

    require_x_impl!(require_output_type, OutputType, "output_type");

    require_x_impl!(require_outputfield, OutputField, "outputfield");

    require_x_impl!(require_outputfields, OutputFields, "outputfields");

    require_x_impl!(require_bytes, Bytes, "bytes", Vec<u8>);

    require_x_impl!(require_uint, Uint, "u64", u64);
}

impl Display for CovenantArg {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        #[allow(clippy::enum_glob_use)]
        use CovenantArg::*;
        match self {
            Hash(hash) => write!(f, "Hash({})", to_hex(&hash[..])),
            PublicKey(public_key) => write!(f, "PublicKey({})", public_key.to_hex()),
            Commitment(commitment) => write!(f, "Commitment({})", commitment.to_hex()),
            TaijiScript(_) => write!(f, "TaijiScript(...)"),
            Covenant(_) => write!(f, "Covenant(...)"),
            Uint(v) => write!(f, "Uint({})", v),
            OutputField(field) => write!(f, "OutputField({})", field.as_byte()),
            OutputFields(fields) => write!(f, "OutputFields({} field(s))", fields.len()),
            Bytes(bytes) => write!(f, "Bytes({} byte(s))", bytes.len()),
            OutputType(output_type) => write!(f, "OutputType({})", output_type),
        }
    }
}

#[cfg(test)]
mod test {
    use taiji_common_types::types::Commitment;
    use taiji_script::script;
    use tari_utilities::hex::from_hex;

    use super::*;
    use crate::{covenant, covenants::byte_codes::*};

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
            arg.require_taijiscript().unwrap_err();
        }
    }

    mod write_to_and_read_from {
        use super::*;

        fn test_case(argument: CovenantArg, mut data: &[u8]) {
            let mut buf = Vec::new();
            argument.write_to(&mut buf).unwrap();
            assert_eq!(buf, data);

            let reader = &mut data;
            let code = reader.read_next_byte_code().unwrap().unwrap();
            let arg = CovenantArg::read_from(&mut data, code).unwrap();
            assert_eq!(arg, argument);
        }

        #[test]
        fn test() {
            test_case(CovenantArg::Uint(2048), &[ARG_UINT, 0, 8, 0, 0, 0, 0, 0, 0][..]);
            test_case(
                CovenantArg::Covenant(covenant!(identity())),
                &[ARG_COVENANT, 0x01, 0x20][..],
            );
            test_case(
                CovenantArg::Bytes(vec![0x01, 0x02, 0xaa]),
                &[ARG_BYTES, 0x03, 0x00, 0x00, 0x00, 0x01, 0x02, 0xaa][..],
            );
            test_case(
                CovenantArg::Commitment(Commitment::default()),
                &from_hex("03200000000000000000000000000000000000000000000000000000000000000000000000").unwrap(),
            );
            test_case(
                CovenantArg::PublicKey(PublicKey::default()),
                &from_hex("02200000000000000000000000000000000000000000000000000000000000000000000000").unwrap(),
            );
            test_case(
                CovenantArg::Hash(FixedHash::zero()),
                &from_hex("010000000000000000000000000000000000000000000000000000000000000000").unwrap(),
            );
            test_case(CovenantArg::TaijiScript(script!(Nop)), &[ARG_TARI_SCRIPT, 0x01, 0x73]);
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
