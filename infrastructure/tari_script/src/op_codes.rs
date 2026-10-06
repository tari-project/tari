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

use std::{fmt, ops::Deref};

use integer_encoding::VarInt;
use serde::{Deserialize, Serialize};
use tari_crypto::{compressed_key::CompressedKey, ristretto::RistrettoPublicKey, tari_utilities::ByteArray};
use tari_max_size::MaxSizeVecError;
use tari_utilities::{ByteArrayError, hex::Hex};

use super::ScriptError;

pub type HashValue = [u8; 32];
pub type ScalarValue = [u8; 32];
pub type Message = [u8; MESSAGE_LENGTH];

const PUBLIC_KEY_LENGTH: usize = 32;
const MESSAGE_LENGTH: usize = 32;
type MultiSigArgs = (u8, u8, Vec<CompressedKey<RistrettoPublicKey>>, Box<Message>, usize);

/// Convert a slice into a HashValue.
///
/// Returns [ScriptError::InvalidData] if the slice is not exactly 32 bytes long.
pub fn slice_to_hash(slice: &[u8]) -> Result<HashValue, ScriptError> {
    HashValue::try_from(slice).map_err(|_| ScriptError::InvalidData)
}

/// Convert a slice into a Boxed HashValue.
///
/// Returns [ScriptError::InvalidData] if the slice is not exactly 32 bytes long.
pub fn slice_to_boxed_hash(slice: &[u8]) -> Result<Box<HashValue>, ScriptError> {
    Ok(Box::new(slice_to_hash(slice)?))
}

/// Convert a slice into a Message.
///
/// Returns [ScriptError::InvalidData] if the slice is not exactly `MESSAGE_LENGTH` (32) bytes long.
pub fn slice_to_message(slice: &[u8]) -> Result<Message, ScriptError> {
    Message::try_from(slice).map_err(|_| ScriptError::InvalidData)
}

/// Convert a slice into a Boxed Message.
///
/// Returns [ScriptError::InvalidData] if the slice is not exactly `MESSAGE_LENGTH` (32) bytes long.
pub fn slice_to_boxed_message(slice: &[u8]) -> Result<Box<Message>, ScriptError> {
    Ok(Box::new(slice_to_message(slice)?))
}

/// Returns the bytes following an opcode byte and the `size`-byte varint that succeeds it.
fn bytes_after_varint(bytes: &[u8], size: usize) -> Result<&[u8], ScriptError> {
    bytes
        .get(size.checked_add(1).ok_or(ScriptError::InvalidData)?..)
        .ok_or(ScriptError::InvalidData)
}

/// Convert a slice into a vector of Public Keys.
pub fn slice_to_vec_pubkeys(slice: &[u8], num: usize) -> Result<Vec<CompressedKey<RistrettoPublicKey>>, ScriptError> {
    let required_len = num.checked_mul(PUBLIC_KEY_LENGTH).ok_or(ScriptError::InvalidData)?;
    if slice.len() < required_len {
        return Err(ScriptError::InvalidData);
    }

    let public_keys = slice
        .as_chunks::<PUBLIC_KEY_LENGTH>()
        .0
        .iter()
        .take(num)
        .map(|chunk| CompressedKey::from_canonical_bytes(chunk))
        .collect::<Result<Vec<CompressedKey<RistrettoPublicKey>>, ByteArrayError>>()?;

    Ok(public_keys)
}

// Opcode constants: Block Height Checks
const OP_CHECK_HEIGHT_VERIFY: u8 = 0x66;
const OP_CHECK_HEIGHT: u8 = 0x67;
const OP_COMPARE_HEIGHT_VERIFY: u8 = 0x68;
const OP_COMPARE_HEIGHT: u8 = 0x69;

// Opcode constants: Stack Manipulation
const OP_DROP: u8 = 0x70;
const OP_DUP: u8 = 0x71;
const OP_REV_ROT: u8 = 0x72;
const OP_PUSH_HASH: u8 = 0x7a;
const OP_PUSH_ZERO: u8 = 0x7b;
const OP_NOP: u8 = 0x73;
const OP_PUSH_ONE: u8 = 0x7c;
const OP_PUSH_INT: u8 = 0x7d;
const OP_PUSH_PUBKEY: u8 = 0x7e;

// Opcode constants: Math Operations
const OP_EQUAL: u8 = 0x80;
const OP_EQUAL_VERIFY: u8 = 0x81;
const OP_ADD: u8 = 0x93;
const OP_SUB: u8 = 0x94;
const OP_GE_ZERO: u8 = 0x82;
const OP_GT_ZERO: u8 = 0x83;
const OP_LE_ZERO: u8 = 0x84;
const OP_LT_ZERO: u8 = 0x85;

// Opcode constants: Boolean Logic
pub const OP_OR_VERIFY: u8 = 0x64;
pub const OP_OR: u8 = 0x65;

// Opcode constants: Cryptographic Operations
const OP_CHECK_SIG: u8 = 0xac;
const OP_CHECK_SIG_VERIFY: u8 = 0xad;
const OP_CHECK_MULTI_SIG: u8 = 0xae;
const OP_CHECK_MULTI_SIG_VERIFY: u8 = 0xaf;
const OP_HASH_BLAKE256: u8 = 0xb0;
const OP_HASH_SHA256: u8 = 0xb1;
const OP_HASH_SHA3: u8 = 0xb2;
const OP_TO_RISTRETTO_POINT: u8 = 0xb3;
const OP_CHECK_MULTI_SIG_VERIFY_AGGREGATE_PUB_KEY: u8 = 0xb4;

