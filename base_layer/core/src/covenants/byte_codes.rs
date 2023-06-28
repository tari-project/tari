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

//---------------------------------- ARG byte codes --------------------------------------------//
pub(super) fn is_valid_arg_code(code: u8) -> bool {
    ALL_ARGS.contains(&code)
}

/// Array with all possible covenant arg byte codes.
pub(super) const ALL_ARGS: [u8; 10] = [
    ARG_HASH,
    ARG_PUBLIC_KEY,
    ARG_COMMITMENT,
    ARG_TARI_SCRIPT,
    ARG_COVENANT,
    ARG_UINT,
    ARG_OUTPUT_FIELD,
    ARG_OUTPUT_FIELDS,
    ARG_BYTES,
    ARG_OUTPUT_TYPE,
];

/// Covenant arg hash byte code.
pub const ARG_HASH: u8 = 0x01;
/// Covenant arg public key byte code.
pub const ARG_PUBLIC_KEY: u8 = 0x02;
/// Covenant arg commitment byte code.
pub const ARG_COMMITMENT: u8 = 0x03;
/// Covenant arg tari script byte code.
pub const ARG_TARI_SCRIPT: u8 = 0x04;
/// Covenant arg covenant byte code.
pub const ARG_COVENANT: u8 = 0x05;
/// Covenant arg uint byte code.
pub const ARG_UINT: u8 = 0x06;
/// Covenant arg output field byte code.
pub const ARG_OUTPUT_FIELD: u8 = 0x07;
/// Covenant arg output fields byte code.
pub const ARG_OUTPUT_FIELDS: u8 = 0x08;
/// Covenant arg bytes byte code.
pub const ARG_BYTES: u8 = 0x09;
/// Covenant arg output type byte code.
pub const ARG_OUTPUT_TYPE: u8 = 0x0a;

//---------------------------------- FILTER byte codes --------------------------------------------//
/// Checks if a byte value results in a valid argument byte code
pub(super) fn is_valid_filter_code(code: u8) -> bool {
    ALL_FILTERS.contains(&code)
}

/// Array with all possible covenant filter bytecodes.
pub(super) const ALL_FILTERS: [u8; 10] = [
    FILTER_IDENTITY,
    FILTER_AND,
    FILTER_OR,
    FILTER_XOR,
    FILTER_NOT,
    FILTER_OUTPUT_HASH_EQ,
    FILTER_FIELDS_PRESERVED,
    FILTER_FIELDS_HASHED_EQ,
    FILTER_FIELD_EQ,
    FILTER_ABSOLUTE_HEIGHT,
];

/// Identity filter.
pub const FILTER_IDENTITY: u8 = 0x20;
/// And filter.
pub const FILTER_AND: u8 = 0x21;
/// Or filter.
pub const FILTER_OR: u8 = 0x22;
/// Xor Filter.
pub const FILTER_XOR: u8 = 0x23;
/// Not filter.
pub const FILTER_NOT: u8 = 0x24;

/// Output hash equality filter.
pub const FILTER_OUTPUT_HASH_EQ: u8 = 0x30;
/// Fields preserved filter.
pub const FILTER_FIELDS_PRESERVED: u8 = 0x31;
/// Fields hashed equality filter.
pub const FILTER_FIELDS_HASHED_EQ: u8 = 0x32;
/// Field equality filter.
pub const FILTER_FIELD_EQ: u8 = 0x33;
/// Absolute height filter.
pub const FILTER_ABSOLUTE_HEIGHT: u8 = 0x34;

//---------------------------------- FIELD byte codes --------------------------------------------//
/// Field commitment.
pub const FIELD_COMMITMENT: u8 = 0x00;
/// Field script.
pub const FIELD_SCRIPT: u8 = 0x01;
/// Field sender offset public key.
pub const FIELD_SENDER_OFFSET_PUBLIC_KEY: u8 = 0x02;
/// Field covenant.
pub const FIELD_COVENANT: u8 = 0x03;
/// Field features.
pub const FIELD_FEATURES: u8 = 0x04;
/// Field features output type.
pub const FIELD_FEATURES_OUTPUT_TYPE: u8 = 0x05;
/// Field features maturity.
pub const FIELD_FEATURES_MATURITY: u8 = 0x06;
/// Field features side chain features.
pub const FIELD_FEATURES_SIDE_CHAIN_FEATURES: u8 = 0x08;

#[cfg(test)]
mod tests {
    use super::*;

    mod is_valid_filter_code {
        use super::*;

        #[test]
        fn it_returns_true_for_all_filter_codes() {
            ALL_FILTERS.iter().for_each(|code| {
                assert!(is_valid_filter_code(*code));
            });
        }

        #[test]
        fn it_returns_false_for_all_arg_codes() {
            ALL_ARGS.iter().for_each(|code| {
                assert!(!is_valid_filter_code(*code));
            });
        }
    }

    mod is_valid_arg_code {
        use super::*;

        #[test]
        fn it_returns_false_for_all_filter_codes() {
            ALL_FILTERS.iter().for_each(|code| {
                assert!(!is_valid_arg_code(*code));
            });
        }

        #[test]
        fn it_returns_true_for_all_arg_codes() {
            ALL_ARGS.iter().for_each(|code| {
                assert!(is_valid_arg_code(*code));
            });
        }
    }
}
