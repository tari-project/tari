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

// pending updates to Dalek/Digest
use std::{cmp::Ordering, collections::HashSet, fmt, io, ops::Deref};

use blake2::Blake2b;
use borsh::{BorshDeserialize, BorshSerialize};
use digest::{Digest, consts::U32};
use integer_encoding::{VarIntReader, VarIntWriter};
use sha2::Sha256;
use sha3::Sha3_256;
use tari_crypto::{
    compressed_commitment::CompressedCommitment,
    compressed_key::CompressedKey,
    ristretto::{RistrettoPublicKey, RistrettoSecretKey},
};
use tari_max_size::MaxSizeVec;
use tari_utilities::{
    ByteArray,
    hex::{Hex, HexError, from_hex, to_hex},
};

use crate::{
    CompressedCheckSigSchnorrSignature,
    ExecutionStack,
    HashValue,
    Opcode,
    ScriptContext,
    ScriptError,
    StackItem,
    op_codes::Message,
    slice_to_hash,
};

#[macro_export]
macro_rules! script {
    ($($opcode:ident$(($($var:expr),+))?) +) => {{
        use $crate::TariScript;
        use $crate::Opcode;
        let script = vec![$(Opcode::$opcode $(($($var),+))?),+];
        TariScript::new(script)
    }}
}

const MAX_MULTISIG_LIMIT: u8 = 32;
/// The maximum length of a serialised script accepted by [TariScript::from_bytes]
pub const MAX_SCRIPT_BYTES: usize = 4096;
/// The maximum number of opcodes in a script
pub const MAX_SCRIPT_OPCODES: usize = 128;

/// The sized vector of opcodes that make up a script
pub type ScriptOpcodes = MaxSizeVec<Opcode, MAX_SCRIPT_OPCODES>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TariScript {
    script: ScriptOpcodes,
}

impl BorshSerialize for TariScript {
    fn serialize<W: io::Write>(&self, writer: &mut W) -> io::Result<()> {
        let bytes = self.to_bytes();
        writer.write_varint(bytes.len())?;
        for b in &bytes {
            b.serialize(writer)?;
        }
        Ok(())
    }
}

impl BorshDeserialize for TariScript {
    fn deserialize_reader<R>(reader: &mut R) -> Result<Self, io::Error>
    where R: io::Read {
        let len = reader.read_varint()?;
        if len > MAX_SCRIPT_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Larger than max script bytes".to_string(),
            ));
        }
        let mut data = Vec::with_capacity(len);
        for _ in 0..len {
            data.push(u8::deserialize_reader(reader)?);
        }
        let script = TariScript::from_bytes(data.as_slice())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        Ok(script)
    }
}

impl TariScript {
    /// Create a script from a list of opcodes.
    ///
    /// Returns [ScriptError::InvalidData] if a `CheckMultiSig*` opcode's `n` does not equal the number of public keys
    /// it carries. Such an opcode would serialise to bytes that deserialise to a different script (the serialised `n`
    /// determines how many keys are read back). [TariScript::from_bytes] can never produce such an opcode.
    pub fn new(script: Vec<Opcode>) -> Result<Self, ScriptError> {
        for opcode in &script {
            if let Opcode::CheckMultiSig(_, n, keys, _) |
            Opcode::CheckMultiSigVerify(_, n, keys, _) |
            Opcode::CheckMultiSigVerifyAggregatePubKey(_, n, keys, _) = opcode &&
                *n as usize != keys.len()
            {
                return Err(ScriptError::InvalidData);
            }
        }
        let script = ScriptOpcodes::try_from(script)?;
        Ok(TariScript { script })
    }