// Opcode constants: Miscellaneous
const OP_RETURN: u8 = 0x60;
const OP_IF_THEN: u8 = 0x61;
const OP_ELSE: u8 = 0x62;
const OP_END_IF: u8 = 0x63;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Opcode {
    // Block Height Checks
    /// Compare the current block height to `height`. Fails with
    /// `IncompatibleTypes` if u64 is not a valid 64-bit unsigned integer. Fails with `VerifyFailed` if the block
    /// height < `height`.
    CheckHeightVerify(u64),
    /// Pushes the value of (the current tip height - `height`) to the stack. In
    /// other words, the top of the stack will hold the height difference between `height` and the current height.
    /// If the chain has progressed beyond `height`, the value is positive; and negative if the chain has yet to
    /// reach `height`. Fails with `IncompatibleTypes` if u64 is not a valid 64-bit unsigned integer. Fails with
    /// `StackOverflow` if the stack would exceed the max stack height.
    CheckHeight(u64),
    /// Pops the top of the stack as an unsigned (u64) `height` and compares it to the current block height.
    /// Fails with `StackUnderflow` if the stack is empty. Fails with `InvalidInput` if the top of the stack is not a
    /// `Number`. Fails with `ValueExceedsBounds` if the `Number` is negative (it cannot be converted to a u64).
    /// Fails with `VerifyFailed` if the block height < `height`.
    ///
    /// Note that this differs from `CompareHeight`, which pops a signed (i64) value.
    CompareHeightVerify,
    /// Pops the top of the stack as a signed (i64) `height`, then pushes the value of (the current height - `height`)
    /// to the stack. In other words, this opcode replaces the top of the stack with the difference between `height`
    /// and the current height; `height` may be negative. Fails with `StackUnderflow` if the stack is empty. Fails with
    /// `InvalidInput` if the top of the stack is not a `Number`. Fails with `ValueExceedsBounds` if the current block
    /// height does not fit in an i64, and with `CompareFailed` if the subtraction overflows.
    CompareHeight,

    // Stack Manipulation
    /// No op. Does nothing. Never fails.
    Nop,
    /// Pushes a zero onto the stack. This is a very common opcode and has the same effect as PushInt(0) but is more
    /// compact. Fails with `StackOverflow` if the stack would exceed the max stack height.
    PushZero,
    /// Pushes a one onto the stack. This is a very common opcode and has the same effect as PushInt(1) but is more
    /// compact. Fails with `StackOverflow` if the stack would exceed the max stack height.
    PushOne,
    /// Pushes the associated 32-byte value onto the stack. Fails with `IncompatibleTypes` if HashValue is not a valid
    /// 32 byte sequence. Fails with `StackOverflow` if the stack would exceed the max stack height.
    PushHash(Box<HashValue>),
    /// Pushes the associated 64-bit signed integer onto the stack Fails with `IncompatibleTypes` if i64 is not a valid
    /// 64-bit integer. Fails with `StackOverflow` if the stack would exceed the max stack height.
    PushInt(i64),
    /// Pushes the associated 32-byte value onto the stack. It will be interpreted as a public key or a commitment.
    /// Fails with `IncompatibleTypes` if RistrettoPublicKey is not a valid 32 byte sequence. Fails with
    /// `StackOverflow` if the stack would exceed the max stack height.
    PushPubKey(Box<CompressedKey<RistrettoPublicKey>>),
    /// Drops the top stack item. Fails with `StackUnderflow` if the stack is empty.
    Drop,
    /// Duplicates the top stack item. Fails with `StackUnderflow` if the stack is empty. Fails with `StackOverflow` if
    /// the stack would exceed the max stack height.
    Dup,
    /// Reverse rotation. The top stack item moves into 3rd place (counting from the top). Written bottom-first with
    /// `c` on top: `[a, b, c] => [c, a, b]`. Fails with `StackUnderflow` if the stack has fewer than three items.
    RevRot,

    // Math Operations
    /// Pops the top stack element as `val`. If `val` is greater than or equal to zero, push a 1 to the stack,
    /// otherwise push 0. Fails with `StackUnderflow` if the stack is empty. Fails with `InvalidInput` if `val` is
    /// not an integer.
    GeZero,
    /// Pops the top stack element as `val`. If `val` is strictly greater than zero, push a 1 to the stack, otherwise
    /// push 0. Fails with `StackUnderflow` if the stack is empty. Fails with `InvalidInput` if the item is not an
    /// integer.
    GtZero,
    /// Pops the top stack element as `val`. If `val` is less than or equal to zero, push a 1 to the stack, otherwise
    /// push 0. Fails with `StackUnderflow` if the stack is empty. Fails with `InvalidInput` if the item is not an
    /// integer.
    LeZero,
    /// Pops the top stack element as `val`. If `val` is strictly less than zero, push a 1 to the stack, otherwise push
    /// 0. Fails with `StackUnderflow` if the stack is empty. Fails with `InvalidInput` if the items is not an
    /// integer.
    LtZero,
    /// Pops two items from the stack and pushes their sum to the stack. Fails with `StackUnderflow` if the stack has
    /// fewer than two items. Fails with `InvalidInput` if the items cannot be added to each other (e.g. an integer and
    /// public key).
    Add,
    /// Pops two items from the stack and pushes the second minus the top to the stack. Fails with `StackUnderflow` if
    /// the stack has fewer than two items. Fails with `InvalidInput` if the items cannot be subtracted from each other
    /// (e.g. an integer and public key).
    Sub,
    /// Pops the top two items from the stack, and pushes 1 to the stack if the inputs are exactly equal, 0 otherwise.
    /// Only two items of the same type can be compared: `Number`, `Hash`, `PublicKey`, `Commitment` or `Signature`.
    /// Fails with `StackUnderflow` if the stack has fewer than two items. Fails with `IncompatibleTypes` (aborting
    /// the script, rather than pushing 0) if the two items are of different types (e.g. an integer and a public
    /// key), or if either item is a `Scalar` (scalars are not comparable, not even with another scalar).
    Equal,
    /// Pops the top two items from the stack, and compares their values using the same rules as `Equal`. Fails with
    /// `StackUnderflow` if the stack has fewer than two items. Fails with `IncompatibleTypes` if the items cannot be
    /// compared (see `Equal`). Fails with `VerifyFailed` if the top two stack elements are not equal.
    EqualVerify,

    // Boolean Logic
    /// Pops `n` + 1 items from the stack (with u8 as `n`). If the last item matches at least one of the first `n`
    /// items, push 1 onto the stack, otherwise push 0 onto the stack. Fails with `StackUnderflow` if the stack has
    /// fewer than `n` + 1 items. Fails with `InvalidInput` if the `n` + 1 items are not all of the same type.
    Or(u8),
    /// Pops `n` + 1 items from the stack (with u8 as `n`). If the last item matches at least one of the first n items,
    /// continue. Fails with `StackUnderflow` if the stack has fewer than `n` + 1 items. Fails with `InvalidInput` if
    /// the `n` + 1 items are not all of the same type. Fails with `VerifyFailed` if the last item does not match at
    /// least one of the first `n` items.
    OrVerify(u8),

    // Cryptographic Operations
    //
    // The hash opcodes below hash the raw, untagged 32-byte payload of the popped item: no type tag or domain
    // separator is included. A `Hash`, a `PublicKey` and a `Commitment` holding the same 32 bytes therefore produce
    // the same digest. `Number`, `Scalar` and `Signature` items cannot be hashed.
    /// Pops the top element (a `Hash`, `PublicKey` or `Commitment`), hashes its raw 32 bytes with the Blake2b<U32>
    /// hash function and pushes the result to the stack as a `Hash`. Fails with `StackUnderflow` if the stack is
    /// empty. Fails with `IncompatibleTypes` if the item is a `Number`, `Scalar` or `Signature`.
    HashBlake256,
    /// Pops the top element (a `Hash`, `PublicKey` or `Commitment`), hashes its raw 32 bytes with the SHA256 hash
    /// function and pushes the result to the stack as a `Hash`. Fails with `StackUnderflow` if the stack is empty.
    /// Fails with `IncompatibleTypes` if the item is a `Number`, `Scalar` or `Signature`.
    HashSha256,
    /// Pops the top element (a `Hash`, `PublicKey` or `Commitment`), hashes its raw 32 bytes with the SHA-3 hash
    /// function and pushes the result to the stack as a `Hash`. Fails with `StackUnderflow` if the stack is empty.
    /// Fails with `IncompatibleTypes` if the item is a `Number`, `Scalar` or `Signature`.
    HashSha3,
    /// Pops the public key and then the signature from the stack. If signature validation using the 32-byte message
    /// and public key succeeds, push 1 to the stack, otherwise push 0. Fails with `StackUnderflow` if the stack has
    /// fewer than 2 items. Fails with `IncompatibleTypes` if the top stack element is not a `PublicKey` or the second
    /// stack element is not a `Signature`. Fails with `InvalidInput` if the public key cannot be decompressed.
    ///
    /// # Security
    ///
    /// The message is a constant embedded in the script; it is not derived from the spending transaction. A valid
    /// signature is therefore replayable: it satisfies this opcode in any script, on any output, that uses the same
    /// (public key, message) pair. The only binding between a script execution and a particular spend is the
    /// input's script signature, which is checked outside the script engine. To avoid cross-output replay, use a
    /// message that is unique to the output, such as the output commitment (as the pre-mine scripts do).
    CheckSig(Box<Message>),
    /// Identical to CheckSig, except that nothing is pushed to the stack if the signature is valid, and the operation
    /// fails with `VerifyFailed` if the signature is invalid. The replay caveat on `CheckSig` applies.
    CheckSigVerify(Box<Message>),
    /// Pops exactly `m` signatures from the stack. The multiple signature validation will not succeed if the `m`
    /// signatures are not unique or if Vec<RistrettoPublicKey> contains a duplicate public key. Each signature is
    /// validated using the 32-byte message and a public key that match. If signature validation for m unique
    /// signatures succeeds, push 1 to the stack, otherwise push 0.
    /// Fails with `IncompatibleTypes` if either `m` (the 1st u8) or `n` (the 2nd u8) is not a valid 8-bit unsigned
    /// integer, if Vec<RistrettoPublicKey> contains an invalid public key or if Message is not a valid 32-byte
    /// sequence.
    /// Fails with `ValueExceedsBounds` if `m` == 0 or if `n` == 0 or if `m` > `n` or if `n` > `MAX_MULTISIG_LIMIT`
    /// (32) or if the number of public keys provided != `n`.
    /// Fails with `StackUnderflow` if the stack has fewer than m items.
    /// Fails with `IncompatibleTypes` if any of the m signatures from the stack is not a valid signature.
    /// Fails with `InvalidInput` if each of the top m elements is not a Signature.
    ///
    /// [TariScript::new](crate::TariScript::new) rejects a `CheckMultiSig*` opcode whose `n` differs from the number
    /// of public keys, since such an opcode cannot be serialised faithfully.
    ///
    /// # Security
    ///
    /// As with `CheckSig`, the message is a script constant, so the signatures are replayable across every output
    /// that shares the same (public keys, message). Binding to a particular spend comes only from the input's script
    /// signature. Prefer a message unique to the output, such as the output commitment.
    CheckMultiSig(u8, u8, Vec<CompressedKey<RistrettoPublicKey>>, Box<Message>),
    /// Identical to CheckMultiSig, except that nothing is pushed to the stack if the multiple signature validation is
    /// either valid or invalid. Fails with `VerifyFailed` if any signature is invalid.
    CheckMultiSigVerify(u8, u8, Vec<CompressedKey<RistrettoPublicKey>>, Box<Message>),
    /// Identical to CheckMultiSig, except that the aggregate of the public keys is pushed to the stack if multiple
    /// signature validation succeeds. Fails with `VerifyFailed` if any signature is invalid.
    CheckMultiSigVerifyAggregatePubKey(u8, u8, Vec<CompressedKey<RistrettoPublicKey>>, Box<Message>),
    /// Pops the top element from the stack (either a scalar or a hash), parses it canonically as a Ristretto secret
    /// key if possible, computes the corresponding Ristretto public key, and pushes this value to the stack.
    /// Fails with `StackUnderflow` if the stack is empty.
    /// Fails with `IncompatibleTypes` if the stack item is not either a scalar or a hash.
    /// Fails with `InvalidInput` if the stack item cannot be canonically parsed as a Ristretto secret key.
    ToRistrettoPoint,

    // Miscellaneous
    /// Always fails with `Return`.
    Return,
    /// Pops the top element of the stack into `pred`. If `pred` is 1, the instructions between `IfThen` and `Else` are
    /// executed. If `pred` is 0, instructions are popped until `Else` or `EndIf` is encountered. If `Else` is
    /// encountered, instructions are executed until `EndIf` is reached. `EndIf` is a marker opcode and a no-op.
    /// Fails with `StackUnderflow` if the stack is empty.
    /// Fails with `InvalidInput` if pred is anything other than 0 or 1.
    /// Fails with the corresponding failure code if any instruction during execution of the clause causes a failure.
    IfThen,
    /// Marks the beginning of the `Else` branch.
    Else,
    /// Marks the end of the `IfThen` statement.
    EndIf,
}

