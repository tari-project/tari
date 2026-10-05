// Copyright 2020. The Tari Project
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use serde::{Serialize, Serializer};
use tari_max_size::{EncodedBytes, ValidatedDecode, impl_validated_decode};
use tari_utilities::hex::Hex;

use crate::{ExecutionStack, MAX_SCRIPT_BYTES, ScriptError, TariScript, stack::MAX_STACK_BYTES};

impl Serialize for TariScript {
    fn serialize<S>(&self, ser: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        let script_bin = self.to_bytes();
        if ser.is_human_readable() {
            ser.serialize_str(&script_bin.to_hex())
        } else {
            ser.serialize_bytes(&script_bin)
        }
    }
}

/// serde: a hex string or a byte array; borsh: the bytes behind a varint length prefix of at most
/// [`MAX_SCRIPT_BYTES`]. Both decoders then apply [`TariScript::from_bytes`].
impl ValidatedDecode for TariScript {
    type Error = ScriptError;
    type Raw = EncodedBytes<MAX_SCRIPT_BYTES>;

    fn validate(raw: Self::Raw) -> Result<Self, Self::Error> {
        TariScript::from_bytes(raw.as_bytes())
    }
}

impl_validated_decode!(TariScript);

// -------------------------------- ExecutionStack -------------------------------- //
impl Serialize for ExecutionStack {
    fn serialize<S>(&self, ser: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        let stack_bin = self.to_bytes();
        if ser.is_human_readable() {
            ser.serialize_str(&stack_bin.to_hex())
        } else {
            ser.serialize_bytes(&stack_bin)
        }
    }
}

/// serde: a hex string or a byte array; borsh: the bytes behind a varint length prefix of at most
/// [`MAX_STACK_BYTES`], the largest encoding of a valid stack. Both decoders then apply [`ExecutionStack::from_bytes`],
/// which applies the item limit and validates every item.
impl ValidatedDecode for ExecutionStack {
    type Error = ScriptError;
    type Raw = EncodedBytes<MAX_STACK_BYTES>;

    fn validate(raw: Self::Raw) -> Result<Self, Self::Error> {
        ExecutionStack::from_bytes(raw.as_bytes())
    }
}

impl_validated_decode!(ExecutionStack);