    /// Returns true if the result of executing this script can depend on the [ScriptContext] it is executed with,
    /// i.e. if it contains any of `CheckHeightVerify`, `CheckHeight`, `CompareHeightVerify` or `CompareHeight`. These
    /// are currently the only opcodes that read the context, and they read only its block height.
    pub fn is_context_sensitive(&self) -> bool {
        self.script.iter().any(|op| {
            matches!(
                op,
                Opcode::CheckHeightVerify(_) |
                    Opcode::CheckHeight(_) |
                    Opcode::CompareHeightVerify |
                    Opcode::CompareHeight
            )
        })
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Opcode> {
        self.script.iter()
    }

    /// This pattern matches two scripts ensure they have the same instructions in the opcodes, but not the same values
    /// inside example:
    /// Script A = {PushPubKey(AA)}, Script B = {PushPubKey(BB)} will pattern match, but doing Script A == Script B will
    /// not match Script A = {PushPubKey(AA)}, Script B = {PushPubKey(AA)} will pattern match, doing Script A ==
    /// Script B will also match Script A = {PushPubKey(AA)}, Script B = {PushHash(BB)} will not pattern match, and
    /// doing Script A == Script B will not match
    pub fn pattern_match(&self, script: &TariScript) -> bool {
        for (i, opcode) in self.script.iter().enumerate() {
            if let Some(code) = script.opcode(i) {
                if std::mem::discriminant(opcode) != std::mem::discriminant(code) {
                    return false;
                }
            } else {
                return false;
            }
        }
        // We need to ensure they are the same length
        script.opcode(self.script.len()).is_none()
    }

    /// Retrieve the opcode at the index, returns None if the index does not exist
    pub fn opcode(&self, i: usize) -> Option<&Opcode> {
        let opcode = self.script.get(i)?;
        Some(opcode)
    }

    /// Executes the script using a default context. If successful, returns the final stack item.
    ///
    /// The default [ScriptContext] has a block height of 0 (and a zero previous block hash and commitment), so any
    /// height-dependent opcode (see [TariScript::is_context_sensitive]) is evaluated as if at the genesis block. Use
    /// [TariScript::execute_with_context] to evaluate a script at a particular height.
    pub fn execute(&self, inputs: &ExecutionStack) -> Result<StackItem, ScriptError> {
        self.execute_with_context(inputs, &ScriptContext::default())
    }

    /// Execute the script with the given inputs and the provided context. If successful, returns the final stack item.
    pub fn execute_with_context(
        &self,
        inputs: &ExecutionStack,
        context: &ScriptContext,
    ) -> Result<StackItem, ScriptError> {
        // Copy all inputs onto the stack
        let mut stack = inputs.clone();
        // Local execution state
        let mut state = ExecutionState::default();

        for opcode in self.script.iter() {
            if self.should_execute(opcode, &state)? {
                self.execute_opcode(opcode, &mut stack, context, &mut state)?
            } else {
                continue;
            }
        }

        // the script has finished but there was an open IfThen or Else!
        if !state.if_stack.is_empty() {
            return Err(ScriptError::MissingOpcode);
        }

        // After the script completes, it is successful if and only if it has not aborted, and there is exactly a single
        // element on the stack. The script fails if the stack is empty, or contains more than one element, or aborts
        // early.
        if stack.size() == 1 {
            stack.pop().ok_or(ScriptError::NonUnitLengthStack)
        } else {
            Err(ScriptError::NonUnitLengthStack)
        }
    }

    /// Returns the number of script op codes
    pub fn size(&self) -> usize {
        self.script.len()
    }

    fn should_execute(&self, opcode: &Opcode, state: &ExecutionState) -> Result<bool, ScriptError> {
        use Opcode::{Else, EndIf, IfThen};
        match opcode {
            // always execute these, they will update execution state
            IfThen | Else | EndIf => Ok(true),
            // otherwise keep calm and carry on
            _ => Ok(state.executing),
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        self.script.iter().fold(Vec::new(), |mut bytes, op| {
            op.to_bytes(&mut bytes);
            bytes
        })
    }

    pub fn as_slice(&self) -> &[Opcode] {
        self.script.as_ref()
    }

    /// Calculate the hash of the script.
    /// `as_hash` returns [ScriptError::InvalidDigest] if the digest function does not produce at least 32 bytes of
    /// output.
    pub fn as_hash<D: Digest>(&self) -> Result<HashValue, ScriptError> {
        if <D as Digest>::output_size() < 32 {
            return Err(ScriptError::InvalidDigest);
        }
        let h = D::digest(self.to_bytes());
        slice_to_hash(h.as_slice().get(..32).ok_or(ScriptError::InvalidDigest)?)
    }

    /// Try to deserialise a byte slice into a valid Tari script. Inputs longer than [MAX_SCRIPT_BYTES] are rejected
    /// before any parsing takes place.
    ///
    /// Decoding is lenient in two respects, which are part of current consensus behaviour:
    /// * Non-minimal varint encodings of integer arguments (e.g. `[0x67, 0x80, 0x00]` for `CheckHeight(0)`) are
    ///   accepted. [TariScript::to_bytes] always re-encodes minimally, and consensus hashes are computed over that
    ///   canonical encoding, so two byte strings that decode to the same script hash identically.
    /// * An empty byte slice is accepted and yields a script with zero opcodes. Executing it simply checks that the
    ///   input stack holds exactly one item, and returns that item.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ScriptError> {
        if bytes.len() > MAX_SCRIPT_BYTES {
            return Err(ScriptError::ScriptTooLarge {
                max: MAX_SCRIPT_BYTES,
                actual: bytes.len(),
            });
        }
        let script = ScriptOpcodes::try_from(Opcode::parse(bytes, MAX_SCRIPT_OPCODES)?)?;

        Ok(TariScript { script })
    }

    /// Convert the script into an array of opcode strings.
    ///
    /// # Example
    /// ```edition2018
    /// use tari_script::TariScript;
    /// use tari_utilities::hex::Hex;
    ///
    /// let hex_script = "71b07aae2337ce44f9ebb6169c863ec168046cb35ab4ef7aa9ed4f5f1f669bb74b09e58170ac276657a418820f34036b20ea615302b373c70ac8feab8d30681a3e0f0960e708";
    /// let script = TariScript::from_hex(hex_script).unwrap();
    /// let ops = vec![
    ///     "Dup",
    ///     "HashBlake256",
    ///     "PushHash(ae2337ce44f9ebb6169c863ec168046cb35ab4ef7aa9ed4f5f1f669bb74b09e5)",
    ///     "EqualVerify",
    ///     "Drop",
    ///     "CheckSig(276657a418820f34036b20ea615302b373c70ac8feab8d30681a3e0f0960e708)",
    /// ]
    /// .into_iter()
    /// .map(String::from)
    /// .collect::<Vec<String>>();
    /// assert_eq!(script.to_opcodes(), ops);
    /// ```
    pub fn to_opcodes(&self) -> Vec<String> {
        self.script.iter().map(|op| op.to_string()).collect()
    }

    // pending updates to Dalek/Digest
    fn execute_opcode(
        &self,
        opcode: &Opcode,
        stack: &mut ExecutionStack,
        ctx: &ScriptContext,
        state: &mut ExecutionState,
    ) -> Result<(), ScriptError> {
        #[allow(clippy::enum_glob_use)]
        use Opcode::*;
        use StackItem::{Hash, Number, PublicKey};
        match opcode {
            CheckHeightVerify(height) => TariScript::handle_check_height_verify(*height, ctx.block_height()),
            CheckHeight(height) => TariScript::handle_check_height(stack, *height, ctx.block_height()),
            CompareHeightVerify => TariScript::handle_compare_height_verify(stack, ctx.block_height()),
            CompareHeight => TariScript::handle_compare_height(stack, ctx.block_height()),
            Nop => Ok(()),
            PushZero => stack.push(Number(0)),
            PushOne => stack.push(Number(1)),
            PushHash(h) => stack.push(Hash(*h.clone())),
            PushInt(n) => stack.push(Number(*n)),
            PushPubKey(p) => stack.push(PublicKey(*p.clone())),
            Drop => TariScript::handle_drop(stack),
            Dup => TariScript::handle_dup(stack),
            RevRot => stack.push_down(2),
            GeZero => TariScript::handle_cmp_to_zero(stack, &[Ordering::Greater, Ordering::Equal]),
            GtZero => TariScript::handle_cmp_to_zero(stack, &[Ordering::Greater]),
            LeZero => TariScript::handle_cmp_to_zero(stack, &[Ordering::Less, Ordering::Equal]),
            LtZero => TariScript::handle_cmp_to_zero(stack, &[Ordering::Less]),
            Add => TariScript::handle_op_add(stack),
            Sub => TariScript::handle_op_sub(stack),
            Equal => {
                if TariScript::handle_equal(stack)? {
                    stack.push(Number(1))
                } else {
                    stack.push(Number(0))
                }
            },
            EqualVerify => {
                if TariScript::handle_equal(stack)? {
                    Ok(())
                } else {
                    Err(ScriptError::VerifyFailed)
                }
            },
            Or(n) => TariScript::handle_or(stack, *n),
            OrVerify(n) => TariScript::handle_or_verify(stack, *n),
            HashBlake256 => TariScript::handle_hash::<Blake2b<U32>>(stack),
            HashSha256 => TariScript::handle_hash::<Sha256>(stack),
            HashSha3 => TariScript::handle_hash::<Sha3_256>(stack),
            CheckSig(msg) => {
                if self.check_sig(stack, *msg.deref())? {
                    stack.push(Number(1))
                } else {
                    stack.push(Number(0))
                }
            },
            CheckSigVerify(msg) => {
                if self.check_sig(stack, *msg.deref())? {
                    Ok(())
                } else {
                    Err(ScriptError::VerifyFailed)
                }
            },
            CheckMultiSig(m, n, public_keys, msg) => {
                if self.check_multisig(stack, *m, *n, public_keys, *msg.deref())?.is_some() {
                    stack.push(Number(1))
                } else {
                    stack.push(Number(0))
                }
            },
            CheckMultiSigVerify(m, n, public_keys, msg) => {
                if self.check_multisig(stack, *m, *n, public_keys, *msg.deref())?.is_some() {
                    Ok(())
                } else {
                    Err(ScriptError::VerifyFailed)
                }
            },
            CheckMultiSigVerifyAggregatePubKey(m, n, public_keys, msg) => {
                if let Some(agg_pub_key) = self.check_multisig(stack, *m, *n, public_keys, *msg.deref())? {
                    stack.push(PublicKey(agg_pub_key))
                } else {
                    Err(ScriptError::VerifyFailed)
                }
            },
            ToRistrettoPoint => self.handle_to_ristretto_point(stack),
            Return => Err(ScriptError::Return),
            IfThen => TariScript::handle_if_then(stack, state),
            Else => TariScript::handle_else(state),
            EndIf => TariScript::handle_end_if(state),
        }
    }

    fn handle_check_height_verify(height: u64, block_height: u64) -> Result<(), ScriptError> {
        if block_height >= height {
            Ok(())
        } else {
            Err(ScriptError::VerifyFailed)
        }
    }

    fn handle_check_height(stack: &mut ExecutionStack, height: u64, block_height: u64) -> Result<(), ScriptError> {
        let height = i64::try_from(height)?;
        let block_height = i64::try_from(block_height)?;

        // Due to the conversion of u64 into i64 which would fail above if they overflowed, these
        // numbers should never enter a state where a `sub` could fail. As they'd both be within range and 0 or above.
        // This differs from compare_height due to a stack number being used, which can be lower than 0
        let item = match block_height.checked_sub(height) {
            Some(num) => StackItem::Number(num),
            None => {
                return Err(ScriptError::CompareFailed(
                    "Subtraction of given height from current block height failed".to_string(),
                ));
            },
        };

        stack.push(item)
    }

    fn handle_compare_height_verify(stack: &mut ExecutionStack, block_height: u64) -> Result<(), ScriptError> {
        let target_height = stack.pop_into_number::<u64>()?;

        if block_height >= target_height {
            Ok(())
        } else {
            Err(ScriptError::VerifyFailed)
        }
    }

    fn handle_compare_height(stack: &mut ExecutionStack, block_height: u64) -> Result<(), ScriptError> {
        let target_height = stack.pop_into_number::<i64>()?;
        let block_height = i64::try_from(block_height)?;

        // Here it is possible to underflow because the stack can take lower numbers where check
        // height does not use a stack number and it's minimum can't be lower than 0.
        let item = match block_height.checked_sub(target_height) {
            Some(num) => StackItem::Number(num),
            None => {
                return Err(ScriptError::CompareFailed(
                    "Couldn't subtract the target height from the current block height".to_string(),
                ));
            },
        };

        stack.push(item)
    }

    fn handle_cmp_to_zero(stack: &mut ExecutionStack, valid_orderings: &[Ordering]) -> Result<(), ScriptError> {
        let stack_number = stack.pop_into_number::<i64>()?;
        let ordering = &stack_number.cmp(&0);

        if valid_orderings.contains(ordering) {
            stack.push(StackItem::Number(1))
        } else {
            stack.push(StackItem::Number(0))
        }
    }

    fn handle_or(stack: &mut ExecutionStack, n: u8) -> Result<(), ScriptError> {
        if stack.pop_n_plus_one_contains(n)? {
            stack.push(StackItem::Number(1))
        } else {
            stack.push(StackItem::Number(0))
        }
    }

    fn handle_or_verify(stack: &mut ExecutionStack, n: u8) -> Result<(), ScriptError> {
        if stack.pop_n_plus_one_contains(n)? {
            Ok(())
        } else {
            Err(ScriptError::VerifyFailed)
        }
    }

    fn handle_if_then(stack: &mut ExecutionStack, state: &mut ExecutionState) -> Result<(), ScriptError> {
        if state.executing {
            let pred = stack.pop().ok_or(ScriptError::StackUnderflow)?;
            match pred {
                StackItem::Number(1) => {
                    // continue execution until Else opcode
                    state.executing = true;
                    let if_state = IfState {
                        branch: Branch::ExecuteIf,
                        else_expected: true,
                    };
                    state.if_stack.push(if_state);
                    Ok(())
                },
                StackItem::Number(0) => {
                    // skip execution until Else opcode
                    state.executing = false;
                    let if_state = IfState {
                        branch: Branch::ExecuteElse,
                        else_expected: true,
                    };
                    state.if_stack.push(if_state);
                    Ok(())
                },
                _ => Err(ScriptError::InvalidInput),
            }
        } else {
            let if_state = IfState {
                branch: Branch::NotExecuted,
                else_expected: true,
            };
            state.if_stack.push(if_state);
            Ok(())
        }
    }

    fn handle_else(state: &mut ExecutionState) -> Result<(), ScriptError> {
        let if_state = state.if_stack.last_mut().ok_or(ScriptError::InvalidOpcode)?;

        // check to make sure Else is expected
        if !if_state.else_expected {
            return Err(ScriptError::InvalidOpcode);
        }

        match if_state.branch {
            Branch::NotExecuted => {
                state.executing = false;
            },
            Branch::ExecuteIf => {
                state.executing = false;
            },
            Branch::ExecuteElse => {
                state.executing = true;
            },
        }
        if_state.else_expected = false;
        Ok(())
    }

    fn handle_end_if(state: &mut ExecutionState) -> Result<(), ScriptError> {
        // check to make sure EndIf is expected
        let if_state = state.if_stack.pop().ok_or(ScriptError::InvalidOpcode)?;

        // check if we still expect an Else first
        if if_state.else_expected {
            return Err(ScriptError::MissingOpcode);
        }

        match if_state.branch {
            Branch::NotExecuted => {
                state.executing = false;
            },
            Branch::ExecuteIf => {
                state.executing = true;
            },
            Branch::ExecuteElse => {
                state.executing = true;
            },
        }
        Ok(())
    }

    /// Handle opcodes that push a hash to the stack. The raw, untagged 32-byte payload of a `Commitment`, `PublicKey`
    /// or `Hash` is hashed, so items of different types with the same bytes produce the same digest. Returns
    /// [ScriptError::InvalidDigest] if `D` does not produce exactly 32 bytes of output.
    fn handle_hash<D: Digest>(stack: &mut ExecutionStack) -> Result<(), ScriptError> {
        use StackItem::{Commitment, Hash, PublicKey};
        if <D as Digest>::output_size() != 32 {
            return Err(ScriptError::InvalidDigest);
        }
        let top = stack.pop().ok_or(ScriptError::StackUnderflow)?;
        // use a closure to grab &b while it still exists in the match expression
        let to_arr = |b: &[u8]| {
            let mut hash = [0u8; 32];
            hash.copy_from_slice(D::digest(b).as_slice());
            hash
        };
        let hash_value = match top {
            Commitment(c) => to_arr(c.as_bytes()),
            PublicKey(k) => to_arr(k.as_bytes()),
            Hash(h) => to_arr(&h),
            _ => return Err(ScriptError::IncompatibleTypes),
        };

        stack.push(Hash(hash_value))
    }

    fn handle_dup(stack: &mut ExecutionStack) -> Result<(), ScriptError> {
        let last = if let Some(last) = stack.peek() {
            last.clone()
        } else {
            return Err(ScriptError::StackUnderflow);
        };
        stack.push(last)
    }

    fn handle_drop(stack: &mut ExecutionStack) -> Result<(), ScriptError> {
        match stack.pop() {
            Some(_) => Ok(()),
            None => Err(ScriptError::StackUnderflow),
        }
    }

    // The `+` operators below are Ristretto group additions on commitments and public keys,
    // which cannot overflow. Integer addition here is already checked.
    #[allow(clippy::arithmetic_side_effects)]
    fn handle_op_add(stack: &mut ExecutionStack) -> Result<(), ScriptError> {
        use StackItem::{Commitment, Number, PublicKey};
        let top = stack.pop().ok_or(ScriptError::StackUnderflow)?;
        let two = stack.pop().ok_or(ScriptError::StackUnderflow)?;
        match (top, two) {
            (Number(v1), Number(v2)) => stack.push(Number(v1.checked_add(v2).ok_or(ScriptError::ValueExceedsBounds)?)),
            (Commitment(c1), Commitment(c2)) => {
                let com_1 = c1.to_commitment()?;
                let com_2 = c2.to_commitment()?;
                stack.push(Commitment(CompressedCommitment::from_commitment(&com_1 + &com_2)))
            },
            (PublicKey(p1), PublicKey(p2)) => {
                let key1 = p1.to_public_key().map_err(|_| ScriptError::InvalidInput)?;
                let key2 = p2.to_public_key().map_err(|_| ScriptError::InvalidInput)?;
                let compressed_key = CompressedKey::new_from_pk(&key1 + &key2);
                stack.push(PublicKey(compressed_key))
            },
            (_, _) => Err(ScriptError::IncompatibleTypes),
        }
    }

    // The `-` operator below is a Ristretto group subtraction on commitments, which cannot
    // overflow. Integer subtraction here is already checked.
    #[allow(clippy::arithmetic_side_effects)]
    fn handle_op_sub(stack: &mut ExecutionStack) -> Result<(), ScriptError> {
        use StackItem::{Commitment, Number};
        let top = stack.pop().ok_or(ScriptError::StackUnderflow)?;
        let two = stack.pop().ok_or(ScriptError::StackUnderflow)?;
        match (top, two) {
            (Number(v1), Number(v2)) => stack.push(Number(v2.checked_sub(v1).ok_or(ScriptError::ValueExceedsBounds)?)),
            (Commitment(c1), Commitment(c2)) => {
                let com_1 = c1.to_commitment()?;
                let com_2 = c2.to_commitment()?;
                stack.push(Commitment(CompressedCommitment::from_commitment(&com_2 - &com_1)))
            },
            (..) => Err(ScriptError::IncompatibleTypes),
        }
    }

    /// Pops two items and compares them. Only items of the same type among `Number`, `Commitment`, `Signature`,
    /// `PublicKey` and `Hash` can be compared; any other pairing (including `Scalar` with `Scalar`) returns
    /// [ScriptError::IncompatibleTypes].
    fn handle_equal(stack: &mut ExecutionStack) -> Result<bool, ScriptError> {
        use StackItem::{Commitment, Hash, Number, PublicKey, Signature};
        let top = stack.pop().ok_or(ScriptError::StackUnderflow)?;
        let two = stack.pop().ok_or(ScriptError::StackUnderflow)?;
        match (top, two) {
            (Number(v1), Number(v2)) => Ok(v1 == v2),
            (Commitment(c1), Commitment(c2)) => Ok(c1 == c2),
            (Signature(s1), Signature(s2)) => Ok(s1 == s2),
            (PublicKey(p1), PublicKey(p2)) => Ok(p1 == p2),
            (Hash(h1), Hash(h2)) => Ok(h1 == h2),
            (..) => Err(ScriptError::IncompatibleTypes),
        }
    }

    fn check_sig(&self, stack: &mut ExecutionStack, message: Message) -> Result<bool, ScriptError> {
        use StackItem::{PublicKey, Signature};
        let pk = stack.pop().ok_or(ScriptError::StackUnderflow)?;
        let sig = stack.pop().ok_or(ScriptError::StackUnderflow)?;
        match (pk, sig) {
            (PublicKey(p), Signature(s)) => {
                let key = p.to_public_key().map_err(|_| ScriptError::InvalidInput)?;
                let sig = s.to_schnorr_signature()?;
                Ok(sig.verify(&key, message))
            },
            (..) => Err(ScriptError::IncompatibleTypes),
        }
    }

    /// Validates an m-of-n multisig script
    ///
    /// This validation broadly proceeds to check if **exactly** _m_ signatures are valid signatures out of a
    /// possible _n_ public keys.
    ///
    /// A successful validation returns `Ok(P)` where _P_ is the sum of the public keys that matched the _m_
    /// signatures. If the validation was NOT successful, `check_multisig` returns `Ok(None)`. This is a private
    /// function, and callers will interpret these results according to their use cases.
    ///
    /// Other problems, such as stack underflows, invalid parameters etc return an `Err` as usual.
    ///
    /// Notes:
    /// * The _m_ signatures are expected to be the top _m_ items on the stack.
    /// * The ordering of signatures on the stack MUST match the relative ordering of the corresponding public keys.
    /// * The list may contain duplicate keys, but each occurrence of a public key may be used AT MOST once.
    /// * Every signature MUST be a valid signature using one of the public keys
    /// * _m_ and _n_ must be positive AND m <= n AND n <= MAX_MULTISIG_LIMIT (32).
    // `agg_pub_key + ristretto_key` is a Ristretto group addition, which cannot overflow.
    #[allow(clippy::arithmetic_side_effects)]
    fn check_multisig(
        &self,
        stack: &mut ExecutionStack,

        m: u8,
        n: u8,
        public_keys: &[CompressedKey<RistrettoPublicKey>],
        message: Message,
    ) -> Result<Option<CompressedKey<RistrettoPublicKey>>, ScriptError> {
        if m == 0 || n == 0 || m > n || n > MAX_MULTISIG_LIMIT || public_keys.len() != n as usize {
            return Err(ScriptError::ValueExceedsBounds);
        }
        // pop m sigs
        let m = m as usize;
        let signatures = stack
            .pop_num_items(m)?
            .into_iter()
            .map(|item| match item {
                StackItem::Signature(s) => Ok(s),
                _ => Err(ScriptError::IncompatibleTypes),
            })
            .collect::<Result<Vec<CompressedCheckSigSchnorrSignature>, ScriptError>>()?;

        // keep a hashset of unique signatures used to prevent someone putting the same signature in more than once.
        #[allow(clippy::mutable_key_type)]
        let mut sig_set = HashSet::new();

        let mut agg_pub_key = RistrettoPublicKey::default();

        // Create an iterator that allows each pubkey to only be checked a single time as they are
        // removed from the collection when referenced
        let mut pub_keys = public_keys.iter();

        // Signatures and public keys must be ordered
        for s in &signatures {
            if pub_keys.len() == 0 {
                return Ok(None);
            }

            if sig_set.contains(s) {
                continue;
            }

            // Each public key is decompressed at most once, since `pub_keys` only moves forward. The signature is
            // decompressed once per outer iteration, on the first comparison (so that a key that fails to decompress
            // is still reported before a signature that fails to decompress), and reused for the remaining keys.
            let mut schnorr_sig = None;
            for pk in pub_keys.by_ref() {
                let ristretto_key = pk.to_public_key().map_err(|_| ScriptError::InvalidInput)?;
                if schnorr_sig.is_none() {
                    schnorr_sig = Some(s.to_schnorr_signature()?);
                }
                if let Some(sig) = &schnorr_sig &&
                    sig.verify(&ristretto_key, message)
                {
                    sig_set.insert(s);
                    agg_pub_key = agg_pub_key + ristretto_key;
                    break;
                }
            }
            // Make sure the signature matched a public key
            if !sig_set.contains(s) {
                return Ok(None);
            }
        }
        if sig_set.len() == m {
            let key = CompressedKey::new_from_pk(agg_pub_key);
            Ok(Some(key))
        } else {
            Ok(None)
        }
    }

    fn handle_to_ristretto_point(&self, stack: &mut ExecutionStack) -> Result<(), ScriptError> {
        let item = stack.pop().ok_or(ScriptError::StackUnderflow)?;
        let scalar = match &item {
            StackItem::Hash(hash) => hash.as_slice(),
            StackItem::Scalar(scalar) => scalar.as_slice(),
            _ => return Err(ScriptError::IncompatibleTypes),
        };
        let ristretto_sk = RistrettoSecretKey::from_canonical_bytes(scalar).map_err(|_| ScriptError::InvalidInput)?;
        let ristretto_pk = CompressedKey::from_secret_key(&ristretto_sk);
        stack.push(StackItem::PublicKey(ristretto_pk))?;
        Ok(())
    }
}

impl<'a> IntoIterator for &'a TariScript {
    type IntoIter = std::slice::Iter<'a, Opcode>;
    type Item = &'a Opcode;