impl Opcode {
    pub fn get_version(&self) -> OpcodeVersion {
        match self {
            Opcode::CheckHeightVerify(..) |
            Opcode::CheckHeight(..) |
            Opcode::CompareHeightVerify |
            Opcode::CompareHeight |
            Opcode::Nop |
            Opcode::PushZero |
            Opcode::PushOne |
            Opcode::PushHash(..) |
            Opcode::PushInt(..) |
            Opcode::PushPubKey(..) |
            Opcode::Drop |
            Opcode::Dup |
            Opcode::RevRot |
            Opcode::GeZero |
            Opcode::GtZero |
            Opcode::LeZero |
            Opcode::LtZero |
            Opcode::Add |
            Opcode::Sub |
            Opcode::Equal |
            Opcode::EqualVerify |
            Opcode::Or(..) |
            Opcode::OrVerify(..) |
            Opcode::HashBlake256 |
            Opcode::HashSha256 |
            Opcode::HashSha3 |
            Opcode::CheckSig(..) |
            Opcode::CheckSigVerify(..) |
            Opcode::CheckMultiSig(..) |
            Opcode::CheckMultiSigVerify(..) |
            Opcode::CheckMultiSigVerifyAggregatePubKey(..) |
            Opcode::ToRistrettoPoint |
            Opcode::Return |
            Opcode::IfThen |
            Opcode::Else |
            Opcode::EndIf => OpcodeVersion::V0,
        }
    }

    /// Parse a byte slice into a list of opcodes. Parsing stops with an error as soon as more than `max_opcodes`
    /// opcodes would be read, so that an oversized script is rejected without materialising all of its opcodes.
    pub fn parse(bytes: &[u8], max_opcodes: usize) -> Result<Vec<Opcode>, ScriptError> {
        let mut script = Vec::new();
        let mut bytes_copy = bytes;

        while !bytes_copy.is_empty() {
            if script.len() >= max_opcodes {
                return Err(MaxSizeVecError::MaxSizeVecLengthError {
                    expected: max_opcodes,
                    actual: script.len().saturating_add(1),
                }
                .into());
            }
            let (opcode, bytes_left) = Opcode::read_next(bytes_copy)?;
            script.push(opcode);
            bytes_copy = bytes_left;
        }

        Ok(script)
    }

