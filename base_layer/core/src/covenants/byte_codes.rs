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
    (0x01..=0x0a).contains(&code)
}
pub const ARG_HASH: u8 = 0x01;
pub const ARG_PUBLIC_KEY: u8 = 0x02;
pub const ARG_COMMITMENT: u8 = 0x03;
pub const ARG_TARI_SCRIPT: u8 = 0x04;
pub const ARG_COVENANT: u8 = 0x05;
pub const ARG_UINT: u8 = 0x06;
pub const ARG_OUTPUT_FIELD: u8 = 0x07;
pub const ARG_OUTPUT_FIELDS: u8 = 0x08;
pub const ARG_BYTES: u8 = 0x09;

//---------------------------------- FILTER byte codes --------------------------------------------//

pub(super) fn is_valid_filter_code(code: u8) -> bool {
    (0x20..=0x24).contains(&code) || (0x30..=0x34).contains(&code)
}

pub const FILTER_IDENTITY: u8 = 0x20;
pub const FILTER_AND: u8 = 0x21;
pub const FILTER_OR: u8 = 0x22;
pub const FILTER_XOR: u8 = 0x23;
pub const FILTER_NOT: u8 = 0x24;

pub const FILTER_OUTPUT_HASH_EQ: u8 = 0x30;
pub const FILTER_FIELDS_PRESERVED: u8 = 0x31;
pub const FILTER_FIELDS_HASHED_EQ: u8 = 0x32;
pub const FILTER_FIELD_EQ: u8 = 0x33;
pub const FILTER_ABSOLUTE_HEIGHT: u8 = 0x34;

//---------------------------------- FIELD byte codes --------------------------------------------//
pub const FIELD_COMMITMENT: u8 = 0x00;
pub const FIELD_SCRIPT: u8 = 0x01;
pub const FIELD_SENDER_OFFSET_PUBLIC_KEY: u8 = 0x02;
pub const FIELD_COVENANT: u8 = 0x03;
pub const FIELD_FEATURES: u8 = 0x04;
pub const FIELD_FEATURES_FLAGS: u8 = 0x05;
pub const FIELD_FEATURES_MATURITY: u8 = 0x06;
pub const FIELD_FEATURES_UNIQUE_ID: u8 = 0x07;
pub const FIELD_FEATURES_PARENT_PUBLIC_KEY: u8 = 0x08;
pub const FIELD_FEATURES_METADATA: u8 = 0x09;
