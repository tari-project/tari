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
use tari_crypto::{ristretto::RistrettoPublicKey, tari_utilities::ByteArray};
use tari_utilities::{hex::Hex, ByteArrayError};

use super::ScriptError;

pub type HashValue = [u8; 32];
pub type ScalarValue = [u8; 32];
pub type Message = [u8; MESSAGE_LENGTH];

const PUBLIC_KEY_LENGTH: usize = 32;
const MESSAGE_LENGTH: usize = 32;
type MultiSigArgs = (u8, u8, Vec<RistrettoPublicKey>, Box<Message>, usize);

/// Convert a slice into a HashValue.
///
/// # Panics
///
/// The function does not check slice for length at all.  You need to check this / guarantee it yourself.
pub fn slice_to_hash(slice: &[u8]) -> HashValue {
    let mut hash = [0u8; 32];
    hash.copy_from_slice(slice);
    hash
}

/// Convert a slice into a Boxed HashValue
pub fn slice_to_boxed_hash(slice: &[u8]) -> Box<HashValue> {
    Box::new(slice_to_hash(slice))
}

/// Convert a slice into a Message.
///
/// # Panics
///
/// The function does not check slice for length at all.  You need to check this / guarantee it yourself.
pub fn slice_to_message(slice: &[u8]) -> Message {
    let mut msg = [0u8; MESSAGE_LENGTH];
    msg.copy_from_slice(slice);
    msg
}

/// Convert a slice into a Boxed Message
pub fn slice_to_boxed_message(slice: &[u8]) -> Box<Message> {
    Box::new(slice_to_message(slice))
}