    /// Take a byte slice and read the next opcode from it, including any associated data. `read_next` returns a tuple
    /// of the deserialised opcode, and an updated slice that has the Opcode and data removed.
    fn read_next(bytes: &[u8]) -> Result<(Opcode, &[u8]), ScriptError> {
        let code = bytes.first().ok_or(ScriptError::InvalidOpcode)?;
        #[allow(clippy::enum_glob_use)]
        use Opcode::*;
        let scrubbed_bytes = bytes.get(1..).ok_or(ScriptError::InvalidData)?;
        match *code {
            OP_CHECK_HEIGHT_VERIFY => {
                let (height, size) = u64::decode_var(scrubbed_bytes).ok_or(ScriptError::InvalidData)?;
                Ok((CheckHeightVerify(height), bytes_after_varint(bytes, size)?))
            },
            OP_CHECK_HEIGHT => {
                let (height, size) = u64::decode_var(scrubbed_bytes).ok_or(ScriptError::InvalidData)?;
                Ok((CheckHeight(height), bytes_after_varint(bytes, size)?))
            },
            OP_COMPARE_HEIGHT_VERIFY => Ok((CompareHeightVerify, scrubbed_bytes)),
            OP_COMPARE_HEIGHT => Ok((CompareHeight, scrubbed_bytes)),
            OP_NOP => Ok((Nop, scrubbed_bytes)),
            OP_PUSH_ZERO => Ok((PushZero, scrubbed_bytes)),
            OP_PUSH_ONE => Ok((PushOne, scrubbed_bytes)),
            OP_PUSH_HASH => {
                let hash = slice_to_boxed_hash(bytes.get(1..33).ok_or(ScriptError::InvalidData)?)?;
                Ok((PushHash(hash), bytes.get(33..).ok_or(ScriptError::InvalidData)?))
            },
            OP_PUSH_INT => {
                let (n, size) = i64::decode_var(scrubbed_bytes).ok_or(ScriptError::InvalidData)?;
                Ok((PushInt(n), bytes_after_varint(bytes, size)?))
            },
            OP_PUSH_PUBKEY => {
                let p = CompressedKey::from_canonical_bytes(bytes.get(1..33).ok_or(ScriptError::InvalidData)?)?;
                Ok((
                    PushPubKey(Box::new(p)),
                    bytes.get(33..).ok_or(ScriptError::InvalidData)?,
                ))
            },
            OP_DROP => Ok((Drop, scrubbed_bytes)),
            OP_DUP => Ok((Dup, scrubbed_bytes)),
            OP_REV_ROT => Ok((RevRot, scrubbed_bytes)),
            OP_GE_ZERO => Ok((GeZero, scrubbed_bytes)),
            OP_GT_ZERO => Ok((GtZero, scrubbed_bytes)),
            OP_LE_ZERO => Ok((LeZero, scrubbed_bytes)),
            OP_LT_ZERO => Ok((LtZero, scrubbed_bytes)),
            OP_ADD => Ok((Add, scrubbed_bytes)),
            OP_SUB => Ok((Sub, scrubbed_bytes)),
            OP_EQUAL => Ok((Equal, scrubbed_bytes)),
            OP_EQUAL_VERIFY => Ok((EqualVerify, scrubbed_bytes)),
            OP_OR => {
                let n = bytes.get(1).ok_or(ScriptError::InvalidData)?;
                Ok((Or(*n), bytes.get(2..).ok_or(ScriptError::InvalidData)?))
            },
            OP_OR_VERIFY => {
                let n = bytes.get(1).ok_or(ScriptError::InvalidData)?;
                Ok((OrVerify(*n), bytes.get(2..).ok_or(ScriptError::InvalidData)?))
            },
            OP_HASH_BLAKE256 => Ok((HashBlake256, scrubbed_bytes)),
            OP_HASH_SHA256 => Ok((HashSha256, scrubbed_bytes)),
            OP_HASH_SHA3 => Ok((HashSha3, scrubbed_bytes)),
            OP_CHECK_SIG => {
                let msg = slice_to_boxed_message(bytes.get(1..33).ok_or(ScriptError::InvalidData)?)?;
                Ok((CheckSig(msg), bytes.get(33..).ok_or(ScriptError::InvalidData)?))
            },
            OP_CHECK_SIG_VERIFY => {
                let msg = slice_to_boxed_message(bytes.get(1..33).ok_or(ScriptError::InvalidData)?)?;
                Ok((CheckSigVerify(msg), bytes.get(33..).ok_or(ScriptError::InvalidData)?))
            },
            OP_CHECK_MULTI_SIG => {
                let (m, n, keys, msg, end) = Opcode::read_multisig_args(bytes)?;
                Ok((
                    CheckMultiSig(m, n, keys, msg),
                    bytes.get(end..).ok_or(ScriptError::InvalidData)?,
                ))
            },
            OP_CHECK_MULTI_SIG_VERIFY => {
                let (m, n, keys, msg, end) = Opcode::read_multisig_args(bytes)?;
                Ok((
                    CheckMultiSigVerify(m, n, keys, msg),
                    bytes.get(end..).ok_or(ScriptError::InvalidData)?,
                ))
            },
            OP_CHECK_MULTI_SIG_VERIFY_AGGREGATE_PUB_KEY => {
                let (m, n, keys, msg, end) = Opcode::read_multisig_args(bytes)?;
                Ok((
                    CheckMultiSigVerifyAggregatePubKey(m, n, keys, msg),
                    bytes.get(end..).ok_or(ScriptError::InvalidData)?,
                ))
            },
            OP_TO_RISTRETTO_POINT => Ok((ToRistrettoPoint, scrubbed_bytes)),
            OP_RETURN => Ok((Return, scrubbed_bytes)),
            OP_IF_THEN => Ok((IfThen, scrubbed_bytes)),
            OP_ELSE => Ok((Else, scrubbed_bytes)),
            OP_END_IF => Ok((EndIf, scrubbed_bytes)),
            _ => Err(ScriptError::InvalidOpcode),
        }
    }

    fn read_multisig_args(bytes: &[u8]) -> Result<MultiSigArgs, ScriptError> {
        if bytes.len() < 3 {
            return Err(ScriptError::InvalidData);
        }
        let m = bytes.get(1).ok_or(ScriptError::InvalidData)?;
        let n = bytes.get(2).ok_or(ScriptError::InvalidData)?;
        let num = *n as usize;
        let len = num
            .checked_mul(PUBLIC_KEY_LENGTH)
            .and_then(|v| v.checked_add(3))
            .ok_or(ScriptError::InvalidData)?;
        let end = len.checked_add(MESSAGE_LENGTH).ok_or(ScriptError::InvalidData)?;
        let keys = slice_to_vec_pubkeys(bytes.get(3..len).ok_or(ScriptError::InvalidData)?, num)?;
        let msg = slice_to_boxed_message(bytes.get(len..end).ok_or(ScriptError::InvalidData)?)?;

        Ok((*m, *n, keys, msg, end))
    }