    fn into_iter(self) -> Self::IntoIter {
        self.script.iter()
    }
}

impl Hex for TariScript {
    fn from_hex(hex: &str) -> Result<Self, HexError>
    where Self: Sized {
        let bytes = from_hex(hex)?;
        TariScript::from_bytes(&bytes).map_err(|_| HexError::HexConversionError {})
    }

    fn to_hex(&self) -> String {
        to_hex(&self.to_bytes())
    }
}

/// The default Tari script is to push a sender pubkey onto the stack
impl Default for TariScript {
    fn default() -> Self {
        script!(PushPubKey(Box::default())).expect("default will not fail")
    }
}

impl fmt::Display for TariScript {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = self.to_opcodes().join(" ");
        f.write_str(&s)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Branch {
    NotExecuted,
    ExecuteIf,
    ExecuteElse,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct IfState {
    branch: Branch,
    else_expected: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ExecutionState {
    executing: bool,
    if_stack: Vec<IfState>,
}

impl Default for ExecutionState {
    fn default() -> Self {
        Self {
            executing: true,
            if_stack: Vec::new(),
        }
    }
}

#[cfg(test)]
mod test {
    #![allow(clippy::indexing_slicing)]
    use blake2::Blake2b;
    use borsh::{BorshDeserialize, BorshSerialize};
    use digest::{Digest, consts::U32};
    use sha2::Sha256;
    use sha3::Sha3_256 as Sha3;
    use tari_crypto::{
        compressed_commitment::CompressedCommitment,
        compressed_key::CompressedKey,
        keys::SecretKey,
        ristretto::{RistrettoPublicKey, RistrettoSecretKey, pedersen::CompressedPedersenCommitment},
    };
    use tari_utilities::{
        ByteArray,
        hex::{Hex, to_hex},
    };

    use crate::{
        CheckSigSchnorrSignature,
        CompressedCheckSigSchnorrSignature,
        ExecutionStack,
        Opcode::CheckMultiSigVerifyAggregatePubKey,
        ScriptContext,
        StackItem,
        StackItem::{Commitment, Hash, Number},
        TariScript,
        error::ScriptError,
        inputs,
        op_codes::{HashValue, Message, slice_to_boxed_hash, slice_to_boxed_message},
        script::MAX_SCRIPT_BYTES,
    };

    fn context_with_height(height: u64) -> ScriptContext {
        ScriptContext::new(height, &HashValue::default(), &CompressedPedersenCommitment::default())
    }

    #[test]
    fn pattern_match() {
        let script_a = script!(Or(1)).unwrap();
        let script_b = script!(Or(1)).unwrap();
        assert_eq!(script_a, script_b);
        assert!(script_a.pattern_match(&script_b));

        let script_b = script!(Or(2)).unwrap();
        assert_ne!(script_a, script_b);
        assert!(script_a.pattern_match(&script_b));

        let script_b = script!(Or(2) Or(2)).unwrap();
        assert_ne!(script_a, script_b);
        assert!(!script_a.pattern_match(&script_b));

        let script_a = script!(Or(2) Or(1)).unwrap();
        let script_b = script!(Or(3) Or(5)).unwrap();
        assert_ne!(script_a, script_b);
        assert!(script_a.pattern_match(&script_b));
    }

    #[test]
    fn op_or() {
        let script = script!(Or(1)).unwrap();

        let inputs = inputs!(4, 4);
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));

        let inputs = inputs!(3, 4);
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(0));

        let script = script!(Or(3)).unwrap();

        let inputs = inputs!(1, 2, 1, 3);
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));