/// Convert a slice into a vector of Public Keys.
pub fn slice_to_vec_pubkeys(slice: &[u8], num: usize) -> Result<Vec<RistrettoPublicKey>, ScriptError> {
    if slice.len() < num * PUBLIC_KEY_LENGTH {
        return Err(ScriptError::InvalidData);
    }

    let public_keys = slice
        .chunks_exact(PUBLIC_KEY_LENGTH)
        .take(num)
        .map(RistrettoPublicKey::from_canonical_bytes)
        .collect::<Result<Vec<RistrettoPublicKey>, ByteArrayError>>()?;

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
    /// Pops the top of the stack as `height`. Compare the current block height to `height`. Fails with
    /// `IncompatibleTypes` if u64 is not a valid 64-bit unsigned integer. Fails with `VerifyFailed` if the block
    /// height < `height`.
    CheckHeightVerify(u64),
    /// Pops the top of the stack as `height`. Pushes the value of (the current tip height - `height`) to the stack. In
    /// other words, the top of the stack will hold the height difference between `height` and the current height.
    /// If the chain has progressed beyond `height`, the value is positive; and negative if the chain has yet to
    /// reach `height`. Fails with `IncompatibleTypes` if u64 is not a valid 64-bit unsigned integer. Fails with
    /// `StackOverflow` if the stack would exceed the max stack height.
    CheckHeight(u64),
    /// Pops the top of the stack as `height` and compares it to the current block height. Fails with `InvalidInput`
    /// if there is not a valid integer value on top of the stack. Fails with `StackUnderflow` if the stack is empty.
    /// Fails with `VerifyFailed` if the block height < `height`.
    CompareHeightVerify,
    /// Pops the top of the stack as `height`, then pushes the value of (the current height - `height`) to the stack.
    /// In other words, this opcode replaces the top of the stack with the difference between `height` and the
    /// current height. Fails with `InvalidInput` if there is not a valid integer value on top of the stack. Fails
    /// with `StackUnderflow` if the stack is empty.
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
    PushPubKey(Box<RistrettoPublicKey>),
    /// Drops the top stack item. Fails with `StackUnderflow` if the stack is empty.
    Drop,
    /// Duplicates the top stack item. Fails with `StackUnderflow` if the stack is empty. Fails with `StackOverflow` if
    /// the stack would exceed the max stack height.
    Dup,
    /// Reverse rotation. The top stack item moves into 3rd place, e.g. abc => bca. Fails with `StackUnderflow` if the
    /// stack has fewer than three items.
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
    /// A 0 is also pushed if the values cannot be compared (e.g. integer and pubkey). Fails with `StackUnderflow` if
    /// the stack has fewer than two items.
    Equal,
    /// Pops the top two items from the stack, and compares their values. Fails with `StackUnderflow` if the stack has
    /// fewer than two items. Fails with `VerifyFailed` if the top two stack elements are not equal.
    EqualVerify,

    // Boolean Logic
    /// Pops `n` + 1 items from the stack (with u8 as `n`). If the last item matches at least one of the first `n`
    /// items, push 1 onto the stack, otherwise push 0 onto the stack. Fails with `StackUnderflow` if the stack has
    /// fewer than `n` + 1 items. Fails with `InvalidInput` if u8 is not a valid 8-bit unsigned integer.
    Or(u8),
    /// Pops `n` + 1 items from the stack (with u8 as `n`). If the last item matches at least one of the first n items,
    /// continue. Fails with `StackUnderflow` if the stack has fewer than `n` + 1 items. Fails with `VerifyFailed`
    /// the last item does not match at least one of the first `n` items. Fails with `InvalidInput` if u8 is not a
    /// valid 8-bit unsigned integer.
    OrVerify(u8),

    // Cryptographic Operations
    /// Pops the top element, hash it with the Blake2b<U32> hash function and push the result to the stack. Fails with
    /// `StackUnderflow` if the stack is empty. Fails with `InvalidInput` if the input is not a valid 32 byte hash
    /// value.
    HashBlake256,
    /// Pops the top element, hash it with the SHA256 hash function and push the result to the stack. Fails with
    /// `StackUnderflow` if the stack is empty. Fails with `InvalidInput` if the input is not a valid 32 byte hash
    /// value.
    HashSha256,
    /// Pops the top element, hash it with the SHA-3 hash function and push the result to the stack. Fails with
    /// `StackUnderflow` if the stack is empty. Fails with `InvalidInput` if the input is not a valid 32 byte hash
    /// value.
    HashSha3,
    /// Pops the public key and then the signature from the stack. If signature validation using the 32-byte message
    /// and public key succeeds , push 1 to the stack, otherwise push 0. Fails with `IncompatibleTypes` if Message
    /// is not a valid 32-byte sequence. Fails with `StackUnderflow` if the stack has fewer than 2 items. Fails
    /// with `InvalidInput` if the top stack element is not a PublicKey. Fails with `InvalidInput` if the second
    /// stack element is not a Signature.
    CheckSig(Box<Message>),
    /// Identical to CheckSig, except that nothing is pushed to the stack if the signature is valid, and the operation
    /// fails with `VerifyFailed` if the signature is invalid.
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
    CheckMultiSig(u8, u8, Vec<RistrettoPublicKey>, Box<Message>),
    /// Identical to CheckMultiSig, except that nothing is pushed to the stack if the multiple signature validation is
    /// either valid or invalid. Fails with `VerifyFailed` if any signature is invalid.
    CheckMultiSigVerify(u8, u8, Vec<RistrettoPublicKey>, Box<Message>),
    /// Identical to CheckMultiSig, except that the aggregate of the public keys is pushed to the stack if multiple
    /// signature validation succeeds. Fails with `VerifyFailed` if any signature is invalid.
    CheckMultiSigVerifyAggregatePubKey(u8, u8, Vec<RistrettoPublicKey>, Box<Message>),
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

    pub fn parse(bytes: &[u8]) -> Result<Vec<Opcode>, ScriptError> {
        let mut script = Vec::new();
        let mut bytes_copy = bytes;

        while !bytes_copy.is_empty() {
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
        match *code {
            OP_CHECK_HEIGHT_VERIFY => {
                let (height, size) = u64::decode_var(&bytes[1..]).ok_or(ScriptError::InvalidData)?;
                Ok((CheckHeightVerify(height), &bytes[size + 1..]))
            },
            OP_CHECK_HEIGHT => {
                let (height, size) = u64::decode_var(&bytes[1..]).ok_or(ScriptError::InvalidData)?;
                Ok((CheckHeight(height), &bytes[size + 1..]))
            },
            OP_COMPARE_HEIGHT_VERIFY => Ok((CompareHeightVerify, &bytes[1..])),
            OP_COMPARE_HEIGHT => Ok((CompareHeight, &bytes[1..])),
            OP_NOP => Ok((Nop, &bytes[1..])),
            OP_PUSH_ZERO => Ok((PushZero, &bytes[1..])),
            OP_PUSH_ONE => Ok((PushOne, &bytes[1..])),
            OP_PUSH_HASH => {
                if bytes.len() < 33 {
                    return Err(ScriptError::InvalidData);
                }
                let hash = slice_to_boxed_hash(&bytes[1..33]);
                Ok((PushHash(hash), &bytes[33..]))
            },
            OP_PUSH_INT => {
                let (n, size) = i64::decode_var(&bytes[1..]).ok_or(ScriptError::InvalidData)?;
                Ok((PushInt(n), &bytes[size + 1..]))
            },
            OP_PUSH_PUBKEY => {
                if bytes.len() < 33 {
                    return Err(ScriptError::InvalidData);
                }
                let p = RistrettoPublicKey::from_canonical_bytes(&bytes[1..33])?;
                Ok((PushPubKey(Box::new(p)), &bytes[33..]))
            },
            OP_DROP => Ok((Drop, &bytes[1..])),
            OP_DUP => Ok((Dup, &bytes[1..])),
            OP_REV_ROT => Ok((RevRot, &bytes[1..])),
            OP_GE_ZERO => Ok((GeZero, &bytes[1..])),
            OP_GT_ZERO => Ok((GtZero, &bytes[1..])),
            OP_LE_ZERO => Ok((LeZero, &bytes[1..])),
            OP_LT_ZERO => Ok((LtZero, &bytes[1..])),
            OP_ADD => Ok((Add, &bytes[1..])),
            OP_SUB => Ok((Sub, &bytes[1..])),
            OP_EQUAL => Ok((Equal, &bytes[1..])),
            OP_EQUAL_VERIFY => Ok((EqualVerify, &bytes[1..])),
            OP_OR => {
                if bytes.len() < 2 {
                    return Err(ScriptError::InvalidData);
                }
                let n = &bytes[1];
                Ok((Or(*n), &bytes[2..]))
            },
            OP_OR_VERIFY => {
                if bytes.len() < 2 {
                    return Err(ScriptError::InvalidData);
                }
                let n = &bytes[1];
                Ok((OrVerify(*n), &bytes[2..]))
            },
            OP_HASH_BLAKE256 => Ok((HashBlake256, &bytes[1..])),
            OP_HASH_SHA256 => Ok((HashSha256, &bytes[1..])),
            OP_HASH_SHA3 => Ok((HashSha3, &bytes[1..])),
            OP_CHECK_SIG => {
                if bytes.len() < 33 {
                    return Err(ScriptError::InvalidData);
                }
                let msg = slice_to_boxed_message(&bytes[1..33]);
                Ok((CheckSig(msg), &bytes[33..]))
            },
            OP_CHECK_SIG_VERIFY => {
                if bytes.len() < 33 {
                    return Err(ScriptError::InvalidData);
                }
                let msg = slice_to_boxed_message(&bytes[1..33]);
                Ok((CheckSigVerify(msg), &bytes[33..]))
            },
            OP_CHECK_MULTI_SIG => {
                let (m, n, keys, msg, end) = Opcode::read_multisig_args(bytes)?;
                Ok((CheckMultiSig(m, n, keys, msg), &bytes[end..]))
            },
            OP_CHECK_MULTI_SIG_VERIFY => {
                let (m, n, keys, msg, end) = Opcode::read_multisig_args(bytes)?;
                Ok((CheckMultiSigVerify(m, n, keys, msg), &bytes[end..]))
            },
            OP_CHECK_MULTI_SIG_VERIFY_AGGREGATE_PUB_KEY => {
                let (m, n, keys, msg, end) = Opcode::read_multisig_args(bytes)?;
                Ok((CheckMultiSigVerifyAggregatePubKey(m, n, keys, msg), &bytes[end..]))
            },
            OP_TO_RISTRETTO_POINT => Ok((ToRistrettoPoint, &bytes[1..])),
            OP_RETURN => Ok((Return, &bytes[1..])),
            OP_IF_THEN => Ok((IfThen, &bytes[1..])),
            OP_ELSE => Ok((Else, &bytes[1..])),
            OP_END_IF => Ok((EndIf, &bytes[1..])),
            _ => Err(ScriptError::InvalidOpcode),
        }
    }

    fn read_multisig_args(bytes: &[u8]) -> Result<MultiSigArgs, ScriptError> {
        if bytes.len() < 3 {
            return Err(ScriptError::InvalidData);
        }
        let m = &bytes[1];
        let n = &bytes[2];
        let num = *n as usize;
        let len = 3 + num * PUBLIC_KEY_LENGTH;
        let end = len + MESSAGE_LENGTH;
        if bytes.len() < end {
            return Err(ScriptError::InvalidData);
        }
        let keys = slice_to_vec_pubkeys(&bytes[3..len], num)?;
        let msg = slice_to_boxed_message(&bytes[len..end]);

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
                array.extend_from_slice(&buf[0..used]);
            },
            CheckHeight(height) => {
                array.push(OP_CHECK_HEIGHT);
                let mut buf = [0u8; 10];
                let used = height.encode_var(&mut buf[..]);
                array.extend_from_slice(&buf[0..used]);
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
                array.extend_from_slice(&buf[0..used]);
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

        &array[n..]
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

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum OpcodeVersion {
    V0 = 0,
}

#[cfg(test)]
mod test {
    use crate::op_codes::*;

    #[test]
    fn empty_script() {
        assert_eq!(Opcode::parse(&[]).unwrap(), Vec::new())
    }

    #[test]
    fn parse() {
        let script = [0xFF, 0x71, 0x00];
        let err = Opcode::parse(&script).unwrap_err();
        assert!(matches!(err, ScriptError::InvalidOpcode));

        let script = [0x60u8, 0x71];
        let opcodes = Opcode::parse(&script).unwrap();
        let code = opcodes.first().unwrap();
        assert_eq!(code, &Opcode::Return);
        let code = opcodes.get(1).unwrap();
        assert_eq!(code, &Opcode::Dup);

        let err = Opcode::parse(&[0x7a]).unwrap_err();
        assert!(matches!(err, ScriptError::InvalidData));
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
            assert_eq!(format!("{}", op).as_str(), display);
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
        assert_eq!(format!("{}", op).as_str(), "PushInt(257)");
    }

    #[test]
    fn push_pubkey() {
        // Serialise
        assert!(matches!(
            Opcode::read_next(b"\x7eshort_needs_33_bytes"),
            Err(ScriptError::InvalidData)
        ));
        let key =
            RistrettoPublicKey::from_hex("6c9cb4d3e57351462122310fa22c90b1e6dfb528d64615363d1261a75da3e401").unwrap();
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
            format!("{}", op).as_str(),
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
            assert_eq!(format!("{}", op).as_str(), display);
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
            assert_eq!(format!("{}", op).as_str(), display);
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
            assert_eq!(format!("{}", op).as_str(), display);
        }
        let msg = &[
            108, 156, 180, 211, 229, 115, 81, 70, 33, 34, 49, 15, 162, 44, 144, 177, 230, 223, 181, 40, 214, 70, 21,
            54, 61, 18, 97, 167, 93, 163, 228, 1,
        ];
        let p1 = "9c8bc5f90d221191748e8dd7686f09e1114b4bada4c367ed58ae199c51eb100b";
        let p2 = "56e9f018b138ba843521b3243a29d81730c3a4c25108b108b1ca47c2132db569";
        let keys = vec![
            RistrettoPublicKey::from_hex(p1).unwrap(),
            RistrettoPublicKey::from_hex(p2).unwrap(),
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
            let s = format!("{}", opcode);
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
        let key =
            RistrettoPublicKey::from_hex("6c9cb4d3e57351462122310fa22c90b1e6dfb528d64615363d1261a75da3e401").unwrap();
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
        let key =
            RistrettoPublicKey::from_hex("6c9cb4d3e57351462122310fa22c90b1e6dfb528d64615363d1261a75da3e401").unwrap();
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
}