    /// Convert an opcode into its binary representation and append it to the array. The function returns the byte slice
    /// that matches the opcode as a convenience
    pub fn to_bytes<'a>(&self, array: &'a mut Vec<u8>) -> &'a [u8] {
        let n = array.len();
        #[allow(clippy::enum_glob_use)]
        use Opcode::*;
        match self {
            CheckHeightVerify(height) => {
                array.push(OP_CHECK_HEIGHT_VERIFY);
                let mut buf = [0u8; 10];
                let used = height.encode_var(&mut buf[..]);
                array.extend_from_slice(buf.get(0..used).expect("Length is always valid"));
            },
            CheckHeight(height) => {
                array.push(OP_CHECK_HEIGHT);
                let mut buf = [0u8; 10];
                let used = height.encode_var(&mut buf[..]);
                array.extend_from_slice(buf.get(0..used).expect("Length is always valid"));
            },
            CompareHeightVerify => array.push(OP_COMPARE_HEIGHT_VERIFY),
            CompareHeight => array.push(OP_COMPARE_HEIGHT),
            Nop => array.push(OP_NOP),
            PushZero => array.push(OP_PUSH_ZERO),
            PushOne => array.push(OP_PUSH_ONE),
            PushHash(h) => {
                array.push(OP_PUSH_HASH);
                array.extend_from_slice(h.deref());
            },
            PushInt(n) => {
                array.push(OP_PUSH_INT);
                let mut buf = [0u8; 10];
                let used = n.encode_var(&mut buf[..]);
                array.extend_from_slice(buf.get(0..used).expect("Length is always valid"));
            },
            PushPubKey(p) => {
                array.push(OP_PUSH_PUBKEY);
                array.extend_from_slice(p.deref().as_bytes());
            },
            Drop => array.push(OP_DROP),
            Dup => array.push(OP_DUP),
            RevRot => array.push(OP_REV_ROT),
            GeZero => array.push(OP_GE_ZERO),
            GtZero => array.push(OP_GT_ZERO),
            LeZero => array.push(OP_LE_ZERO),
            LtZero => array.push(OP_LT_ZERO),
            Add => array.push(OP_ADD),
            Sub => array.push(OP_SUB),
            Equal => array.push(OP_EQUAL),
            EqualVerify => array.push(OP_EQUAL_VERIFY),
            Or(n) => {
                array.push(OP_OR);
                array.push(*n);
            },
            OrVerify(n) => {
                array.push(OP_OR_VERIFY);
                array.push(*n);
            },
            HashBlake256 => array.push(OP_HASH_BLAKE256),
            HashSha256 => array.push(OP_HASH_SHA256),
            HashSha3 => array.push(OP_HASH_SHA3),
            CheckSig(msg) => {
                array.push(OP_CHECK_SIG);
                array.extend_from_slice(msg.deref());
            },
            CheckSigVerify(msg) => {
                array.push(OP_CHECK_SIG_VERIFY);
                array.extend_from_slice(msg.deref());
            },
            CheckMultiSig(m, n, public_keys, msg) => {
                array.extend_from_slice(&[OP_CHECK_MULTI_SIG, *m, *n]);
                for public_key in public_keys {
                    array.extend(public_key.as_bytes());
                }
                array.extend_from_slice(msg.deref());
            },
            CheckMultiSigVerify(m, n, public_keys, msg) => {
                array.extend_from_slice(&[OP_CHECK_MULTI_SIG_VERIFY, *m, *n]);
                for public_key in public_keys {
                    array.extend(public_key.as_bytes());
                }
                array.extend_from_slice(msg.deref());
            },
            CheckMultiSigVerifyAggregatePubKey(m, n, public_keys, msg) => {
                array.extend_from_slice(&[OP_CHECK_MULTI_SIG_VERIFY_AGGREGATE_PUB_KEY, *m, *n]);
                for public_key in public_keys {
                    array.extend(public_key.as_bytes());
                }
                array.extend_from_slice(msg.deref());
            },
            ToRistrettoPoint => array.push(OP_TO_RISTRETTO_POINT),
            Return => array.push(OP_RETURN),
            IfThen => array.push(OP_IF_THEN),
            Else => array.push(OP_ELSE),
            EndIf => array.push(OP_END_IF),
        };

        array.get(n..).expect("Length is always valid")
    }
}

impl fmt::Display for Opcode {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        #[allow(clippy::enum_glob_use)]
        use Opcode::*;
        match self {
            CheckHeightVerify(height) => write!(fmt, "CheckHeightVerify({})", *height),
            CheckHeight(height) => write!(fmt, "CheckHeight({})", *height),
            CompareHeightVerify => write!(fmt, "CompareHeightVerify"),
            CompareHeight => write!(fmt, "CompareHeight"),
            Nop => write!(fmt, "Nop"),
            PushZero => write!(fmt, "PushZero"),
            PushOne => write!(fmt, "PushOne"),
            PushHash(h) => write!(fmt, "PushHash({})", (*h).to_hex()),
            PushInt(n) => write!(fmt, "PushInt({})", *n),
            PushPubKey(h) => write!(fmt, "PushPubKey({})", (*h).to_hex()),
            Drop => write!(fmt, "Drop"),
            Dup => write!(fmt, "Dup"),
            RevRot => write!(fmt, "RevRot"),
            GeZero => write!(fmt, "GeZero"),
            GtZero => write!(fmt, "GtZero"),
            LeZero => write!(fmt, "LeZero"),
            LtZero => write!(fmt, "LtZero"),
            Add => write!(fmt, "Add"),
            Sub => write!(fmt, "Sub"),
            Equal => write!(fmt, "Equal"),
            EqualVerify => write!(fmt, "EqualVerify"),
            Or(n) => write!(fmt, "Or({})", *n),
            OrVerify(n) => write!(fmt, "OrVerify({})", *n),
            HashBlake256 => write!(fmt, "HashBlake256"),
            HashSha256 => write!(fmt, "HashSha256"),
            HashSha3 => write!(fmt, "HashSha3"),
            CheckSig(msg) => write!(fmt, "CheckSig({})", (*msg).to_hex()),
            CheckSigVerify(msg) => write!(fmt, "CheckSigVerify({})", (*msg).to_hex()),
            CheckMultiSig(m, n, public_keys, msg) => {
                let keys: Vec<String> = public_keys.iter().map(|p| p.to_hex()).collect();
                write!(
                    fmt,
                    "CheckMultiSig({}, {}, [{}], {})",
                    *m,
                    *n,
                    keys.join(", "),
                    (*msg).to_hex()
                )
            },
            CheckMultiSigVerify(m, n, public_keys, msg) => {
                let keys: Vec<String> = public_keys.iter().map(|p| p.to_hex()).collect();
                write!(
                    fmt,
                    "CheckMultiSigVerify({}, {}, [{}], {})",
                    *m,
                    *n,
                    keys.join(", "),
                    (*msg).to_hex()
                )
            },
            CheckMultiSigVerifyAggregatePubKey(m, n, public_keys, msg) => {
                let keys: Vec<String> = public_keys.iter().map(|p| p.to_hex()).collect();
                write!(
                    fmt,
                    "CheckMultiSigVerifyAggregatePubKey({}, {}, [{}], {})",
                    *m,
                    *n,
                    keys.join(", "),
                    (*msg).to_hex()
                )
            },
            ToRistrettoPoint => write!(fmt, "ToRistrettoPoint"),
            Return => write!(fmt, "Return"),
            IfThen => write!(fmt, "IfThen"),
            Else => write!(fmt, "Else"),
            EndIf => write!(fmt, "EndIf"),
        }
    }
}

/// The script opcode version, used by consensus to restrict which opcodes may appear in output scripts.
///
/// Note on how this gate is (and is not) applied:
/// * The parser ([Opcode::parse] / `Opcode::read_next`) rejects any byte it does not recognise with `InvalidOpcode`
///   *before* the consensus opcode-version range is ever consulted. An opcode that the parser does not know about can
///   therefore never be admitted by widening the version range alone.
/// * The consensus version range is only applied to output scripts, not to input scripts.
///
/// Adding a new opcode therefore requires changing `read_next` first, and then applying the version range to input
/// scripts as well, so that a node cannot be made to execute an opcode that is newer than the consensus rules allow.
#[derive(Debug, Clone, PartialEq, PartialOrd, Serialize, Deserialize)]
#[repr(u8)]
pub enum OpcodeVersion {
    V0 = 0,
}