        let inputs = inputs!(1, 2, 4, 3);
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(0));

        let mut rng = rand::rng();
        let (_, p) = CompressedKey::<RistrettoPublicKey>::random_keypair(&mut rng);
        let inputs = inputs!(1, p.clone(), 1, 3);
        let err = script.execute(&inputs).unwrap_err();
        assert!(matches!(err, ScriptError::InvalidInput));

        let inputs = inputs!(p, 2, 1, 3);
        let err = script.execute(&inputs).unwrap_err();
        assert!(matches!(err, ScriptError::InvalidInput));

        let inputs = inputs!(2, 4, 3);
        let err = script.execute(&inputs).unwrap_err();
        assert!(matches!(err, ScriptError::StackUnderflow));

        let script = script!(OrVerify(1)).unwrap();

        let inputs = inputs!(1, 4, 4);
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));

        let inputs = inputs!(1, 3, 4);
        let err = script.execute(&inputs).unwrap_err();
        assert!(matches!(err, ScriptError::VerifyFailed));

        let script = script!(OrVerify(2)).unwrap();

        let inputs = inputs!(1, 2, 2, 3);
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));

        let inputs = inputs!(1, 2, 3, 4);
        let err = script.execute(&inputs).unwrap_err();
        assert!(matches!(err, ScriptError::VerifyFailed));
    }

    #[test]
    fn op_if_then_else() {
        // basic
        let script = script!(IfThen PushInt(420) Else PushInt(66) EndIf).unwrap();
        let inputs = inputs!(1);
        let result = script.execute(&inputs);
        assert_eq!(result.unwrap(), Number(420));

        let inputs = inputs!(0);
        let result = script.execute(&inputs);
        assert_eq!(result.unwrap(), Number(66));

        // nested
        let script =
            script!(IfThen PushOne IfThen PushInt(420) Else PushInt(555) EndIf Else PushInt(66) EndIf).unwrap();
        let inputs = inputs!(1);
        let result = script.execute(&inputs);
        assert_eq!(result.unwrap(), Number(420));

        let script =
            script!(IfThen PushInt(420) Else PushZero IfThen PushInt(111) Else PushInt(66) EndIf Nop EndIf).unwrap();
        let inputs = inputs!(0);
        let result = script.execute(&inputs);
        assert_eq!(result.unwrap(), Number(66));

        // duplicate else
        let script = script!(IfThen PushInt(420) Else PushInt(66) Else PushInt(777) EndIf).unwrap();
        let inputs = inputs!(0);
        let result = script.execute(&inputs);
        assert_eq!(result.unwrap_err(), ScriptError::InvalidOpcode);

        // unexpected else
        let script = script!(Else).unwrap();
        let inputs = inputs!(0);
        let result = script.execute(&inputs);
        assert_eq!(result.unwrap_err(), ScriptError::InvalidOpcode);

        // unexpected endif
        let script = script!(EndIf).unwrap();
        let inputs = inputs!(0);
        let result = script.execute(&inputs);
        assert_eq!(result.unwrap_err(), ScriptError::InvalidOpcode);

        // duplicate endif
        let script = script!(IfThen PushInt(420) Else PushInt(66) EndIf EndIf).unwrap();
        let inputs = inputs!(0);
        let result = script.execute(&inputs);
        assert_eq!(result.unwrap_err(), ScriptError::InvalidOpcode);

        // no else or endif
        let script = script!(IfThen PushOne IfThen PushOne).unwrap();
        let inputs = inputs!(1);
        let result = script.execute(&inputs);
        assert_eq!(result.unwrap_err(), ScriptError::MissingOpcode);

        // no else
        let script = script!(IfThen PushOne EndIf).unwrap();
        let inputs = inputs!(1);
        let result = script.execute(&inputs);
        assert_eq!(result.unwrap_err(), ScriptError::MissingOpcode);

        // nested bug
        let script =
            script!(IfThen PushInt(111) Else PushZero IfThen PushInt(222) Else PushInt(333) EndIf EndIf).unwrap();
        let inputs = inputs!(1);
        let result = script.execute(&inputs);
        assert_eq!(result.unwrap(), Number(111));
    }

    #[test]
    fn op_check_height() {
        let inputs = ExecutionStack::default();
        let script = script!(CheckHeight(5)).unwrap();

        for block_height in 1..=10 {
            let ctx = context_with_height(u64::try_from(block_height).unwrap());
            assert_eq!(
                script.execute_with_context(&inputs, &ctx).unwrap(),
                Number(block_height - 5)
            );
        }

        let script = script!(CheckHeight(u64::MAX)).unwrap();
        let ctx = context_with_height(i64::MAX as u64);
        let err = script.execute_with_context(&inputs, &ctx).unwrap_err();
        assert!(matches!(err, ScriptError::ValueExceedsBounds));

        let script = script!(CheckHeightVerify(5)).unwrap();
        let inputs = inputs!(1);

        for block_height in 1..5 {
            let ctx = context_with_height(block_height);
            let err = script.execute_with_context(&inputs, &ctx).unwrap_err();
            assert!(matches!(err, ScriptError::VerifyFailed));
        }

        for block_height in 5..=10 {
            let ctx = context_with_height(block_height);
            let result = script.execute_with_context(&inputs, &ctx).unwrap();
            assert_eq!(result, Number(1));
        }
    }

    #[test]
    fn op_compare_height() {
        let script = script!(CompareHeight).unwrap();
        let inputs = inputs!(5);

        for block_height in 1..=10 {
            let ctx = context_with_height(u64::try_from(block_height).unwrap());
            assert_eq!(
                script.execute_with_context(&inputs, &ctx).unwrap(),
                Number(block_height - 5)
            );
        }

        let script = script!(CompareHeightVerify).unwrap();
        let inputs = inputs!(1, 5);

        for block_height in 1..5 {
            let ctx = context_with_height(block_height);
            let err = script.execute_with_context(&inputs, &ctx).unwrap_err();
            assert!(matches!(err, ScriptError::VerifyFailed));
        }

        for block_height in 5..=10 {
            let ctx = context_with_height(block_height);
            let result = script.execute_with_context(&inputs, &ctx).unwrap();
            assert_eq!(result, Number(1));
        }
    }

    #[test]
    fn op_drop_push() {
        let inputs = inputs!(420);
        let script = script!(Drop PushOne).unwrap();
        assert_eq!(script.execute(&inputs).unwrap(), Number(1));

        let script = script!(Drop PushZero).unwrap();
        assert_eq!(script.execute(&inputs).unwrap(), Number(0));

        let script = script!(Drop PushInt(5)).unwrap();
        assert_eq!(script.execute(&inputs).unwrap(), Number(5));
    }

    #[test]
    fn op_comparison_to_zero() {
        let script = script!(GeZero).unwrap();
        let inputs = inputs!(1);
        assert_eq!(script.execute(&inputs).unwrap(), Number(1));
        let inputs = inputs!(0);
        assert_eq!(script.execute(&inputs).unwrap(), Number(1));

        let script = script!(GtZero).unwrap();
        let inputs = inputs!(1);
        assert_eq!(script.execute(&inputs).unwrap(), Number(1));
        let inputs = inputs!(0);
        assert_eq!(script.execute(&inputs).unwrap(), Number(0));

        let script = script!(LeZero).unwrap();
        let inputs = inputs!(-1);
        assert_eq!(script.execute(&inputs).unwrap(), Number(1));
        let inputs = inputs!(0);
        assert_eq!(script.execute(&inputs).unwrap(), Number(1));

        let script = script!(LtZero).unwrap();
        let inputs = inputs!(-1);
        assert_eq!(script.execute(&inputs).unwrap(), Number(1));
        let inputs = inputs!(0);
        assert_eq!(script.execute(&inputs).unwrap(), Number(0));
    }

    #[test]
    fn op_hash() {
        let mut rng = rand::rng();
        let (_, p) = CompressedKey::<RistrettoPublicKey>::random_keypair(&mut rng);
        let c = CompressedCommitment::<RistrettoPublicKey>::from_compressed_key(p.clone());
        let script = script!(HashSha256).unwrap();

        let hash = Sha256::digest(p.as_bytes());
        let inputs = inputs!(p.clone());
        assert_eq!(script.execute(&inputs).unwrap(), Hash(hash.into()));

        let hash = Sha256::digest(c.as_bytes());
        let inputs = inputs!(c.clone());
        assert_eq!(script.execute(&inputs).unwrap(), Hash(hash.into()));

        let script = script!(HashSha3).unwrap();

        let hash = Sha3::digest(p.as_bytes());
        let inputs = inputs!(p);
        assert_eq!(script.execute(&inputs).unwrap(), Hash(hash.into()));

        let hash = Sha3::digest(c.as_bytes());
        let inputs = inputs!(c);
        assert_eq!(script.execute(&inputs).unwrap(), Hash(hash.into()));
    }

    #[test]
    fn op_return() {
        let script = script!(Return).unwrap();
        let inputs = ExecutionStack::default();
        assert_eq!(script.execute(&inputs), Err(ScriptError::Return));
    }

    #[test]
    fn op_add() {
        let script = script!(Add).unwrap();
        let inputs = inputs!(3, 2);
        assert_eq!(script.execute(&inputs).unwrap(), Number(5));
        let inputs = inputs!(3, -3);
        assert_eq!(script.execute(&inputs).unwrap(), Number(0));
        let inputs = inputs!(i64::MAX, 1);
        assert_eq!(script.execute(&inputs), Err(ScriptError::ValueExceedsBounds));
        let inputs = inputs!(1);
        assert_eq!(script.execute(&inputs), Err(ScriptError::StackUnderflow));
    }

    #[test]
    fn op_add_commitments() {
        let script = script!(Add).unwrap();
        let mut rng = rand::rng();
        let (_, c1) = CompressedKey::<RistrettoPublicKey>::random_keypair(&mut rng);
        let (_, c2) = CompressedKey::<RistrettoPublicKey>::random_keypair(&mut rng);
        let c3 = &c1.to_public_key().unwrap() + &c2.to_public_key().unwrap();
        let c3 = CompressedCommitment::<RistrettoPublicKey>::from_public_key(c3);
        let inputs = inputs!(
            CompressedCommitment::<RistrettoPublicKey>::from_compressed_key(c1),
            CompressedCommitment::<RistrettoPublicKey>::from_compressed_key(c2)
        );
        assert_eq!(script.execute(&inputs).unwrap(), Commitment(c3));
    }

    #[test]
    fn op_sub() {
        use crate::StackItem::Number;
        let script = script!(Add Sub).unwrap();
        let inputs = inputs!(5, 3, 2);
        assert_eq!(script.execute(&inputs).unwrap(), Number(0));
        let inputs = inputs!(i64::MAX, 1);
        assert_eq!(script.execute(&inputs), Err(ScriptError::ValueExceedsBounds));
        let script = script!(Sub).unwrap();
        let inputs = inputs!(5, 3);
        assert_eq!(script.execute(&inputs).unwrap(), Number(2));
    }

    #[test]
    fn serialisation() {
        let script = script!(Add Sub Add).unwrap();
        assert_eq!(&script.to_bytes(), &[0x93, 0x94, 0x93]);
        assert_eq!(TariScript::from_bytes(&[0x93, 0x94, 0x93]).unwrap(), script);
        assert_eq!(script.to_hex(), "939493");
        assert_eq!(TariScript::from_hex("939493").unwrap(), script);
    }

    #[test]
    fn check_sig() {
        use crate::StackItem::Number;
        let mut rng = rand::rng();
        let (pvt_key, pub_key) = CompressedKey::<RistrettoPublicKey>::random_keypair(&mut rng);
        let m_key = RistrettoSecretKey::random(&mut rng);
        let sig = CompressedCheckSigSchnorrSignature::new_from_schnorr(
            CheckSigSchnorrSignature::sign(&pvt_key, m_key.as_bytes(), &mut rng).unwrap(),
        );
        let msg = slice_to_boxed_message(m_key.as_bytes()).unwrap();
        let script = script!(CheckSig(msg)).unwrap();
        let inputs = inputs!(sig.clone(), pub_key.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));

        let n_key = RistrettoSecretKey::random(&mut rng);
        let msg = slice_to_boxed_message(n_key.as_bytes()).unwrap();
        let script = script!(CheckSig(msg)).unwrap();
        let inputs = inputs!(sig, pub_key);
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(0));
    }

    #[test]
    fn check_sig_verify() {
        use crate::StackItem::Number;
        let mut rng = rand::rng();
        let (pvt_key, pub_key) = CompressedKey::<RistrettoPublicKey>::random_keypair(&mut rng);
        let m_key = RistrettoSecretKey::random(&mut rng);
        let sig = CompressedCheckSigSchnorrSignature::new_from_schnorr(
            CheckSigSchnorrSignature::sign(&pvt_key, m_key.as_bytes(), &mut rng).unwrap(),
        );
        let msg = slice_to_boxed_message(m_key.as_bytes()).unwrap();
        let script = script!(CheckSigVerify(msg) PushOne).unwrap();
        let inputs = inputs!(sig.clone(), pub_key.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));

        let n_key = RistrettoSecretKey::random(&mut rng);
        let msg = slice_to_boxed_message(n_key.as_bytes()).unwrap();
        let script = script!(CheckSigVerify(msg)).unwrap();
        let inputs = inputs!(sig, pub_key);
        let err = script.execute(&inputs).unwrap_err();
        assert!(matches!(err, ScriptError::VerifyFailed));
    }

    #[allow(clippy::type_complexity)]
    fn multisig_data(
        n: usize,
    ) -> (
        Box<Message>,
        Vec<(
            RistrettoSecretKey,
            CompressedKey<RistrettoPublicKey>,
            CompressedCheckSigSchnorrSignature,
        )>,
    ) {
        let mut rng = rand::rng();
        let mut data = Vec::with_capacity(n);
        let m = RistrettoSecretKey::random(&mut rng);
        let msg = slice_to_boxed_message(m.as_bytes()).unwrap();

        for _ in 0..n {
            let (k, p) = CompressedKey::<RistrettoPublicKey>::random_keypair(&mut rng);
            let s = CompressedCheckSigSchnorrSignature::new_from_schnorr(
                CheckSigSchnorrSignature::sign(&k, m.as_bytes(), &mut rng).unwrap(),
            );
            data.push((k, p, s));
        }

        (msg, data)
    }

    #[allow(clippy::too_many_lines)]
    #[test]
    fn check_multisig() {
        use crate::{StackItem::Number, op_codes::Opcode::CheckMultiSig};
        let mut rng = rand::rng();
        let (k_alice, p_alice) = CompressedKey::<RistrettoPublicKey>::random_keypair(&mut rng);
        let (k_bob, p_bob) = CompressedKey::<RistrettoPublicKey>::random_keypair(&mut rng);
        let (k_eve, _) = CompressedKey::<RistrettoPublicKey>::random_keypair(&mut rng);
        let (k_carol, p_carol) = CompressedKey::<RistrettoPublicKey>::random_keypair(&mut rng);
        let m = RistrettoSecretKey::random(&mut rng);
        let s_alice = CompressedCheckSigSchnorrSignature::new_from_schnorr(
            CheckSigSchnorrSignature::sign(&k_alice, m.as_bytes(), &mut rng).unwrap(),
        );
        let s_bob = CompressedCheckSigSchnorrSignature::new_from_schnorr(
            CheckSigSchnorrSignature::sign(&k_bob, m.as_bytes(), &mut rng).unwrap(),
        );
        let s_eve = CompressedCheckSigSchnorrSignature::new_from_schnorr(
            CheckSigSchnorrSignature::sign(&k_eve, m.as_bytes(), &mut rng).unwrap(),
        );
        let s_carol = CompressedCheckSigSchnorrSignature::new_from_schnorr(
            CheckSigSchnorrSignature::sign(&k_carol, m.as_bytes(), &mut rng).unwrap(),
        );
        let s_alice2 = CompressedCheckSigSchnorrSignature::new_from_schnorr(
            CheckSigSchnorrSignature::sign(&k_alice, m.as_bytes(), &mut rng).unwrap(),
        );
        let msg = slice_to_boxed_message(m.as_bytes()).unwrap();

        // 1 of 2
        let keys = vec![p_alice.clone(), p_bob.clone()];
        let ops = vec![CheckMultiSig(1, 2, keys, msg.clone())];
        let script = TariScript::new(ops).unwrap();

        let inputs = inputs!(s_alice.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));
        let inputs = inputs!(s_bob.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));
        let inputs = inputs!(s_eve.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(0));

        // 2 of 2
        let keys = vec![p_alice.clone(), p_bob.clone()];
        let ops = vec![CheckMultiSig(2, 2, keys, msg.clone())];
        let script = TariScript::new(ops).unwrap();

        let inputs = inputs!(s_alice.clone(), s_bob.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));
        let inputs = inputs!(s_bob.clone(), s_alice.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(0));
        let inputs = inputs!(s_eve.clone(), s_bob.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(0));

        // 2 of 2 - don't allow same sig to sign twice
        let inputs = inputs!(s_alice.clone(), s_alice.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(0));

        // 1 of 3
        let keys = vec![p_alice.clone(), p_bob.clone(), p_carol.clone()];
        let ops = vec![CheckMultiSig(1, 3, keys, msg.clone())];
        let script = TariScript::new(ops).unwrap();

        let inputs = inputs!(s_alice.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));
        let inputs = inputs!(s_bob.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));
        let inputs = inputs!(s_carol.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));
        let inputs = inputs!(s_eve.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(0));

        // 2 of 3
        let keys = vec![p_alice.clone(), p_bob.clone(), p_carol.clone()];
        let ops = vec![CheckMultiSig(2, 3, keys, msg.clone())];
        let script = TariScript::new(ops).unwrap();

        let inputs = inputs!(s_alice.clone(), s_bob.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));
        let inputs = inputs!(s_alice.clone(), s_carol.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));
        let inputs = inputs!(s_bob.clone(), s_carol.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));
        let inputs = inputs!(s_carol.clone(), s_bob.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(0));
        let inputs = inputs!(s_carol.clone(), s_eve.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(0));

        // check that sigs are only counted once
        let keys = vec![p_alice.clone(), p_bob.clone(), p_alice.clone()];
        let ops = vec![CheckMultiSig(2, 3, keys, msg.clone())];
        let script = TariScript::new(ops).unwrap();

        let inputs = inputs!(s_alice.clone(), s_carol.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(0));
        let inputs = inputs!(s_alice.clone(), s_alice.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(0));
        let inputs = inputs!(s_alice.clone(), s_alice2.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));
        // Interesting case where either sig could match either pubkey
        let inputs = inputs!(s_alice2, s_alice.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));

        // 3 of 3
        let keys = vec![p_alice.clone(), p_bob.clone(), p_carol];
        let ops = vec![CheckMultiSig(3, 3, keys, msg.clone())];
        let script = TariScript::new(ops).unwrap();

        let inputs = inputs!(s_alice.clone(), s_bob.clone(), s_carol.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));
        let inputs = inputs!(s_carol.clone(), s_alice.clone(), s_bob.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(0));
        let inputs = inputs!(s_eve.clone(), s_bob.clone(), s_carol);
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(0));
        let inputs = inputs!(s_eve, s_bob);
        let err = script.execute(&inputs).unwrap_err();
        assert_eq!(err, ScriptError::StackUnderflow);

        // errors
        let keys = vec![p_alice.clone(), p_bob.clone()];
        let ops = vec![CheckMultiSig(0, 2, keys, msg.clone())];
        let script = TariScript::new(ops).unwrap();
        let inputs = inputs!(s_alice.clone());
        let err = script.execute(&inputs).unwrap_err();
        assert_eq!(err, ScriptError::ValueExceedsBounds);

        // An `n` that does not match the number of keys cannot be constructed
        let keys = vec![p_alice.clone(), p_bob.clone()];
        let ops = vec![CheckMultiSig(1, 0, keys, msg.clone())];
        assert_eq!(TariScript::new(ops), Err(ScriptError::InvalidData));
        let keys = vec![p_alice.clone(), p_bob.clone()];
        let ops = vec![CheckMultiSig(2, 1, keys, msg.clone())];
        assert_eq!(TariScript::new(ops), Err(ScriptError::InvalidData));

        let ops = vec![CheckMultiSig(1, 0, vec![], msg.clone())];
        let script = TariScript::new(ops).unwrap();
        let inputs = inputs!(s_alice.clone());
        let err = script.execute(&inputs).unwrap_err();
        assert_eq!(err, ScriptError::ValueExceedsBounds);

        let keys = vec![p_alice];
        let ops = vec![CheckMultiSig(2, 1, keys, msg)];
        let script = TariScript::new(ops).unwrap();
        let inputs = inputs!(s_alice);
        let err = script.execute(&inputs).unwrap_err();
        assert_eq!(err, ScriptError::ValueExceedsBounds);

        // max n is 32
        let (msg, data) = multisig_data(33);
        let keys = data.iter().map(|(_, p, _)| p.clone()).collect();
        let sigs = data.iter().take(17).map(|(_, _, s)| s.clone());
        let script = script!(CheckMultiSig(17, 33, keys, msg)).unwrap();
        let items = sigs.map(StackItem::Signature).collect();
        let inputs = ExecutionStack::new(items);
        let err = script.execute(&inputs).unwrap_err();
        assert_eq!(err, ScriptError::ValueExceedsBounds);

        // 3 of 4
        let (msg, data) = multisig_data(4);
        let keys = vec![
            data[0].1.clone(),
            data[1].1.clone(),
            data[2].1.clone(),
            data[3].1.clone(),
        ];
        let ops = vec![CheckMultiSig(3, 4, keys, msg)];
        let script = TariScript::new(ops).unwrap();
        let inputs = inputs!(data[0].2.clone(), data[1].2.clone(), data[2].2.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));

        // 5 of 7
        let (msg, data) = multisig_data(7);
        let keys = vec![
            data[0].1.clone(),
            data[1].1.clone(),
            data[2].1.clone(),
            data[3].1.clone(),
            data[4].1.clone(),
            data[5].1.clone(),
            data[6].1.clone(),
        ];
        let ops = vec![CheckMultiSig(5, 7, keys, msg)];
        let script = TariScript::new(ops).unwrap();
        let inputs = inputs!(
            data[0].2.clone(),
            data[1].2.clone(),
            data[2].2.clone(),
            data[3].2.clone(),
            data[4].2.clone()
        );
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));
    }

    #[test]
    fn check_multisig_semantics_pinned() {
        use crate::{StackItem::Number, op_codes::Opcode::CheckMultiSig};
        let mut rng = rand::rng();
        let (msg, data) = multisig_data(3);
        let (p_alice, s_alice) = (data[0].1.clone(), data[0].2.clone());
        let (p_bob, s_bob) = (data[1].1.clone(), data[1].2.clone());
        let (p_carol, s_carol) = (data[2].1.clone(), data[2].2.clone());
        let s_alice2 = CompressedCheckSigSchnorrSignature::new_from_schnorr(
            CheckSigSchnorrSignature::sign(&data[0].0, msg.as_slice(), &mut rng).unwrap(),
        );
        let (_, s_eve) = {
            let (msg_eve, data_eve) = multisig_data(1);
            (msg_eve, data_eve[0].2.clone())
        };
        // A key that is not a valid point, and a signature whose nonce is not a valid point
        let bad_key = CompressedKey::<RistrettoPublicKey>::new(&[0xff; 32]);
        let bad_sig = CompressedCheckSigSchnorrSignature::new(bad_key.clone(), s_alice.get_signature().clone());

        let run = |m: u8, keys: Vec<CompressedKey<RistrettoPublicKey>>, inputs: ExecutionStack| {
            let n = u8::try_from(keys.len()).unwrap();
            TariScript::new(vec![CheckMultiSig(m, n, keys, msg.clone())])
                .unwrap()
                .execute(&inputs)
        };

        // Duplicate keys: each occurrence may be used once, by distinct signatures only
        let keys = vec![p_alice.clone(), p_alice.clone()];
        assert_eq!(
            run(2, keys.clone(), inputs!(s_alice.clone(), s_alice2.clone())),
            Ok(Number(1))
        );
        assert_eq!(
            run(2, keys.clone(), inputs!(s_alice.clone(), s_alice.clone())),
            Ok(Number(0))
        );
        assert_eq!(run(2, keys, inputs!(s_alice.clone(), s_bob.clone())), Ok(Number(0)));

        // Out-of-order signatures fail, since keys are only scanned forward
        let keys = vec![p_alice.clone(), p_bob.clone(), p_carol.clone()];
        assert_eq!(
            run(2, keys.clone(), inputs!(s_alice.clone(), s_carol.clone())),
            Ok(Number(1))
        );
        assert_eq!(
            run(2, keys.clone(), inputs!(s_carol.clone(), s_alice.clone())),
            Ok(Number(0))
        );
        assert_eq!(
            run(
                3,
                keys.clone(),
                inputs!(s_bob.clone(), s_alice.clone(), s_carol.clone())
            ),
            Ok(Number(0))
        );
        // A non-matching signature exhausts the remaining keys
        assert_eq!(run(2, keys, inputs!(s_eve.clone(), s_alice.clone())), Ok(Number(0)));

        // An invalid key is only an error if the scan actually reaches it
        let keys = vec![p_alice.clone(), bad_key.clone()];
        assert_eq!(run(1, keys.clone(), inputs!(s_alice.clone())), Ok(Number(1)));
        assert_eq!(run(1, keys, inputs!(s_eve.clone())), Err(ScriptError::InvalidInput));
        let keys = vec![p_alice.clone(), p_bob.clone(), bad_key.clone()];
        assert_eq!(run(2, keys, inputs!(s_alice.clone(), s_bob.clone())), Ok(Number(1)));

        // An invalid key reached before an invalid signature is reported first
        let keys = vec![bad_key, p_alice.clone()];
        assert_eq!(
            run(1, keys, inputs!(StackItem::Signature(bad_sig.clone()))),
            Err(ScriptError::InvalidInput)
        );
        let keys = vec![p_alice, p_bob];
        assert_eq!(
            run(1, keys.clone(), inputs!(StackItem::Signature(bad_sig.clone()))),
            Err(ScriptError::InvalidData)
        );
        // ... but an invalid signature that is never compared to a key is not an error
        assert_eq!(
            run(2, keys, inputs!(s_eve, StackItem::Signature(bad_sig))),
            Ok(Number(0))
        );
    }

    #[test]
    fn from_bytes_rejects_oversized_scripts() {
        // A script of exactly MAX_SCRIPT_BYTES of Nop (0x73) is parsed, and then rejected for having too many opcodes
        let err = TariScript::from_bytes(&[0x73; MAX_SCRIPT_BYTES]).unwrap_err();
        assert!(matches!(err, ScriptError::MaxSizeVecError(_)));
        // One more byte is rejected before parsing, even though the bytes are not valid opcodes
        let err = TariScript::from_bytes(&[0xFF; MAX_SCRIPT_BYTES + 1]).unwrap_err();
        assert_eq!(err, ScriptError::ScriptTooLarge {
            max: MAX_SCRIPT_BYTES,
            actual: MAX_SCRIPT_BYTES + 1
        });
        let err = TariScript::from_hex(&to_hex(&[0xFF; MAX_SCRIPT_BYTES + 1]));
        assert!(err.is_err());
    }

    #[allow(clippy::too_many_lines)]
    #[test]
    fn check_multisig_verify() {
        use crate::{StackItem::Number, op_codes::Opcode::CheckMultiSigVerify};
        let mut rng = rand::rng();
        let (k_alice, p_alice) = CompressedKey::<RistrettoPublicKey>::random_keypair(&mut rng);
        let (k_bob, p_bob) = CompressedKey::<RistrettoPublicKey>::random_keypair(&mut rng);
        let (k_eve, _) = CompressedKey::<RistrettoPublicKey>::random_keypair(&mut rng);
        let (k_carol, p_carol) = CompressedKey::<RistrettoPublicKey>::random_keypair(&mut rng);
        let m = RistrettoSecretKey::random(&mut rng);
        let s_alice = CompressedCheckSigSchnorrSignature::new_from_schnorr(
            CheckSigSchnorrSignature::sign(&k_alice, m.as_bytes(), &mut rng).unwrap(),
        );
        let s_bob = CompressedCheckSigSchnorrSignature::new_from_schnorr(
            CheckSigSchnorrSignature::sign(&k_bob, m.as_bytes(), &mut rng).unwrap(),
        );
        let s_eve = CompressedCheckSigSchnorrSignature::new_from_schnorr(
            CheckSigSchnorrSignature::sign(&k_eve, m.as_bytes(), &mut rng).unwrap(),
        );
        let s_carol = CompressedCheckSigSchnorrSignature::new_from_schnorr(
            CheckSigSchnorrSignature::sign(&k_carol, m.as_bytes(), &mut rng).unwrap(),
        );
        let msg = slice_to_boxed_message(m.as_bytes()).unwrap();

        // 1 of 2
        let keys = vec![p_alice.clone(), p_bob.clone()];
        let ops = vec![CheckMultiSigVerify(1, 2, keys, msg.clone())];
        let script = TariScript::new(ops).unwrap();

        let inputs = inputs!(Number(1), s_alice.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));
        let inputs = inputs!(Number(1), s_bob.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));
        let inputs = inputs!(Number(1), s_eve.clone());
        let err = script.execute(&inputs).unwrap_err();
        assert_eq!(err, ScriptError::VerifyFailed);

        // 2 of 2
        let keys = vec![p_alice.clone(), p_bob.clone()];
        let ops = vec![CheckMultiSigVerify(2, 2, keys, msg.clone())];
        let script = TariScript::new(ops).unwrap();

        let inputs = inputs!(Number(1), s_alice.clone(), s_bob.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));
        let inputs = inputs!(Number(1), s_bob.clone(), s_alice.clone());
        let err = script.execute(&inputs).unwrap_err();
        assert_eq!(err, ScriptError::VerifyFailed);
        let inputs = inputs!(Number(1), s_eve.clone(), s_bob.clone());
        let err = script.execute(&inputs).unwrap_err();
        assert_eq!(err, ScriptError::VerifyFailed);

        // 1 of 3
        let keys = vec![p_alice.clone(), p_bob.clone(), p_carol.clone()];
        let ops = vec![CheckMultiSigVerify(1, 3, keys, msg.clone())];
        let script = TariScript::new(ops).unwrap();

        let inputs = inputs!(Number(1), s_alice.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));
        let inputs = inputs!(Number(1), s_bob.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));
        let inputs = inputs!(Number(1), s_carol.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));
        let inputs = inputs!(Number(1), s_eve.clone());
        let err = script.execute(&inputs).unwrap_err();
        assert_eq!(err, ScriptError::VerifyFailed);

        // 2 of 3
        let keys = vec![p_alice.clone(), p_bob.clone(), p_carol.clone()];
        let ops = vec![CheckMultiSigVerify(2, 3, keys, msg.clone())];
        let script = TariScript::new(ops).unwrap();

        let inputs = inputs!(Number(1), s_alice.clone(), s_bob.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));
        let inputs = inputs!(Number(1), s_alice.clone(), s_carol.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));
        let inputs = inputs!(Number(1), s_bob.clone(), s_carol.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));
        let inputs = inputs!(Number(1), s_carol.clone(), s_bob.clone());
        let err = script.execute(&inputs).unwrap_err();
        assert_eq!(err, ScriptError::VerifyFailed);
        let inputs = inputs!(Number(1), s_carol.clone(), s_eve.clone());
        let err = script.execute(&inputs).unwrap_err();
        assert_eq!(err, ScriptError::VerifyFailed);

        // 2 of 3 (returning the aggregate public key of the signatories)
        let keys = vec![p_alice.clone(), p_bob.clone(), p_carol.clone()];
        let ops = vec![CheckMultiSigVerifyAggregatePubKey(2, 3, keys, msg.clone())];
        let script = TariScript::new(ops).unwrap();

        let inputs = inputs!(s_alice.clone(), s_bob.clone());
        let agg_pub_key = script.execute(&inputs).unwrap();
        assert_eq!(
            agg_pub_key,
            StackItem::PublicKey(CompressedKey::<RistrettoPublicKey>::new_from_pk(
                &p_alice.clone().to_public_key().unwrap() + &p_bob.clone().to_public_key().unwrap()
            ))
        );

        let inputs = inputs!(s_alice.clone(), s_carol.clone());
        let agg_pub_key = script.execute(&inputs).unwrap();
        assert_eq!(
            agg_pub_key,
            StackItem::PublicKey(CompressedKey::<RistrettoPublicKey>::new_from_pk(
                &p_alice.clone().to_public_key().unwrap() + &p_carol.clone().to_public_key().unwrap()
            ))
        );

        let inputs = inputs!(s_bob.clone(), s_carol.clone());
        let agg_pub_key = script.execute(&inputs).unwrap();
        assert_eq!(
            agg_pub_key,
            StackItem::PublicKey(CompressedKey::<RistrettoPublicKey>::new_from_pk(
                &p_bob.clone().to_public_key().unwrap() + &p_carol.clone().to_public_key().unwrap()
            ))
        );

        let inputs = inputs!(s_carol.clone(), s_bob.clone());
        let err = script.execute(&inputs).unwrap_err();
        assert_eq!(err, ScriptError::VerifyFailed);

        let inputs = inputs!(s_alice.clone(), s_bob.clone(), s_carol.clone());
        let err = script.execute(&inputs).unwrap_err();
        assert_eq!(err, ScriptError::NonUnitLengthStack);

        let inputs = inputs!(p_bob.clone());
        let err = script.execute(&inputs).unwrap_err();
        assert_eq!(err, ScriptError::StackUnderflow);

        // 3 of 3
        let keys = vec![p_alice.clone(), p_bob.clone(), p_carol];
        let ops = vec![CheckMultiSigVerify(3, 3, keys, msg.clone())];
        let script = TariScript::new(ops).unwrap();

        let inputs = inputs!(Number(1), s_alice.clone(), s_bob.clone(), s_carol.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));
        let inputs = inputs!(Number(1), s_bob.clone(), s_alice.clone(), s_carol.clone());
        let err = script.execute(&inputs).unwrap_err();
        assert_eq!(err, ScriptError::VerifyFailed);
        let inputs = inputs!(Number(1), s_eve.clone(), s_bob.clone(), s_carol);
        let err = script.execute(&inputs).unwrap_err();
        assert_eq!(err, ScriptError::VerifyFailed);
        let inputs = inputs!(Number(1), s_eve, s_bob);
        let err = script.execute(&inputs).unwrap_err();
        assert_eq!(err, ScriptError::IncompatibleTypes);

        // errors
        let keys = vec![p_alice.clone(), p_bob.clone()];
        let ops = vec![CheckMultiSigVerify(0, 2, keys, msg.clone())];
        let script = TariScript::new(ops).unwrap();
        let inputs = inputs!(s_alice.clone());
        let err = script.execute(&inputs).unwrap_err();
        assert_eq!(err, ScriptError::ValueExceedsBounds);

        // An `n` that does not match the number of keys cannot be constructed
        let keys = vec![p_alice.clone(), p_bob.clone()];
        let ops = vec![CheckMultiSigVerify(1, 0, keys, msg.clone())];
        assert_eq!(TariScript::new(ops), Err(ScriptError::InvalidData));
        let keys = vec![p_alice.clone(), p_bob.clone()];
        let ops = vec![CheckMultiSigVerify(2, 1, keys, msg.clone())];
        assert_eq!(TariScript::new(ops), Err(ScriptError::InvalidData));

        let ops = vec![CheckMultiSigVerify(1, 0, vec![], msg.clone())];
        let script = TariScript::new(ops).unwrap();
        let inputs = inputs!(s_alice.clone());
        let err = script.execute(&inputs).unwrap_err();
        assert_eq!(err, ScriptError::ValueExceedsBounds);

        let keys = vec![p_alice];
        let ops = vec![CheckMultiSigVerify(2, 1, keys, msg)];
        let script = TariScript::new(ops).unwrap();
        let inputs = inputs!(s_alice);
        let err = script.execute(&inputs).unwrap_err();
        assert_eq!(err, ScriptError::ValueExceedsBounds);

        // 3 of 4
        let (msg, data) = multisig_data(4);
        let keys = vec![
            data[0].1.clone(),
            data[1].1.clone(),
            data[2].1.clone(),
            data[3].1.clone(),
        ];
        let ops = vec![CheckMultiSigVerify(3, 4, keys, msg)];
        let script = TariScript::new(ops).unwrap();
        let inputs = inputs!(Number(1), data[0].2.clone(), data[1].2.clone(), data[2].2.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));

        // 5 of 7
        let (msg, data) = multisig_data(7);
        let keys = vec![
            data[0].1.clone(),
            data[1].1.clone(),
            data[2].1.clone(),
            data[3].1.clone(),
            data[4].1.clone(),
            data[5].1.clone(),
            data[6].1.clone(),
        ];
        let ops = vec![CheckMultiSigVerify(5, 7, keys, msg)];
        let script = TariScript::new(ops).unwrap();
        let inputs = inputs!(
            Number(1),
            data[0].2.clone(),
            data[1].2.clone(),
            data[2].2.clone(),
            data[3].2.clone(),
            data[4].2.clone()
        );
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));
    }

    #[test]
    fn pay_to_public_key_hash() {
        use crate::StackItem::PublicKey;
        let k =
            RistrettoSecretKey::from_hex("7212ac93ee205cdbbb57c4f0f815fbf8db25b4d04d3532e2262e31907d82c700").unwrap();
        let p = CompressedKey::<RistrettoPublicKey>::from_secret_key(&k); // 56c0fa32558d6edc0916baa26b48e745de834571534ca253ea82435f08ebbc7c
        let hash = Blake2b::<U32>::digest(p.as_bytes());
        let pkh = slice_to_boxed_hash(hash.as_slice()).unwrap(); // ae2337ce44f9ebb6169c863ec168046cb35ab4ef7aa9ed4f5f1f669bb74b09e5

        // Unlike in Bitcoin where P2PKH includes a CheckSig at the end of the script, that part of the process is built
        // into definition of how TariScript is evaluated by a base node or wallet
        let script = script!(Dup HashBlake256 PushHash(pkh) EqualVerify).unwrap();
        let hex_script = "71b07aae2337ce44f9ebb6169c863ec168046cb35ab4ef7aa9ed4f5f1f669bb74b09e581";
        // Test serialisation
        assert_eq!(script.to_hex(), hex_script);
        // Test de-serialisation
        assert_eq!(TariScript::from_hex(hex_script).unwrap(), script);

        let inputs = inputs!(p.clone());

        let result = script.execute(&inputs).unwrap();

        assert_eq!(result, PublicKey(p));
    }

    #[test]
    fn hex_roundtrip() {
        // Generate a signature
        let mut rng = rand::rng();
        let (secret_key, public_key) = CompressedKey::<RistrettoPublicKey>::random_keypair(&mut rng);
        let message = [1u8; 32];
        let sig = CompressedCheckSigSchnorrSignature::new_from_schnorr(
            CheckSigSchnorrSignature::sign(&secret_key, message, &mut rng).unwrap(),
        );

        // Produce a script using the signature
        let script = script!(CheckSig(slice_to_boxed_message(message.as_bytes()).unwrap())).unwrap();

        // Produce input satisfying the script
        let input = inputs!(sig, public_key);

        // Check that script execution succeeds
        assert_eq!(script.execute(&input).unwrap(), StackItem::Number(1));

        // Convert the script to hex and back
        let parsed_script = TariScript::from_hex(script.to_hex().as_str()).unwrap();
        assert_eq!(script.to_opcodes(), parsed_script.to_opcodes());

        // Convert the input to hex and back
        let parsed_input = ExecutionStack::from_hex(input.to_hex().as_str()).unwrap();
        assert_eq!(input, parsed_input);

        // Check that script execution still succeeds
        assert_eq!(parsed_script.execute(&parsed_input).unwrap(), StackItem::Number(1));
    }

    #[test]
    fn disassemble() {
        let hex_script = "71b07aae2337ce44f9ebb6169c863ec168046cb35ab4ef7aa9ed4f5f1f669bb74b09e58170ac276657a418820f34036b20ea615302b373c70ac8feab8d30681a3e0f0960e708";
        let script = TariScript::from_hex(hex_script).unwrap();
        let ops = vec![
            "Dup",
            "HashBlake256",
            "PushHash(ae2337ce44f9ebb6169c863ec168046cb35ab4ef7aa9ed4f5f1f669bb74b09e5)",
            "EqualVerify",
            "Drop",
            "CheckSig(276657a418820f34036b20ea615302b373c70ac8feab8d30681a3e0f0960e708)",
        ]
        .into_iter()
        .map(String::from)
        .collect::<Vec<String>>();
        assert_eq!(script.to_opcodes(), ops);
        assert_eq!(
            script.to_string(),
            "Dup HashBlake256 PushHash(ae2337ce44f9ebb6169c863ec168046cb35ab4ef7aa9ed4f5f1f669bb74b09e5) EqualVerify \
             Drop CheckSig(276657a418820f34036b20ea615302b373c70ac8feab8d30681a3e0f0960e708)"
        );
    }

    #[test]
    fn time_locked_contract_example() {
        let k_alice =
            RistrettoSecretKey::from_hex("f305e64c0e73cbdb665165ac97b69e5df37b2cd81f9f8f569c3bd854daff290e").unwrap();
        let p_alice = CompressedKey::<RistrettoPublicKey>::from_secret_key(&k_alice); // 9c35e9f0f11cf25ce3ca1182d37682ab5824aa033f2024651e007364d06ec355

        let k_bob =
            RistrettoSecretKey::from_hex("e0689386a018e88993a7bb14cbff5bad8a8858ea101d6e0da047df3ddf499c0e").unwrap();
        let p_bob = CompressedKey::<RistrettoPublicKey>::from_secret_key(&k_bob); // 3a58f371e94da76a8902e81b4b55ddabb7dc006cd8ebde3011c46d0e02e9172f

        let lock_height = 4000u64;

        let script = script!(Dup PushPubKey(Box::new(p_bob.clone())) CheckHeight(lock_height) GeZero IfThen PushPubKey(Box::new(p_alice.clone())) OrVerify(2) Else EqualVerify EndIf ).unwrap();

        // Alice tries to spend the output before the height is reached
        let inputs_alice_spends_early = inputs!(p_alice.clone());
        let ctx = context_with_height(3990u64);
        assert_eq!(
            script.execute_with_context(&inputs_alice_spends_early, &ctx),
            Err(ScriptError::VerifyFailed)
        );

        // Alice tries to spend the output after the height is reached
        let inputs_alice_spends_early = inputs!(p_alice.clone());
        let ctx = context_with_height(4000u64);
        assert_eq!(
            script.execute_with_context(&inputs_alice_spends_early, &ctx).unwrap(),
            StackItem::PublicKey(p_alice)
        );

        // Bob spends before time lock is reached
        let inputs_bob_spends_early = inputs!(p_bob.clone());
        let ctx = context_with_height(3990u64);
        assert_eq!(
            script.execute_with_context(&inputs_bob_spends_early, &ctx).unwrap(),
            StackItem::PublicKey(p_bob.clone())
        );

        // Bob spends after time lock is reached
        let inputs_bob_spends_early = inputs!(p_bob.clone());
        let ctx = context_with_height(4001u64);
        assert_eq!(
            script.execute_with_context(&inputs_bob_spends_early, &ctx).unwrap(),
            StackItem::PublicKey(p_bob)
        );
    }

    #[test]
    fn m_of_n_signatures() {
        use crate::StackItem::PublicKey;
        let mut rng = rand::rng();
        let (k_alice, p_alice) = CompressedKey::<RistrettoPublicKey>::random_keypair(&mut rng);
        let (k_bob, p_bob) = CompressedKey::<RistrettoPublicKey>::random_keypair(&mut rng);
        let (k_eve, _) = CompressedKey::<RistrettoPublicKey>::random_keypair(&mut rng);

        let m = RistrettoSecretKey::random(&mut rng);
        let msg = slice_to_boxed_message(m.as_bytes()).unwrap();

        let s_alice = CompressedCheckSigSchnorrSignature::new_from_schnorr(
            CheckSigSchnorrSignature::sign(&k_alice, m.as_bytes(), &mut rng).unwrap(),
        );
        let s_bob = CompressedCheckSigSchnorrSignature::new_from_schnorr(
            CheckSigSchnorrSignature::sign(&k_bob, m.as_bytes(), &mut rng).unwrap(),
        );
        let s_eve = CompressedCheckSigSchnorrSignature::new_from_schnorr(
            CheckSigSchnorrSignature::sign(&k_eve, m.as_bytes(), &mut rng).unwrap(),
        );

        // 1 of 2
        use crate::Opcode::{CheckSig, Drop, Dup, Else, EndIf, IfThen, PushPubKey, Return};
        let ops = vec![
            Dup,
            PushPubKey(Box::new(p_alice.clone())),
            CheckSig(msg.clone()),
            IfThen,
            Drop,
            PushPubKey(Box::new(p_alice.clone())),
            Else,
            PushPubKey(Box::new(p_bob.clone())),
            CheckSig(msg),
            IfThen,
            PushPubKey(Box::new(p_bob.clone())),
            Else,
            Return,
            EndIf,
            EndIf,
        ];
        let script = TariScript::new(ops).unwrap();

        // alice
        let inputs = inputs!(s_alice);
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, PublicKey(p_alice));

        // bob
        let inputs = inputs!(s_bob);
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, PublicKey(p_bob));

        // eve
        let inputs = inputs!(s_eve);
        let result = script.execute(&inputs).unwrap_err();
        assert_eq!(result, ScriptError::Return);
    }

    #[test]
    fn to_ristretto_point() {
        use crate::{Opcode::ToRistrettoPoint, StackItem::PublicKey};

        // Generate a key pair
        let mut rng = rand::rng();
        let (k_1, p_1) = CompressedKey::<RistrettoPublicKey>::random_keypair(&mut rng);

        // Generate a test script
        let ops = vec![ToRistrettoPoint];
        let script = TariScript::new(ops).unwrap();

        // Invalid stack type
        let inputs = inputs!(CompressedKey::<RistrettoPublicKey>::default());
        let err = script.execute(&inputs).unwrap_err();
        assert!(matches!(err, ScriptError::IncompatibleTypes));

        // Valid scalar
        let mut scalar = [0u8; 32];
        scalar.copy_from_slice(k_1.as_bytes());
        let inputs = inputs!(scalar);
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, PublicKey(p_1.clone()));

        // Valid hash
        let inputs = ExecutionStack::new(vec![Hash(scalar)]);
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, PublicKey(p_1));

        // Invalid bytes
        let invalid = [u8::MAX; 32]; // not a canonical scalar encoding!
        let inputs = inputs!(invalid);
        assert!(matches!(script.execute(&inputs), Err(ScriptError::InvalidInput)));
    }

    #[test]
    fn test_borsh_de_serialization() {
        let hex_script = "71b07aae2337ce44f9ebb6169c863ec168046cb35ab4ef7aa9ed4f5f1f669bb74b09e58170ac276657a418820f34036b20ea615302b373c70ac8feab8d30681a3e0f0960e708";
        let script = TariScript::from_hex(hex_script).unwrap();
        let mut buf = Vec::new();
        script.serialize(&mut buf).unwrap();
        buf.extend_from_slice(&[1, 2, 3]);
        let buf = &mut buf.as_slice();
        assert_eq!(script, TariScript::deserialize(buf).unwrap());
        assert_eq!(buf, &[1, 2, 3]);
    }

    #[test]
    fn test_borsh_de_serialization_too_large() {
        // We dont care about the actual script here, just that its not too large on the varint size
        // We lie about the size to try and get a mem panic, and say this script is u64::max large.
        let buf = vec![255, 255, 255, 255, 255, 255, 255, 255, 255, 1, 49, 8, 2, 5, 6];
        let buf = &mut buf.as_slice();
        assert!(TariScript::deserialize(buf).is_err());
    }

    #[test]
    fn test_compare_height_block_height_exceeds_bounds() {
        let script = script!(CompareHeight).unwrap();

        let inputs = inputs!(0);
        let ctx = context_with_height(u64::MAX);
        let stack_item = script.execute_with_context(&inputs, &ctx);
        assert!(matches!(stack_item, Err(ScriptError::ValueExceedsBounds)));
    }

    #[test]
    fn test_compare_height_underflows() {
        let script = script!(CompareHeight).unwrap();

        let inputs = ExecutionStack::new(vec![Number(i64::MIN)]);
        let ctx = context_with_height(i64::MAX as u64);
        let stack_item = script.execute_with_context(&inputs, &ctx);
        assert!(matches!(stack_item, Err(ScriptError::CompareFailed(_))));
    }

    #[test]
    fn test_compare_height_underflows_on_empty_stack() {
        let script = script!(CompareHeight).unwrap();

        let inputs = ExecutionStack::new(vec![]);
        let ctx = context_with_height(i64::MAX as u64);
        let stack_item = script.execute_with_context(&inputs, &ctx);
        assert!(matches!(stack_item, Err(ScriptError::StackUnderflow)));
    }

    #[test]
    fn test_compare_height_valid_with_uint_result() {
        let script = script!(CompareHeight).unwrap();

        let inputs = inputs!(100);
        let ctx = context_with_height(24_u64);
        let stack_item = script.execute_with_context(&inputs, &ctx);
        assert!(stack_item.is_ok());
        assert_eq!(stack_item.unwrap(), Number(-76))
    }

    #[test]
    fn test_compare_height_valid_with_int_result() {
        let script = script!(CompareHeight).unwrap();

        let inputs = inputs!(100);
        let ctx = context_with_height(110_u64);
        let stack_item = script.execute_with_context(&inputs, &ctx);
        assert!(stack_item.is_ok());
        assert_eq!(stack_item.unwrap(), Number(10))
    }

    #[test]
    fn test_check_height_block_height_exceeds_bounds() {
        let script = script!(CheckHeight(0)).unwrap();

        let inputs = ExecutionStack::new(vec![]);
        let ctx = context_with_height(u64::MAX);
        let stack_item = script.execute_with_context(&inputs, &ctx);
        assert!(matches!(stack_item, Err(ScriptError::ValueExceedsBounds)));
    }

    #[test]
    fn test_check_height_exceeds_bounds() {
        let script = script!(CheckHeight(u64::MAX)).unwrap();

        let inputs = ExecutionStack::new(vec![]);
        let ctx = context_with_height(10_u64);
        let stack_item = script.execute_with_context(&inputs, &ctx);
        assert!(matches!(stack_item, Err(ScriptError::ValueExceedsBounds)));
    }

    #[test]
    fn test_check_height_overflows_on_max_stack() {
        let script = script!(CheckHeight(0)).unwrap();

        let mut inputs = ExecutionStack::new(vec![]);

        for i in 0..255 {
            inputs.push(Number(i)).unwrap();
        }

        let ctx = context_with_height(i64::MAX as u64);
        let stack_item = script.execute_with_context(&inputs, &ctx);
        assert!(matches!(stack_item, Err(ScriptError::StackOverflow)));
    }

    #[test]
    fn test_check_height_valid_with_uint_result() {
        let script = script!(CheckHeight(24)).unwrap();

        let inputs = ExecutionStack::new(vec![]);
        let ctx = context_with_height(100_u64);
        let stack_item = script.execute_with_context(&inputs, &ctx);
        assert!(stack_item.is_ok());
        assert_eq!(stack_item.unwrap(), Number(76))
    }

    #[test]
    fn test_check_height_valid_with_int_result() {
        let script = script!(CheckHeight(100)).unwrap();

        let inputs = ExecutionStack::new(vec![]);
        let ctx = context_with_height(24_u64);
        let stack_item = script.execute_with_context(&inputs, &ctx);
        assert!(stack_item.is_ok());
        assert_eq!(stack_item.unwrap(), Number(-76))
    }

    // ---- Pinned behaviour: these tests assert the current engine semantics, which are part of consensus ----

    #[test]
    fn rev_rot_pinned() {
        // Bottom-first, `c` on top: [a, b, c] => [c, a, b]
        let mut stack = inputs!(1, 2, 3);
        stack.push_down(2).unwrap();
        assert_eq!(stack, inputs!(3, 1, 2));
        // Deeper items are untouched
        let mut stack = inputs!(9, 1, 2, 3);
        stack.push_down(2).unwrap();
        assert_eq!(stack, inputs!(9, 3, 1, 2));

        // Through the engine: the old top ends up at the bottom of the three
        let script = script!(RevRot Drop Drop).unwrap();
        assert_eq!(script.execute(&inputs!(1, 2, 3)), Ok(Number(3)));
        // [1, 2, 3] => [3, 1, 2]; Sub computes 1 - 2 = -1; Add computes 3 + -1 = 2
        let script = script!(RevRot Sub Add).unwrap();
        assert_eq!(script.execute(&inputs!(1, 2, 3)), Ok(Number(2)));

        // Underflow with two or fewer items
        let script = script!(RevRot).unwrap();
        assert_eq!(script.execute(&inputs!(1, 2)), Err(ScriptError::StackUnderflow));
        assert_eq!(script.execute(&inputs!(1)), Err(ScriptError::StackUnderflow));
        assert_eq!(
            script.execute(&ExecutionStack::default()),
            Err(ScriptError::StackUnderflow)
        );
    }

    #[test]
    fn equal_mixed_types_is_incompatible_types_pinned() {
        let mut rng = rand::rng();
        let (_, p) = CompressedKey::<RistrettoPublicKey>::random_keypair(&mut rng);
        let h: HashValue = p.as_bytes().try_into().unwrap();
        let c = CompressedCommitment::<RistrettoPublicKey>::from_compressed_key(p.clone());

        for script in [script!(Equal).unwrap(), script!(EqualVerify PushOne).unwrap()] {
            // Mixed types abort the script; they do not push 0
            assert_eq!(
                script.execute(&inputs!(1, p.clone())),
                Err(ScriptError::IncompatibleTypes)
            );
            // Same bytes, different types, is still an abort
            assert_eq!(
                script.execute(&inputs!(Hash(h), p.clone())),
                Err(ScriptError::IncompatibleTypes)
            );
            assert_eq!(
                script.execute(&inputs!(c.clone(), p.clone())),
                Err(ScriptError::IncompatibleTypes)
            );
            assert_eq!(script.execute(&inputs!(1)), Err(ScriptError::StackUnderflow));
        }
        // Same types compare normally
        assert_eq!(
            script!(Equal).unwrap().execute(&inputs!(Hash(h), Hash(h))),
            Ok(Number(1))
        );
        assert_eq!(script!(Equal).unwrap().execute(&inputs!(1, 2)), Ok(Number(0)));
        assert_eq!(
            script!(EqualVerify PushOne).unwrap().execute(&inputs!(1, 2)),
            Err(ScriptError::VerifyFailed)
        );
    }

    #[test]
    fn equal_scalar_is_incompatible_types_pinned() {
        let scalar = StackItem::Scalar([7u8; 32]);
        let stack = ExecutionStack::new(vec![scalar.clone(), scalar.clone()]);
        // Scalars are not comparable, not even with an identical scalar
        assert_eq!(
            script!(Equal).unwrap().execute(&stack),
            Err(ScriptError::IncompatibleTypes)
        );
        assert_eq!(
            script!(EqualVerify PushOne).unwrap().execute(&stack),
            Err(ScriptError::IncompatibleTypes)
        );
        let stack = ExecutionStack::new(vec![Hash([7u8; 32]), scalar]);
        assert_eq!(
            script!(Equal).unwrap().execute(&stack),
            Err(ScriptError::IncompatibleTypes)
        );
    }

    #[test]
    fn compare_height_verify_negative_is_value_exceeds_bounds_pinned() {
        // CompareHeightVerify pops a u64: a negative value cannot be converted
        let script = script!(CompareHeightVerify PushOne).unwrap();
        let ctx = context_with_height(100);
        assert_eq!(
            script.execute_with_context(&inputs!(-1), &ctx),
            Err(ScriptError::ValueExceedsBounds)
        );
        assert_eq!(
            script.execute_with_context(&inputs!(i64::MIN), &ctx),
            Err(ScriptError::ValueExceedsBounds)
        );
        // A non-number is InvalidInput, and an empty stack is StackUnderflow
        assert_eq!(
            script.execute_with_context(&inputs!(Hash([0u8; 32])), &ctx),
            Err(ScriptError::InvalidInput)
        );
        assert_eq!(
            script.execute_with_context(&ExecutionStack::default(), &ctx),
            Err(ScriptError::StackUnderflow)
        );
        assert_eq!(script.execute_with_context(&inputs!(100), &ctx), Ok(Number(1)));
        assert_eq!(
            script.execute_with_context(&inputs!(101), &ctx),
            Err(ScriptError::VerifyFailed)
        );

        // CompareHeight pops an i64, so a negative value is accepted
        let script = script!(CompareHeight).unwrap();
        assert_eq!(script.execute_with_context(&inputs!(-1), &ctx), Ok(Number(101)));
    }

    #[test]
    fn hash_opcodes_hash_untagged_payload_pinned() {
        let mut rng = rand::rng();
        let (_, p) = CompressedKey::<RistrettoPublicKey>::random_keypair(&mut rng);
        let h: HashValue = p.as_bytes().try_into().unwrap();
        let c = CompressedCommitment::<RistrettoPublicKey>::from_compressed_key(p.clone());
        let (_, data) = multisig_data(1);
        let sig = data[0].2.clone();

        for (script, expected) in [
            (script!(HashBlake256).unwrap(), Blake2b::<U32>::digest(h).into()),
            (script!(HashSha256).unwrap(), Sha256::digest(h).into()),
            (script!(HashSha3).unwrap(), Sha3::digest(h).into()),
        ] {
            let expected: HashValue = expected;
            // Hash, PublicKey and Commitment with the same bytes produce the same digest
            assert_eq!(script.execute(&inputs!(Hash(h))), Ok(Hash(expected)));
            assert_eq!(script.execute(&inputs!(p.clone())), Ok(Hash(expected)));
            assert_eq!(script.execute(&inputs!(c.clone())), Ok(Hash(expected)));
            // Number, Scalar and Signature cannot be hashed
            assert_eq!(script.execute(&inputs!(1)), Err(ScriptError::IncompatibleTypes));
            assert_eq!(
                script.execute(&ExecutionStack::new(vec![StackItem::Scalar(h)])),
                Err(ScriptError::IncompatibleTypes)
            );
            assert_eq!(
                script.execute(&inputs!(sig.clone())),
                Err(ScriptError::IncompatibleTypes)
            );
        }
    }

    #[test]
    fn handle_hash_rejects_non_32_byte_digests() {
        let mut stack = inputs!(Hash([1u8; 32]));
        assert_eq!(
            TariScript::handle_hash::<sha2::Sha512>(&mut stack),
            Err(ScriptError::InvalidDigest)
        );
        // The stack is left untouched
        assert_eq!(stack, inputs!(Hash([1u8; 32])));
        let mut stack = inputs!(Hash([1u8; 32]));
        assert_eq!(
            TariScript::handle_hash::<Blake2b<digest::consts::U64>>(&mut stack),
            Err(ScriptError::InvalidDigest)
        );
        let mut stack = inputs!(Hash([1u8; 32]));
        assert!(TariScript::handle_hash::<Sha256>(&mut stack).is_ok());
    }

    #[test]
    fn new_rejects_multisig_n_mismatch() {
        use crate::Opcode::{CheckMultiSig, CheckMultiSigVerify};
        let (msg, data) = multisig_data(3);
        let keys: Vec<_> = data.iter().map(|d| d.1.clone()).collect();

        type Ctor = fn(u8, u8, Vec<CompressedKey<RistrettoPublicKey>>, Box<Message>) -> crate::Opcode;
        let ctors: [Ctor; 3] = [CheckMultiSig, CheckMultiSigVerify, CheckMultiSigVerifyAggregatePubKey];
        for ctor in ctors {
            // n larger or smaller than the number of keys is rejected
            for n in [0u8, 1, 2, 4, u8::MAX] {
                assert_eq!(
                    TariScript::new(vec![crate::Opcode::Nop, ctor(1, n, keys.clone(), msg.clone())]),
                    Err(ScriptError::InvalidData),
                    "n = {n}"
                );
            }
            // n == keys.len() is accepted, and round-trips through bytes
            let script = TariScript::new(vec![ctor(2, 3, keys.clone(), msg.clone())]).unwrap();
            assert_eq!(TariScript::from_bytes(&script.to_bytes()).unwrap(), script);
        }
    }

    #[test]
    fn is_context_sensitive_per_opcode() {
        use crate::Opcode;
        let (msg, data) = multisig_data(1);
        let keys = vec![data[0].1.clone()];
        let sensitive = [
            Opcode::CheckHeightVerify(1),
            Opcode::CheckHeight(1),
            Opcode::CompareHeightVerify,
            Opcode::CompareHeight,
        ];
        let insensitive = [
            Opcode::Nop,
            Opcode::PushZero,
            Opcode::PushOne,
            Opcode::PushHash(Box::new([0u8; 32])),
            Opcode::PushInt(1),
            Opcode::PushPubKey(Box::default()),
            Opcode::Drop,
            Opcode::Dup,
            Opcode::RevRot,
            Opcode::GeZero,
            Opcode::GtZero,
            Opcode::LeZero,
            Opcode::LtZero,
            Opcode::Add,
            Opcode::Sub,
            Opcode::Equal,
            Opcode::EqualVerify,
            Opcode::Or(1),
            Opcode::OrVerify(1),
            Opcode::HashBlake256,
            Opcode::HashSha256,
            Opcode::HashSha3,
            Opcode::CheckSig(msg.clone()),
            Opcode::CheckSigVerify(msg.clone()),
            Opcode::CheckMultiSig(1, 1, keys.clone(), msg.clone()),
            Opcode::CheckMultiSigVerify(1, 1, keys.clone(), msg.clone()),
            Opcode::CheckMultiSigVerifyAggregatePubKey(1, 1, keys, msg),
            Opcode::ToRistrettoPoint,
            Opcode::Return,
            Opcode::IfThen,
            Opcode::Else,
            Opcode::EndIf,
        ];
        for op in &sensitive {
            assert!(
                TariScript::new(vec![op.clone()]).unwrap().is_context_sensitive(),
                "{op}"
            );
            // Anywhere in the script, including inside a branch
            let script = TariScript::new(vec![
                Opcode::PushOne,
                Opcode::IfThen,
                Opcode::Nop,
                Opcode::Else,
                op.clone(),
            ])
            .unwrap();
            assert!(script.is_context_sensitive(), "{op}");
        }
        for op in &insensitive {
            assert!(
                !TariScript::new(vec![op.clone()]).unwrap().is_context_sensitive(),
                "{op}"
            );
        }
        assert!(!TariScript::new(insensitive.to_vec()).unwrap().is_context_sensitive());
        assert!(!TariScript::new(vec![]).unwrap().is_context_sensitive());

        // The default context is height 0
        assert_eq!(ScriptContext::default().block_height(), 0);
        let script = script!(CheckHeight(0)).unwrap();
        assert_eq!(script.execute(&ExecutionStack::default()), Ok(Number(0)));
        let script = script!(CheckHeightVerify(1) PushOne).unwrap();
        assert_eq!(
            script.execute(&ExecutionStack::default()),
            Err(ScriptError::VerifyFailed)
        );
    }

    #[test]
    fn from_bytes_accepts_empty_script_pinned() {
        let script = TariScript::from_bytes(&[]).unwrap();
        assert_eq!(script.size(), 0);
        assert_eq!(script.execute(&inputs!(5)), Ok(Number(5)));
        assert_eq!(
            script.execute(&ExecutionStack::default()),
            Err(ScriptError::NonUnitLengthStack)
        );
    }
}