#[cfg(test)]
mod test {
    #![allow(clippy::indexing_slicing)]
    use crate::{op_codes::*, script::MAX_SCRIPT_OPCODES};

    #[test]
    fn empty_script() {
        assert_eq!(Opcode::parse(&[], MAX_SCRIPT_OPCODES).unwrap(), Vec::new())
    }

    #[test]
    fn parse() {
        let script = [0xFF, 0x71, 0x00];
        let err = Opcode::parse(&script, MAX_SCRIPT_OPCODES).unwrap_err();
        assert!(matches!(err, ScriptError::InvalidOpcode));

        let script = [0x60u8, 0x71];
        let opcodes = Opcode::parse(&script, MAX_SCRIPT_OPCODES).unwrap();
        let code = opcodes.first().unwrap();
        assert_eq!(code, &Opcode::Return);
        let code = opcodes.get(1).unwrap();
        assert_eq!(code, &Opcode::Dup);

        let err = Opcode::parse(&[0x7a], MAX_SCRIPT_OPCODES).unwrap_err();
        assert!(matches!(err, ScriptError::InvalidData));
    }

    #[test]
    fn parse_stops_at_max_opcodes() {
        // Exactly the maximum number of opcodes parses
        let script = vec![OP_NOP; MAX_SCRIPT_OPCODES];
        assert_eq!(
            Opcode::parse(&script, MAX_SCRIPT_OPCODES).unwrap().len(),
            MAX_SCRIPT_OPCODES
        );

        // One more is rejected as soon as it would be pushed, before any of the following bytes are read (the trailing
        // invalid opcode would otherwise have produced `InvalidOpcode`)
        let mut script = vec![OP_NOP; MAX_SCRIPT_OPCODES + 1];
        script.push(0xFF);
        let err = Opcode::parse(&script, MAX_SCRIPT_OPCODES).unwrap_err();
        assert_eq!(
            err,
            ScriptError::MaxSizeVecError(MaxSizeVecError::MaxSizeVecLengthError {
                expected: MAX_SCRIPT_OPCODES,
                actual: MAX_SCRIPT_OPCODES + 1,
            })
        );
    }

    #[test]
    fn push_hash() {
        let (code, b) = Opcode::read_next(b"\x7a/thirty-two~character~hash~val./").unwrap();
        assert!(matches!(code, Opcode::PushHash(v) if &*v == b"/thirty-two~character~hash~val./"));
        assert!(b.is_empty());
    }

    #[test]
    fn check_height() {
        fn test_check_height(op: &Opcode, val: u8, display: &str) {
            // Serialize
            assert!(matches!(Opcode::read_next(&[val, 255]), Err(ScriptError::InvalidData)));
            let s = &[val, 63, 1, 2, 3];
            let (opcode, rem) = Opcode::read_next(s).unwrap();
            assert_eq!(opcode, *op);
            assert_eq!(rem, &[1, 2, 3]);
            // Deserialise
            let mut arr = vec![1, 2, 3];
            op.to_bytes(&mut arr);
            assert_eq!(&arr, &[1, 2, 3, val, 63]);
            // Format
            assert_eq!(format!("{op}").as_str(), display);
        }
        test_check_height(&Opcode::CheckHeight(63), 0x67, "CheckHeight(63)");
        test_check_height(&Opcode::CheckHeightVerify(63), 0x66, "CheckHeightVerify(63)");
    }

    #[test]
    fn push_int() {
        // Serialise
        assert!(matches!(Opcode::read_next(&[0x7d, 255]), Err(ScriptError::InvalidData)));
        let s = &[OP_PUSH_INT, 130, 4];
        let (opcode, rem) = Opcode::read_next(s).unwrap();
        let mut arr = vec![];
        Opcode::PushInt(257).to_bytes(&mut arr);
        assert!(matches!(opcode, Opcode::PushInt(257)));
        assert!(rem.is_empty());
        // Deserialise
        let op = Opcode::PushInt(257);
        let mut arr = vec![];
        op.to_bytes(&mut arr);
        assert_eq!(&arr, &[OP_PUSH_INT, 130, 4]);
        // Format
        assert_eq!(format!("{op}").as_str(), "PushInt(257)");
    }

    #[test]
    fn push_pubkey() {
        // Serialise
        assert!(matches!(
            Opcode::read_next(b"\x7eshort_needs_33_bytes"),
            Err(ScriptError::InvalidData)
        ));
        let key = CompressedKey::<RistrettoPublicKey>::from_hex(
            "6c9cb4d3e57351462122310fa22c90b1e6dfb528d64615363d1261a75da3e401",
        )
        .unwrap();
        let s = &[
            OP_PUSH_PUBKEY,
            108,
            156,
            180,
            211,
            229,
            115,
            81,
            70,
            33,
            34,
            49,
            15,
            162,
            44,
            144,
            177,
            230,
            223,
            181,
            40,
            214,
            70,
            21,
            54,
            61,
            18,
            97,
            167,
            93,
            163,
            228,
            1,
        ];
        let op = Opcode::PushPubKey(Box::new(key));
        let (opcode, rem) = Opcode::read_next(s).unwrap();
        assert_eq!(opcode, op);
        assert!(rem.is_empty());
        // Deserialise
        let mut arr = vec![];
        op.to_bytes(&mut arr);
        assert_eq!(&arr, s);
        // Format
        assert_eq!(
            format!("{op}").as_str(),
            "PushPubKey(6c9cb4d3e57351462122310fa22c90b1e6dfb528d64615363d1261a75da3e401)"
        );
    }

    #[test]
    fn or() {
        fn test_or(op: &Opcode, val: u8, display: &str) {
            // Serialise
            assert!(matches!(Opcode::read_next(&[val]), Err(ScriptError::InvalidData)));
            let s = &[val, 5, 83];
            let (opcode, rem) = Opcode::read_next(s).unwrap();
            assert_eq!(opcode, *op);
            assert_eq!(rem, &[83]);
            // Deserialise
            let mut arr = vec![];
            op.to_bytes(&mut arr);
            assert_eq!(&arr, &[val, 5]);
            // Format
            assert_eq!(format!("{op}").as_str(), display);
        }
        test_or(&Opcode::Or(5), OP_OR, "Or(5)");
        test_or(&Opcode::OrVerify(5), OP_OR_VERIFY, "OrVerify(5)");
    }

    #[test]
    fn check_sig() {
        fn test_checksig(op: &Opcode, val: u8, display: &str) {
            // Serialise
            assert!(matches!(Opcode::read_next(&[val]), Err(ScriptError::InvalidData)));
            let msg = &[
                val, 108, 156, 180, 211, 229, 115, 81, 70, 33, 34, 49, 15, 162, 44, 144, 177, 230, 223, 181, 40, 214,
                70, 21, 54, 61, 18, 97, 167, 93, 163, 228, 1,
            ];
            let (opcode, rem) = Opcode::read_next(msg).unwrap();
            assert_eq!(opcode, *op);
            assert!(rem.is_empty());
            // Deserialise
            let mut arr = vec![];
            op.to_bytes(&mut arr);
            assert_eq!(arr, msg);
            // Format
            assert_eq!(format!("{op}").as_str(), display);
        }
        let msg = &[
            108, 156, 180, 211, 229, 115, 81, 70, 33, 34, 49, 15, 162, 44, 144, 177, 230, 223, 181, 40, 214, 70, 21,
            54, 61, 18, 97, 167, 93, 163, 228, 1,
        ];
        test_checksig(
            &Opcode::CheckSig(Box::new(*msg)),
            OP_CHECK_SIG,
            "CheckSig(6c9cb4d3e57351462122310fa22c90b1e6dfb528d64615363d1261a75da3e401)",
        );
        test_checksig(
            &Opcode::CheckSigVerify(Box::new(*msg)),
            OP_CHECK_SIG_VERIFY,
            "CheckSigVerify(6c9cb4d3e57351462122310fa22c90b1e6dfb528d64615363d1261a75da3e401)",
        );
    }

    #[test]
    fn check_multisig() {
        fn test_checkmultisig(op: &Opcode, val: u8, display: &str) {
            // Serialise
            assert!(matches!(Opcode::read_next(&[val]), Err(ScriptError::InvalidData)));
            let bytes = &[
                val, 1, 2, 156, 139, 197, 249, 13, 34, 17, 145, 116, 142, 141, 215, 104, 111, 9, 225, 17, 75, 75, 173,
                164, 195, 103, 237, 88, 174, 25, 156, 81, 235, 16, 11, 86, 233, 240, 24, 177, 56, 186, 132, 53, 33,
                179, 36, 58, 41, 216, 23, 48, 195, 164, 194, 81, 8, 177, 8, 177, 202, 71, 194, 19, 45, 181, 105, 108,
                156, 180, 211, 229, 115, 81, 70, 33, 34, 49, 15, 162, 44, 144, 177, 230, 223, 181, 40, 214, 70, 21, 54,
                61, 18, 97, 167, 93, 163, 228, 1,
            ];
            let (opcode, rem) = Opcode::read_next(bytes).unwrap();
            assert_eq!(opcode, *op);
            assert!(rem.is_empty());
            // Deserialise
            let mut arr = vec![];
            op.to_bytes(&mut arr);
            assert_eq!(arr, bytes);
            // Format
            assert_eq!(format!("{op}").as_str(), display);
        }
        let msg = &[
            108, 156, 180, 211, 229, 115, 81, 70, 33, 34, 49, 15, 162, 44, 144, 177, 230, 223, 181, 40, 214, 70, 21,
            54, 61, 18, 97, 167, 93, 163, 228, 1,
        ];
        let p1 = "9c8bc5f90d221191748e8dd7686f09e1114b4bada4c367ed58ae199c51eb100b";
        let p2 = "56e9f018b138ba843521b3243a29d81730c3a4c25108b108b1ca47c2132db569";
        let keys = vec![
            CompressedKey::<RistrettoPublicKey>::from_hex(p1).unwrap(),
            CompressedKey::<RistrettoPublicKey>::from_hex(p2).unwrap(),
        ];

        test_checkmultisig(
            &Opcode::CheckMultiSig(1, 2, keys.clone(), Box::new(*msg)),
            OP_CHECK_MULTI_SIG,
            "CheckMultiSig(1, 2, [9c8bc5f90d221191748e8dd7686f09e1114b4bada4c367ed58ae199c51eb100b, \
             56e9f018b138ba843521b3243a29d81730c3a4c25108b108b1ca47c2132db569], \
             6c9cb4d3e57351462122310fa22c90b1e6dfb528d64615363d1261a75da3e401)",
        );
        test_checkmultisig(
            &Opcode::CheckMultiSigVerify(1, 2, keys.clone(), Box::new(*msg)),
            OP_CHECK_MULTI_SIG_VERIFY,
            "CheckMultiSigVerify(1, 2, [9c8bc5f90d221191748e8dd7686f09e1114b4bada4c367ed58ae199c51eb100b, \
             56e9f018b138ba843521b3243a29d81730c3a4c25108b108b1ca47c2132db569], \
             6c9cb4d3e57351462122310fa22c90b1e6dfb528d64615363d1261a75da3e401)",
        );
        test_checkmultisig(
            &Opcode::CheckMultiSigVerifyAggregatePubKey(1, 2, keys, Box::new(*msg)),
            OP_CHECK_MULTI_SIG_VERIFY_AGGREGATE_PUB_KEY,
            "CheckMultiSigVerifyAggregatePubKey(1, 2, \
             [9c8bc5f90d221191748e8dd7686f09e1114b4bada4c367ed58ae199c51eb100b, \
             56e9f018b138ba843521b3243a29d81730c3a4c25108b108b1ca47c2132db569], \
             6c9cb4d3e57351462122310fa22c90b1e6dfb528d64615363d1261a75da3e401)",
        );
    }

    #[test]
    fn deserialise_no_param_opcodes() {
        fn test_opcode(code: u8, expected: &Opcode) {
            let s = &[code, 1, 2, 3];
            let (opcode, rem) = Opcode::read_next(s).unwrap();
            assert_eq!(opcode, *expected);
            assert_eq!(rem, &[1, 2, 3]);
        }
        test_opcode(OP_COMPARE_HEIGHT_VERIFY, &Opcode::CompareHeightVerify);
        test_opcode(OP_COMPARE_HEIGHT, &Opcode::CompareHeight);
        test_opcode(OP_NOP, &Opcode::Nop);
        test_opcode(OP_PUSH_ZERO, &Opcode::PushZero);
        test_opcode(OP_PUSH_ONE, &Opcode::PushOne);
        test_opcode(OP_DROP, &Opcode::Drop);
        test_opcode(OP_DUP, &Opcode::Dup);
        test_opcode(OP_REV_ROT, &Opcode::RevRot);
        test_opcode(OP_GE_ZERO, &Opcode::GeZero);
        test_opcode(OP_GT_ZERO, &Opcode::GtZero);
        test_opcode(OP_LE_ZERO, &Opcode::LeZero);
        test_opcode(OP_LT_ZERO, &Opcode::LtZero);
        test_opcode(OP_EQUAL, &Opcode::Equal);
        test_opcode(OP_EQUAL_VERIFY, &Opcode::EqualVerify);
        test_opcode(OP_HASH_SHA3, &Opcode::HashSha3);
        test_opcode(OP_HASH_BLAKE256, &Opcode::HashBlake256);
        test_opcode(OP_HASH_SHA256, &Opcode::HashSha256);
        test_opcode(OP_TO_RISTRETTO_POINT, &Opcode::ToRistrettoPoint);
        test_opcode(OP_IF_THEN, &Opcode::IfThen);
        test_opcode(OP_ELSE, &Opcode::Else);
        test_opcode(OP_END_IF, &Opcode::EndIf);
        test_opcode(OP_ADD, &Opcode::Add);
        test_opcode(OP_SUB, &Opcode::Sub);
        test_opcode(OP_RETURN, &Opcode::Return);
    }

    #[test]
    fn serialise_no_param_opcodes() {
        fn test_opcode(val: u8, opcode: &Opcode) {
            let mut arr = vec![];
            assert_eq!(opcode.to_bytes(&mut arr), &[val]);
        }
        test_opcode(OP_COMPARE_HEIGHT_VERIFY, &Opcode::CompareHeightVerify);
        test_opcode(OP_COMPARE_HEIGHT, &Opcode::CompareHeight);
        test_opcode(OP_NOP, &Opcode::Nop);
        test_opcode(OP_PUSH_ZERO, &Opcode::PushZero);
        test_opcode(OP_PUSH_ONE, &Opcode::PushOne);
        test_opcode(OP_DROP, &Opcode::Drop);
        test_opcode(OP_DUP, &Opcode::Dup);
        test_opcode(OP_REV_ROT, &Opcode::RevRot);
        test_opcode(OP_GE_ZERO, &Opcode::GeZero);
        test_opcode(OP_GT_ZERO, &Opcode::GtZero);
        test_opcode(OP_LE_ZERO, &Opcode::LeZero);
        test_opcode(OP_LT_ZERO, &Opcode::LtZero);
        test_opcode(OP_EQUAL, &Opcode::Equal);
        test_opcode(OP_EQUAL_VERIFY, &Opcode::EqualVerify);
        test_opcode(OP_HASH_SHA3, &Opcode::HashSha3);
        test_opcode(OP_HASH_BLAKE256, &Opcode::HashBlake256);
        test_opcode(OP_HASH_SHA256, &Opcode::HashSha256);
        test_opcode(OP_TO_RISTRETTO_POINT, &Opcode::ToRistrettoPoint);
        test_opcode(OP_IF_THEN, &Opcode::IfThen);
        test_opcode(OP_ELSE, &Opcode::Else);
        test_opcode(OP_END_IF, &Opcode::EndIf);
        test_opcode(OP_ADD, &Opcode::Add);
        test_opcode(OP_SUB, &Opcode::Sub);
        test_opcode(OP_RETURN, &Opcode::Return);
    }

    #[test]
    fn display() {
        fn test_opcode(opcode: &Opcode, expected: &str) {
            let s = format!("{opcode}");
            assert_eq!(s.as_str(), expected);
        }
        test_opcode(&Opcode::CompareHeightVerify, "CompareHeightVerify");
        test_opcode(&Opcode::CompareHeight, "CompareHeight");
        test_opcode(&Opcode::Nop, "Nop");
        test_opcode(&Opcode::PushZero, "PushZero");
        test_opcode(&Opcode::PushOne, "PushOne");
        test_opcode(&Opcode::Drop, "Drop");
        test_opcode(&Opcode::Dup, "Dup");
        test_opcode(&Opcode::RevRot, "RevRot");
        test_opcode(&Opcode::GeZero, "GeZero");
        test_opcode(&Opcode::GtZero, "GtZero");
        test_opcode(&Opcode::LeZero, "LeZero");
        test_opcode(&Opcode::LtZero, "LtZero");
        test_opcode(&Opcode::Equal, "Equal");
        test_opcode(&Opcode::EqualVerify, "EqualVerify");
        test_opcode(&Opcode::HashSha3, "HashSha3");
        test_opcode(&Opcode::HashBlake256, "HashBlake256");
        test_opcode(&Opcode::HashSha256, "HashSha256");
        test_opcode(&Opcode::ToRistrettoPoint, "ToRistrettoPoint");
        test_opcode(&Opcode::IfThen, "IfThen");
        test_opcode(&Opcode::Else, "Else");
        test_opcode(&Opcode::EndIf, "EndIf");
        test_opcode(&Opcode::Add, "Add");
        test_opcode(&Opcode::Sub, "Sub");
        test_opcode(&Opcode::Return, "Return");
    }

    #[test]
    fn test_slice_to_vec_pubkeys() {
        let key = CompressedKey::<RistrettoPublicKey>::from_hex(
            "6c9cb4d3e57351462122310fa22c90b1e6dfb528d64615363d1261a75da3e401",
        )
        .unwrap();
        let bytes = key.as_bytes();
        let vec = [bytes, bytes, bytes].concat();
        let slice = vec.as_bytes();
        let vec = slice_to_vec_pubkeys(slice, 3).unwrap();
        for pk in vec {
            assert_eq!(key, pk);
        }
    }

    #[test]
    fn test_read_multisig_args() {
        let key = CompressedKey::<RistrettoPublicKey>::from_hex(
            "6c9cb4d3e57351462122310fa22c90b1e6dfb528d64615363d1261a75da3e401",
        )
        .unwrap();
        let bytes = key.as_bytes();
        let message = &[
            108, 156, 180, 211, 229, 115, 81, 70, 33, 34, 49, 15, 162, 44, 144, 177, 230, 223, 181, 40, 214, 70, 21,
            54, 61, 18, 97, 167, 93, 163, 228, 1,
        ];
        let vec = [&[OP_CHECK_MULTI_SIG, 1, 2], bytes, bytes, message].concat();
        let slice = vec.as_bytes();
        let (m, n, keys, msg, end) = Opcode::read_multisig_args(slice).unwrap();
        assert_eq!(m, 1);
        assert_eq!(n, 2);
        assert_eq!(*msg, *message);
        assert_eq!(end, vec.len());
        for p in keys {
            assert_eq!(key, p);
        }
    }

    #[test]
    fn slice_helpers_reject_wrong_lengths() {
        let bytes = [7u8; 33];
        assert_eq!(slice_to_hash(&bytes[..32]), Ok([7u8; 32]));
        assert_eq!(slice_to_boxed_hash(&bytes[..32]), Ok(Box::new([7u8; 32])));
        assert_eq!(slice_to_message(&bytes[..32]), Ok([7u8; 32]));
        assert_eq!(slice_to_boxed_message(&bytes[..32]), Ok(Box::new([7u8; 32])));
        for len in [0, 1, 31, 33] {
            assert_eq!(slice_to_hash(&bytes[..len]), Err(ScriptError::InvalidData));
            assert_eq!(slice_to_boxed_hash(&bytes[..len]), Err(ScriptError::InvalidData));
            assert_eq!(slice_to_message(&bytes[..len]), Err(ScriptError::InvalidData));
            assert_eq!(slice_to_boxed_message(&bytes[..len]), Err(ScriptError::InvalidData));
        }
    }

    /// Pins the current (lenient) varint decoding of `integer-encoding` 3.0.4: a non-minimal encoding of zero is
    /// accepted, and re-serialised minimally. If a dependency bump makes the decoder strict, this test must fail,
    /// because it would change which scripts are valid (a consensus change).
    #[test]
    fn non_minimal_varint_is_accepted_pinned() {
        let opcodes = Opcode::parse(&[0x67, 0x80, 0x00], MAX_SCRIPT_OPCODES).unwrap();
        assert_eq!(opcodes, vec![Opcode::CheckHeight(0)]);
        let mut bytes = Vec::new();
        opcodes[0].to_bytes(&mut bytes);
        assert_eq!(bytes, vec![0x67, 0x00]);

        // The same holds for the other varint-carrying opcodes
        assert_eq!(
            Opcode::parse(&[0x66, 0x81, 0x80, 0x00], MAX_SCRIPT_OPCODES).unwrap(),
            vec![Opcode::CheckHeightVerify(1)]
        );
        // PushInt is zig-zag encoded: 0x02 is 1
        assert_eq!(Opcode::parse(&[0x7d, 0x82, 0x00], MAX_SCRIPT_OPCODES).unwrap(), vec![
            Opcode::PushInt(1)
        ]);
    }
}
