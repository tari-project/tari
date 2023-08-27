// Copyright 2020. The Taiji Project
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
use std::{cmp::Ordering, collections::HashSet, convert::TryFrom, fmt, io, ops::Deref};

use blake2::Blake2b;
use borsh::{BorshDeserialize, BorshSerialize};
use digest::{consts::U32, Digest};
use integer_encoding::{VarIntReader, VarIntWriter};
use sha2::Sha256;
use sha3::Sha3_256;
use tari_crypto::{
    keys::PublicKey,
    ristretto::{RistrettoPublicKey, RistrettoSchnorr, RistrettoSecretKey},
};
use tari_utilities::{
    hex::{from_hex, to_hex, Hex, HexError},
    ByteArray,
};

use crate::{
    op_codes::Message,
    slice_to_hash,
    ExecutionStack,
    HashValue,
    Opcode,
    ScriptContext,
    ScriptError,
    StackItem,
};

#[macro_export]
macro_rules! script {
    ($($opcode:ident$(($($var:expr),+))?) +) => {{
        use $crate::TaijiScript;
        use $crate::Opcode;
        let script = vec![$(Opcode::$opcode $(($($var),+))?),+];
        TaijiScript::new(script)
    }}
}

const MAX_MULTISIG_LIMIT: u8 = 32;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaijiScript {
    script: Vec<Opcode>,
}

impl BorshSerialize for TaijiScript {
    fn serialize<W: io::Write>(&self, writer: &mut W) -> io::Result<()> {
        let bytes = self.to_bytes();
        writer.write_varint(bytes.len())?;
        for b in &bytes {
            b.serialize(writer)?;
        }
        Ok(())
    }
}

impl BorshDeserialize for TaijiScript {
    fn deserialize_reader<R>(reader: &mut R) -> Result<Self, io::Error>
    where R: io::Read {
        let len = reader.read_varint()?;
        let mut data = Vec::with_capacity(len);
        for _ in 0..len {
            data.push(u8::deserialize_reader(reader)?);
        }
        let script = TaijiScript::from_bytes(data.as_slice())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        Ok(script)
    }
}

impl TaijiScript {
    pub fn new(script: Vec<Opcode>) -> Self {
        TaijiScript { script }
    }

    /// Executes the script using a default context. If successful, returns the final stack item.
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

        for opcode in &self.script {
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
        self.script.as_slice()
    }

    /// Calculate the hash of the script.
    /// `as_hash` returns [ScriptError::InvalidDigest] if the digest function does not produce at least 32 bytes of
    /// output.
    pub fn as_hash<D: Digest>(&self) -> Result<HashValue, ScriptError> {
        if <D as Digest>::output_size() < 32 {
            return Err(ScriptError::InvalidDigest);
        }
        let h = D::digest(self.to_bytes());
        Ok(slice_to_hash(&h.as_slice()[..32]))
    }

    /// Try to deserialise a byte slice into a valid Taiji script
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ScriptError> {
        let script = Opcode::parse(bytes)?;

        Ok(TaijiScript { script })
    }

    /// Convert the script into an array of opcode strings.
    ///
    /// # Example
    /// ```edition2018
    /// use taiji_script::TaijiScript;
    /// use tari_utilities::hex::Hex;
    ///
    /// let hex_script = "71b07aae2337ce44f9ebb6169c863ec168046cb35ab4ef7aa9ed4f5f1f669bb74b09e58170ac276657a418820f34036b20ea615302b373c70ac8feab8d30681a3e0f0960e708";
    /// let script = TaijiScript::from_hex(hex_script).unwrap();
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

    /// Calculate the message hash that CHECKSIG uses to verify signatures
    pub fn script_message(&self, pub_key: &RistrettoPublicKey) -> Result<RistrettoSecretKey, ScriptError> {
        let b = Blake2b::<U32>::default()
            .chain_update(pub_key.as_bytes())
            .chain_update(self.to_bytes())
            .finalize();
        RistrettoSecretKey::from_bytes(b.as_slice()).map_err(|_| ScriptError::InvalidSignature)
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
            CheckHeightVerify(height) => TaijiScript::handle_check_height_verify(*height, ctx.block_height()),
            CheckHeight(height) => TaijiScript::handle_check_height(stack, *height, ctx.block_height()),
            CompareHeightVerify => TaijiScript::handle_compare_height_verify(stack, ctx.block_height()),
            CompareHeight => TaijiScript::handle_compare_height(stack, ctx.block_height()),
            Nop => Ok(()),
            PushZero => stack.push(Number(0)),
            PushOne => stack.push(Number(1)),
            PushHash(h) => stack.push(Hash(*h.clone())),
            PushInt(n) => stack.push(Number(*n)),
            PushPubKey(p) => stack.push(PublicKey(*p.clone())),
            Drop => TaijiScript::handle_drop(stack),
            Dup => TaijiScript::handle_dup(stack),
            RevRot => stack.push_down(2),
            GeZero => TaijiScript::handle_cmp_to_zero(stack, &[Ordering::Greater, Ordering::Equal]),
            GtZero => TaijiScript::handle_cmp_to_zero(stack, &[Ordering::Greater]),
            LeZero => TaijiScript::handle_cmp_to_zero(stack, &[Ordering::Less, Ordering::Equal]),
            LtZero => TaijiScript::handle_cmp_to_zero(stack, &[Ordering::Less]),
            Add => TaijiScript::handle_op_add(stack),
            Sub => TaijiScript::handle_op_sub(stack),
            Equal => {
                if TaijiScript::handle_equal(stack)? {
                    stack.push(Number(1))
                } else {
                    stack.push(Number(0))
                }
            },
            EqualVerify => {
                if TaijiScript::handle_equal(stack)? {
                    Ok(())
                } else {
                    Err(ScriptError::VerifyFailed)
                }
            },
            Or(n) => TaijiScript::handle_or(stack, *n),
            OrVerify(n) => TaijiScript::handle_or_verify(stack, *n),
            HashBlake256 => TaijiScript::handle_hash::<Blake2b<U32>>(stack),
            HashSha256 => TaijiScript::handle_hash::<Sha256>(stack),
            HashSha3 => TaijiScript::handle_hash::<Sha3_256>(stack),
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
            IfThen => TaijiScript::handle_if_then(stack, state),
            Else => TaijiScript::handle_else(state),
            EndIf => TaijiScript::handle_end_if(state),
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
        let item = StackItem::Number(block_height - height);

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

        let item = StackItem::Number(block_height - target_height);

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

    /// Handle opcodes that push a hash to the stack. I'm not doing any length checks right now, so this should be
    /// added once other digest functions are provided that don't produce 32 byte hashes
    fn handle_hash<D: Digest>(stack: &mut ExecutionStack) -> Result<(), ScriptError> {
        use StackItem::{Commitment, Hash, PublicKey};
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

    fn handle_op_add(stack: &mut ExecutionStack) -> Result<(), ScriptError> {
        use StackItem::{Commitment, Number, PublicKey, Signature};
        let top = stack.pop().ok_or(ScriptError::StackUnderflow)?;
        let two = stack.pop().ok_or(ScriptError::StackUnderflow)?;
        match (top, two) {
            (Number(v1), Number(v2)) => stack.push(Number(v1.checked_add(v2).ok_or(ScriptError::ValueExceedsBounds)?)),
            (Commitment(c1), Commitment(c2)) => stack.push(Commitment(&c1 + &c2)),
            (PublicKey(p1), PublicKey(p2)) => stack.push(PublicKey(&p1 + &p2)),
            (Signature(s1), Signature(s2)) => stack.push(Signature(&s1 + &s2)),
            (_, _) => Err(ScriptError::IncompatibleTypes),
        }
    }

    fn handle_op_sub(stack: &mut ExecutionStack) -> Result<(), ScriptError> {
        use StackItem::{Commitment, Number};
        let top = stack.pop().ok_or(ScriptError::StackUnderflow)?;
        let two = stack.pop().ok_or(ScriptError::StackUnderflow)?;
        match (top, two) {
            (Number(v1), Number(v2)) => stack.push(Number(v2.checked_sub(v1).ok_or(ScriptError::ValueExceedsBounds)?)),
            (Commitment(c1), Commitment(c2)) => stack.push(Commitment(&c2 - &c1)),
            (..) => Err(ScriptError::IncompatibleTypes),
        }
    }

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
            (PublicKey(p), Signature(s)) => Ok(s.verify_challenge(&p, &message)),
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
    /// * Every public key can be used AT MOST once.
    /// * Every signature MUST be a valid signature using one of the public keys
    /// * _m_ and _n_ must be positive AND m <= n AND n <= MAX_MULTISIG_LIMIT (32).
    fn check_multisig(
        &self,
        stack: &mut ExecutionStack,
        m: u8,
        n: u8,
        public_keys: &[RistrettoPublicKey],
        message: Message,
    ) -> Result<Option<RistrettoPublicKey>, ScriptError> {
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
            .collect::<Result<Vec<RistrettoSchnorr>, ScriptError>>()?;

        let mut key_signed = vec![false; public_keys.len()];
        // keep a hashset of unique signatures used to prevent someone putting the same signature in more than once.
        #[allow(clippy::mutable_key_type)]
        let mut sig_set = HashSet::new();

        let mut agg_pub_key = RistrettoPublicKey::default();
        // Check every signature against each public key looking for a valid signature
        for s in &signatures {
            for (i, pk) in public_keys.iter().enumerate() {
                if !sig_set.contains(s) && !key_signed[i] && s.verify_challenge(pk, &message) {
                    // This prevents Alice creating 2 different sigs against her public key
                    key_signed[i] = true;
                    sig_set.insert(s);
                    agg_pub_key = agg_pub_key + pk;
                    break;
                }
            }
            // Make sure the signature matched a public key
            if !sig_set.contains(s) {
                return Ok(None);
            }
        }
        if sig_set.len() == m {
            Ok(Some(agg_pub_key))
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
        let ristretto_sk = RistrettoSecretKey::from_bytes(scalar).map_err(|_| ScriptError::InvalidData)?;
        let ristretto_pk = RistrettoPublicKey::from_secret_key(&ristretto_sk);
        stack.push(StackItem::PublicKey(ristretto_pk))?;
        Ok(())
    }
}

impl Hex for TaijiScript {
    fn from_hex(hex: &str) -> Result<Self, HexError>
    where Self: Sized {
        let bytes = from_hex(hex)?;
        TaijiScript::from_bytes(&bytes).map_err(|_| HexError::HexConversionError {})
    }

    fn to_hex(&self) -> String {
        to_hex(&self.to_bytes())
    }
}

/// The default Taiji script is to push a single zero onto the stack; which will execute successfully with zero inputs.
impl Default for TaijiScript {
    fn default() -> Self {
        script!(PushZero)
    }
}

impl fmt::Display for TaijiScript {
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
    use blake2::Blake2b;
    use borsh::{BorshDeserialize, BorshSerialize};
    use digest::{consts::U32, Digest};
    use sha2::Sha256;
    use sha3::Sha3_256 as Sha3;
    use tari_crypto::{
        keys::{PublicKey, SecretKey},
        ristretto::{pedersen::PedersenCommitment, RistrettoPublicKey, RistrettoSchnorr, RistrettoSecretKey},
    };
    use tari_utilities::{hex::Hex, ByteArray};

    use crate::{
        error::ScriptError,
        inputs,
        op_codes::{slice_to_boxed_hash, slice_to_boxed_message, HashValue, Message},
        ExecutionStack,
        Opcode::CheckMultiSigVerifyAggregatePubKey,
        ScriptContext,
        StackItem,
        StackItem::{Commitment, Hash, Number},
        TaijiScript,
        DEFAULT_SCRIPT_HASH,
    };

    fn context_with_height(height: u64) -> ScriptContext {
        ScriptContext::new(height, &HashValue::default(), &PedersenCommitment::default())
    }

    #[test]
    fn default_script() {
        let script = TaijiScript::default();
        let inputs = ExecutionStack::default();
        assert!(script.execute(&inputs).is_ok());
        assert_eq!(&script.to_hex(), "7b");
        assert_eq!(script.as_hash::<Blake2b<U32>>().unwrap(), DEFAULT_SCRIPT_HASH);
    }

    #[test]
    fn op_or() {
        let script = script!(Or(1));

        let inputs = inputs!(4, 4);
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));

        let inputs = inputs!(3, 4);
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(0));

        let script = script!(Or(3));

        let inputs = inputs!(1, 2, 1, 3);
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));

        let inputs = inputs!(1, 2, 4, 3);
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(0));

        let mut rng = rand::thread_rng();
        let (_, p) = RistrettoPublicKey::random_keypair(&mut rng);
        let inputs = inputs!(1, p.clone(), 1, 3);
        let err = script.execute(&inputs).unwrap_err();
        assert!(matches!(err, ScriptError::InvalidInput));

        let inputs = inputs!(p, 2, 1, 3);
        let err = script.execute(&inputs).unwrap_err();
        assert!(matches!(err, ScriptError::InvalidInput));

        let inputs = inputs!(2, 4, 3);
        let err = script.execute(&inputs).unwrap_err();
        assert!(matches!(err, ScriptError::StackUnderflow));

        let script = script!(OrVerify(1));

        let inputs = inputs!(1, 4, 4);
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));

        let inputs = inputs!(1, 3, 4);
        let err = script.execute(&inputs).unwrap_err();
        assert!(matches!(err, ScriptError::VerifyFailed));

        let script = script!(OrVerify(2));

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
        let script = script!(IfThen PushInt(420) Else PushInt(66) EndIf);
        let inputs = inputs!(1);
        let result = script.execute(&inputs);
        assert_eq!(result.unwrap(), Number(420));

        let inputs = inputs!(0);
        let result = script.execute(&inputs);
        assert_eq!(result.unwrap(), Number(66));

        // nested
        let script = script!(IfThen PushOne IfThen PushInt(420) Else PushInt(555) EndIf Else PushInt(66) EndIf);
        let inputs = inputs!(1);
        let result = script.execute(&inputs);
        assert_eq!(result.unwrap(), Number(420));

        let script = script!(IfThen PushInt(420) Else PushZero IfThen PushInt(111) Else PushInt(66) EndIf Nop EndIf);
        let inputs = inputs!(0);
        let result = script.execute(&inputs);
        assert_eq!(result.unwrap(), Number(66));

        // duplicate else
        let script = script!(IfThen PushInt(420) Else PushInt(66) Else PushInt(777) EndIf);
        let inputs = inputs!(0);
        let result = script.execute(&inputs);
        assert_eq!(result.unwrap_err(), ScriptError::InvalidOpcode);

        // unexpected else
        let script = script!(Else);
        let inputs = inputs!(0);
        let result = script.execute(&inputs);
        assert_eq!(result.unwrap_err(), ScriptError::InvalidOpcode);

        // unexpected endif
        let script = script!(EndIf);
        let inputs = inputs!(0);
        let result = script.execute(&inputs);
        assert_eq!(result.unwrap_err(), ScriptError::InvalidOpcode);

        // duplicate endif
        let script = script!(IfThen PushInt(420) Else PushInt(66) EndIf EndIf);
        let inputs = inputs!(0);
        let result = script.execute(&inputs);
        assert_eq!(result.unwrap_err(), ScriptError::InvalidOpcode);

        // no else or endif
        let script = script!(IfThen PushOne IfThen PushOne);
        let inputs = inputs!(1);
        let result = script.execute(&inputs);
        assert_eq!(result.unwrap_err(), ScriptError::MissingOpcode);

        // no else
        let script = script!(IfThen PushOne EndIf);
        let inputs = inputs!(1);
        let result = script.execute(&inputs);
        assert_eq!(result.unwrap_err(), ScriptError::MissingOpcode);

        // nested bug
        let script = script!(IfThen PushInt(111) Else PushZero IfThen PushInt(222) Else PushInt(333) EndIf EndIf);
        let inputs = inputs!(1);
        let result = script.execute(&inputs);
        assert_eq!(result.unwrap(), Number(111));
    }

    #[test]
    fn op_check_height() {
        let inputs = ExecutionStack::default();
        let script = script!(CheckHeight(5));

        for block_height in 1..=10 {
            let ctx = context_with_height(u64::try_from(block_height).unwrap());
            assert_eq!(
                script.execute_with_context(&inputs, &ctx).unwrap(),
                Number(block_height - 5)
            );
        }

        let script = script!(CheckHeight(u64::MAX));
        let ctx = context_with_height(i64::MAX as u64);
        let err = script.execute_with_context(&inputs, &ctx).unwrap_err();
        assert!(matches!(err, ScriptError::ValueExceedsBounds));

        let script = script!(CheckHeightVerify(5));
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
        let script = script!(CompareHeight);
        let inputs = inputs!(5);

        for block_height in 1..=10 {
            let ctx = context_with_height(u64::try_from(block_height).unwrap());
            assert_eq!(
                script.execute_with_context(&inputs, &ctx).unwrap(),
                Number(block_height - 5)
            );
        }

        let script = script!(CompareHeightVerify);
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
        let script = script!(Drop PushOne);
        assert_eq!(script.execute(&inputs).unwrap(), Number(1));

        let script = script!(Drop PushZero);
        assert_eq!(script.execute(&inputs).unwrap(), Number(0));

        let script = script!(Drop PushInt(5));
        assert_eq!(script.execute(&inputs).unwrap(), Number(5));
    }

    #[test]
    fn op_comparison_to_zero() {
        let script = script!(GeZero);
        let inputs = inputs!(1);
        assert_eq!(script.execute(&inputs).unwrap(), Number(1));
        let inputs = inputs!(0);
        assert_eq!(script.execute(&inputs).unwrap(), Number(1));

        let script = script!(GtZero);
        let inputs = inputs!(1);
        assert_eq!(script.execute(&inputs).unwrap(), Number(1));
        let inputs = inputs!(0);
        assert_eq!(script.execute(&inputs).unwrap(), Number(0));

        let script = script!(LeZero);
        let inputs = inputs!(-1);
        assert_eq!(script.execute(&inputs).unwrap(), Number(1));
        let inputs = inputs!(0);
        assert_eq!(script.execute(&inputs).unwrap(), Number(1));

        let script = script!(LtZero);
        let inputs = inputs!(-1);
        assert_eq!(script.execute(&inputs).unwrap(), Number(1));
        let inputs = inputs!(0);
        assert_eq!(script.execute(&inputs).unwrap(), Number(0));
    }

    #[test]
    fn op_hash() {
        let mut rng = rand::thread_rng();
        let (_, p) = RistrettoPublicKey::random_keypair(&mut rng);
        let c = PedersenCommitment::from_public_key(&p);
        let script = script!(HashSha256);

        let hash = Sha256::digest(p.as_bytes());
        let inputs = inputs!(p.clone());
        assert_eq!(script.execute(&inputs).unwrap(), Hash(hash.into()));

        let hash = Sha256::digest(c.as_bytes());
        let inputs = inputs!(c.clone());
        assert_eq!(script.execute(&inputs).unwrap(), Hash(hash.into()));

        let script = script!(HashSha3);

        let hash = Sha3::digest(p.as_bytes());
        let inputs = inputs!(p);
        assert_eq!(script.execute(&inputs).unwrap(), Hash(hash.into()));

        let hash = Sha3::digest(c.as_bytes());
        let inputs = inputs!(c);
        assert_eq!(script.execute(&inputs).unwrap(), Hash(hash.into()));
    }

    #[test]
    fn op_return() {
        let script = script!(Return);
        let inputs = ExecutionStack::default();
        assert_eq!(script.execute(&inputs), Err(ScriptError::Return));
    }

    #[test]
    fn op_add() {
        let script = script!(Add);
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
        let script = script!(Add);
        let mut rng = rand::thread_rng();
        let (_, c1) = RistrettoPublicKey::random_keypair(&mut rng);
        let (_, c2) = RistrettoPublicKey::random_keypair(&mut rng);
        let c3 = &c1 + &c2;
        let c3 = PedersenCommitment::from_public_key(&c3);
        let inputs = inputs!(
            PedersenCommitment::from_public_key(&c1),
            PedersenCommitment::from_public_key(&c2)
        );
        assert_eq!(script.execute(&inputs).unwrap(), Commitment(c3));
    }

    #[test]
    fn op_sub() {
        use crate::StackItem::Number;
        let script = script!(Add Sub);
        let inputs = inputs!(5, 3, 2);
        assert_eq!(script.execute(&inputs).unwrap(), Number(0));
        let inputs = inputs!(i64::MAX, 1);
        assert_eq!(script.execute(&inputs), Err(ScriptError::ValueExceedsBounds));
        let script = script!(Sub);
        let inputs = inputs!(5, 3);
        assert_eq!(script.execute(&inputs).unwrap(), Number(2));
    }

    #[test]
    fn serialisation() {
        let script = script!(Add Sub Add);
        assert_eq!(&script.to_bytes(), &[0x93, 0x94, 0x93]);
        assert_eq!(TaijiScript::from_bytes(&[0x93, 0x94, 0x93]).unwrap(), script);
        assert_eq!(script.to_hex(), "939493");
        assert_eq!(TaijiScript::from_hex("939493").unwrap(), script);
    }

    #[test]
    fn check_sig() {
        use crate::StackItem::Number;
        let mut rng = rand::thread_rng();
        let (pvt_key, pub_key) = RistrettoPublicKey::random_keypair(&mut rng);
        let nonce = RistrettoSecretKey::random(&mut rng);
        let m_key = RistrettoSecretKey::random(&mut rng);
        let sig = RistrettoSchnorr::sign_raw(&pvt_key, nonce, m_key.as_bytes()).unwrap();
        let msg = slice_to_boxed_message(m_key.as_bytes());
        let script = script!(CheckSig(msg));
        let inputs = inputs!(sig.clone(), pub_key.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));

        let n_key = RistrettoSecretKey::random(&mut rng);
        let msg = slice_to_boxed_message(n_key.as_bytes());
        let script = script!(CheckSig(msg));
        let inputs = inputs!(sig, pub_key);
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(0));
    }

    #[test]
    fn check_sig_verify() {
        use crate::StackItem::Number;
        let mut rng = rand::thread_rng();
        let (pvt_key, pub_key) = RistrettoPublicKey::random_keypair(&mut rng);
        let nonce = RistrettoSecretKey::random(&mut rng);
        let m_key = RistrettoSecretKey::random(&mut rng);
        let sig = RistrettoSchnorr::sign_raw(&pvt_key, nonce, m_key.as_bytes()).unwrap();
        let msg = slice_to_boxed_message(m_key.as_bytes());
        let script = script!(CheckSigVerify(msg) PushOne);
        let inputs = inputs!(sig.clone(), pub_key.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));

        let n_key = RistrettoSecretKey::random(&mut rng);
        let msg = slice_to_boxed_message(n_key.as_bytes());
        let script = script!(CheckSigVerify(msg));
        let inputs = inputs!(sig, pub_key);
        let err = script.execute(&inputs).unwrap_err();
        assert!(matches!(err, ScriptError::VerifyFailed));
    }

    fn multisig_data(
        n: usize,
    ) -> (
        Box<Message>,
        Vec<(RistrettoSecretKey, RistrettoPublicKey, RistrettoSchnorr)>,
    ) {
        let mut rng = rand::thread_rng();
        let mut data = Vec::with_capacity(n);
        let m = RistrettoSecretKey::random(&mut rng);
        let msg = slice_to_boxed_message(m.as_bytes());

        for _ in 0..n {
            let (k, p) = RistrettoPublicKey::random_keypair(&mut rng);
            let r = RistrettoSecretKey::random(&mut rng);
            let s = RistrettoSchnorr::sign_raw(&k, r, m.as_bytes()).unwrap();
            data.push((k, p, s));
        }

        (msg, data)
    }

    #[allow(clippy::too_many_lines)]
    #[test]
    fn check_multisig() {
        use crate::{op_codes::Opcode::CheckMultiSig, StackItem::Number};
        let mut rng = rand::thread_rng();
        let (k_alice, p_alice) = RistrettoPublicKey::random_keypair(&mut rng);
        let (k_bob, p_bob) = RistrettoPublicKey::random_keypair(&mut rng);
        let (k_eve, _) = RistrettoPublicKey::random_keypair(&mut rng);
        let (k_carol, p_carol) = RistrettoPublicKey::random_keypair(&mut rng);
        let r1 = RistrettoSecretKey::random(&mut rng);
        let r2 = RistrettoSecretKey::random(&mut rng);
        let r3 = RistrettoSecretKey::random(&mut rng);
        let r4 = RistrettoSecretKey::random(&mut rng);
        let r5 = RistrettoSecretKey::random(&mut rng);
        let m = RistrettoSecretKey::random(&mut rng);
        let s_alice = RistrettoSchnorr::sign_raw(&k_alice, r1, m.as_bytes()).unwrap();
        let s_bob = RistrettoSchnorr::sign_raw(&k_bob, r2, m.as_bytes()).unwrap();
        let s_eve = RistrettoSchnorr::sign_raw(&k_eve, r3, m.as_bytes()).unwrap();
        let s_carol = RistrettoSchnorr::sign_raw(&k_carol, r4, m.as_bytes()).unwrap();
        let s_alice2 = RistrettoSchnorr::sign_raw(&k_alice, r5, m.as_bytes()).unwrap();
        let msg = slice_to_boxed_message(m.as_bytes());

        // 1 of 2
        let keys = vec![p_alice.clone(), p_bob.clone()];
        let ops = vec![CheckMultiSig(1, 2, keys, msg.clone())];
        let script = TaijiScript::new(ops);

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
        let script = TaijiScript::new(ops);

        let inputs = inputs!(s_alice.clone(), s_bob.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));
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
        let script = TaijiScript::new(ops);

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
        let script = TaijiScript::new(ops);

        let inputs = inputs!(s_alice.clone(), s_bob.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));
        let inputs = inputs!(s_alice.clone(), s_carol.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));
        let inputs = inputs!(s_carol.clone(), s_bob.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));
        let inputs = inputs!(s_carol.clone(), s_eve.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(0));

        // check that sigs are only counted once
        let keys = vec![p_alice.clone(), p_bob.clone(), p_alice.clone()];
        let ops = vec![CheckMultiSig(2, 3, keys, msg.clone())];
        let script = TaijiScript::new(ops);

        let inputs = inputs!(s_alice.clone(), s_carol.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(0));
        let inputs = inputs!(s_alice.clone(), s_alice.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(0));
        let inputs = inputs!(s_alice.clone(), s_alice2);
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));

        // 3 of 3
        let keys = vec![p_alice.clone(), p_bob.clone(), p_carol];
        let ops = vec![CheckMultiSig(3, 3, keys, msg.clone())];
        let script = TaijiScript::new(ops);

        let inputs = inputs!(s_alice.clone(), s_bob.clone(), s_carol.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));
        let inputs = inputs!(s_eve.clone(), s_bob.clone(), s_carol);
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(0));
        let inputs = inputs!(s_eve, s_bob);
        let err = script.execute(&inputs).unwrap_err();
        assert_eq!(err, ScriptError::StackUnderflow);

        // errors
        let keys = vec![p_alice.clone(), p_bob.clone()];
        let ops = vec![CheckMultiSig(0, 2, keys, msg.clone())];
        let script = TaijiScript::new(ops);
        let inputs = inputs!(s_alice.clone());
        let err = script.execute(&inputs).unwrap_err();
        assert_eq!(err, ScriptError::ValueExceedsBounds);

        let keys = vec![p_alice.clone(), p_bob.clone()];
        let ops = vec![CheckMultiSig(1, 0, keys, msg.clone())];
        let script = TaijiScript::new(ops);
        let inputs = inputs!(s_alice.clone());
        let err = script.execute(&inputs).unwrap_err();
        assert_eq!(err, ScriptError::ValueExceedsBounds);

        let keys = vec![p_alice, p_bob];
        let ops = vec![CheckMultiSig(2, 1, keys, msg)];
        let script = TaijiScript::new(ops);
        let inputs = inputs!(s_alice);
        let err = script.execute(&inputs).unwrap_err();
        assert_eq!(err, ScriptError::ValueExceedsBounds);

        // max n is 32
        let (msg, data) = multisig_data(33);
        let keys = data.iter().map(|(_, p, _)| p.clone()).collect();
        let sigs = data.iter().take(17).map(|(_, _, s)| s.clone());
        let script = script!(CheckMultiSig(17, 33, keys, msg));
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
        let script = TaijiScript::new(ops);
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
        let script = TaijiScript::new(ops);
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

    #[allow(clippy::too_many_lines)]
    #[test]
    fn check_multisig_verify() {
        use crate::{op_codes::Opcode::CheckMultiSigVerify, StackItem::Number};
        let mut rng = rand::thread_rng();
        let (k_alice, p_alice) = RistrettoPublicKey::random_keypair(&mut rng);
        let (k_bob, p_bob) = RistrettoPublicKey::random_keypair(&mut rng);
        let (k_eve, _) = RistrettoPublicKey::random_keypair(&mut rng);
        let (k_carol, p_carol) = RistrettoPublicKey::random_keypair(&mut rng);
        let r1 = RistrettoSecretKey::random(&mut rng);
        let r2 = RistrettoSecretKey::random(&mut rng);
        let r3 = RistrettoSecretKey::random(&mut rng);
        let r4 = RistrettoSecretKey::random(&mut rng);
        let m = RistrettoSecretKey::random(&mut rng);
        let s_alice = RistrettoSchnorr::sign_raw(&k_alice, r1, m.as_bytes()).unwrap();
        let s_bob = RistrettoSchnorr::sign_raw(&k_bob, r2, m.as_bytes()).unwrap();
        let s_eve = RistrettoSchnorr::sign_raw(&k_eve, r3, m.as_bytes()).unwrap();
        let s_carol = RistrettoSchnorr::sign_raw(&k_carol, r4, m.as_bytes()).unwrap();
        let msg = slice_to_boxed_message(m.as_bytes());

        // 1 of 2
        let keys = vec![p_alice.clone(), p_bob.clone()];
        let ops = vec![CheckMultiSigVerify(1, 2, keys, msg.clone())];
        let script = TaijiScript::new(ops);

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
        let script = TaijiScript::new(ops);

        let inputs = inputs!(Number(1), s_alice.clone(), s_bob.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));
        let inputs = inputs!(Number(1), s_eve.clone(), s_bob.clone());
        let err = script.execute(&inputs).unwrap_err();
        assert_eq!(err, ScriptError::VerifyFailed);

        // 1 of 3
        let keys = vec![p_alice.clone(), p_bob.clone(), p_carol.clone()];
        let ops = vec![CheckMultiSigVerify(1, 3, keys, msg.clone())];
        let script = TaijiScript::new(ops);

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
        let script = TaijiScript::new(ops);

        let inputs = inputs!(Number(1), s_alice.clone(), s_bob.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));
        let inputs = inputs!(Number(1), s_alice.clone(), s_carol.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));
        let inputs = inputs!(Number(1), s_carol.clone(), s_bob.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));
        let inputs = inputs!(Number(1), s_carol.clone(), s_eve.clone());
        let err = script.execute(&inputs).unwrap_err();
        assert_eq!(err, ScriptError::VerifyFailed);

        // 2 of 3 (returning the aggregate public key of the signatories)
        let keys = vec![p_alice.clone(), p_bob.clone(), p_carol.clone()];
        let ops = vec![CheckMultiSigVerifyAggregatePubKey(2, 3, keys, msg.clone())];
        let script = TaijiScript::new(ops);

        let inputs = inputs!(s_alice.clone(), s_bob.clone());
        let agg_pub_key = script.execute(&inputs).unwrap();
        assert_eq!(agg_pub_key, StackItem::PublicKey(p_alice.clone() + p_bob.clone()));

        let inputs = inputs!(s_alice.clone(), s_carol.clone());
        let agg_pub_key = script.execute(&inputs).unwrap();
        assert_eq!(agg_pub_key, StackItem::PublicKey(p_alice.clone() + p_carol.clone()));

        let inputs = inputs!(s_bob.clone(), s_carol.clone());
        let agg_pub_key = script.execute(&inputs).unwrap();
        assert_eq!(agg_pub_key, StackItem::PublicKey(p_bob.clone() + p_carol.clone()));

        let inputs = inputs!(s_alice.clone(), s_carol.clone(), s_bob.clone());
        let err = script.execute(&inputs).unwrap_err();
        assert_eq!(err, ScriptError::NonUnitLengthStack);

        let inputs = inputs!(p_bob.clone());
        let err = script.execute(&inputs).unwrap_err();
        assert_eq!(err, ScriptError::StackUnderflow);

        // 3 of 3
        let keys = vec![p_alice.clone(), p_bob.clone(), p_carol];
        let ops = vec![CheckMultiSigVerify(3, 3, keys, msg.clone())];
        let script = TaijiScript::new(ops);

        let inputs = inputs!(Number(1), s_alice.clone(), s_bob.clone(), s_carol.clone());
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));
        let inputs = inputs!(Number(1), s_eve.clone(), s_bob.clone(), s_carol);
        let err = script.execute(&inputs).unwrap_err();
        assert_eq!(err, ScriptError::VerifyFailed);
        let inputs = inputs!(Number(1), s_eve, s_bob);
        let err = script.execute(&inputs).unwrap_err();
        assert_eq!(err, ScriptError::IncompatibleTypes);

        // errors
        let keys = vec![p_alice.clone(), p_bob.clone()];
        let ops = vec![CheckMultiSigVerify(0, 2, keys, msg.clone())];
        let script = TaijiScript::new(ops);
        let inputs = inputs!(s_alice.clone());
        let err = script.execute(&inputs).unwrap_err();
        assert_eq!(err, ScriptError::ValueExceedsBounds);

        let keys = vec![p_alice.clone(), p_bob.clone()];
        let ops = vec![CheckMultiSigVerify(1, 0, keys, msg.clone())];
        let script = TaijiScript::new(ops);
        let inputs = inputs!(s_alice.clone());
        let err = script.execute(&inputs).unwrap_err();
        assert_eq!(err, ScriptError::ValueExceedsBounds);

        let keys = vec![p_alice, p_bob];
        let ops = vec![CheckMultiSigVerify(2, 1, keys, msg)];
        let script = TaijiScript::new(ops);
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
        let script = TaijiScript::new(ops);
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
        let script = TaijiScript::new(ops);
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
    fn add_partial_signatures() {
        use crate::StackItem::Number;
        let mut rng = rand::thread_rng();
        let (k1, p1) = RistrettoPublicKey::random_keypair(&mut rng);
        let (k2, p2) = RistrettoPublicKey::random_keypair(&mut rng);
        let r1 = RistrettoSecretKey::random(&mut rng);
        let r2 = RistrettoSecretKey::random(&mut rng);

        let m = RistrettoSecretKey::random(&mut rng);
        let msg = slice_to_boxed_message(m.as_bytes());
        let script = script!(Add RevRot Add CheckSigVerify(msg) PushOne);

        let s1 = RistrettoSchnorr::sign_raw(&k1, r1, m.as_bytes()).unwrap();
        let s2 = RistrettoSchnorr::sign_raw(&k2, r2, m.as_bytes()).unwrap();
        let inputs = inputs!(p1, p2, s1, s2);
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));
    }

    #[test]
    fn pay_to_public_key_hash() {
        use crate::StackItem::PublicKey;
        let k =
            RistrettoSecretKey::from_hex("7212ac93ee205cdbbb57c4f0f815fbf8db25b4d04d3532e2262e31907d82c700").unwrap();
        let p = RistrettoPublicKey::from_secret_key(&k); // 56c0fa32558d6edc0916baa26b48e745de834571534ca253ea82435f08ebbc7c
        let hash = Blake2b::<U32>::digest(p.as_bytes());
        let pkh = slice_to_boxed_hash(hash.as_slice()); // ae2337ce44f9ebb6169c863ec168046cb35ab4ef7aa9ed4f5f1f669bb74b09e5

        // Unlike in Bitcoin where P2PKH includes a CheckSig at the end of the script, that part of the process is built
        // into definition of how TaijiScript is evaluated by a base node or wallet
        let script = script!(Dup HashBlake256 PushHash(pkh) EqualVerify);
        let hex_script = "71b07aae2337ce44f9ebb6169c863ec168046cb35ab4ef7aa9ed4f5f1f669bb74b09e581";
        // Test serialisation
        assert_eq!(script.to_hex(), hex_script);
        // Test de-serialisation
        assert_eq!(TaijiScript::from_hex(hex_script).unwrap(), script);

        let inputs = inputs!(p.clone());

        let result = script.execute(&inputs).unwrap();

        assert_eq!(result, PublicKey(p));
    }

    #[test]
    fn hex_only() {
        use crate::StackItem::Number;
        let hex = "0500f7c695528c858cde76dab3076908e01228b6dbdd5f671bed1b03b89e170c313d415e0584ef82b79e3bf9bdebeeef53d13aefdc0cfa64f616acea0229e6ee0f0456c0fa32558d6edc0916baa26b48e745de834571534ca253ea82435f08ebbc7c";
        let inputs = ExecutionStack::from_hex(hex).unwrap();
        let script =
            TaijiScript::from_hex("71b07aae2337ce44f9ebb6169c863ec168046cb35ab4ef7aa9ed4f5f1f669bb74b09e581ac276657a418820f34036b20ea615302b373c70ac8feab8d30681a3e0f0960e708")
                .unwrap();
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(1));

        // Try again with invalid sig
        let inputs = ExecutionStack::from_hex("0500b7c695528c858cde76dab3076908e01228b6dbdd5f671bed1b03\
        b89e170c314c7b413e971dbb85879ba990e851607454da4bdf65839456d7cac19e5a338f060456c0fa32558d6edc0916baa26b48e745de8\
        34571534ca253ea82435f08ebbc7c").unwrap();
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, Number(0));
    }

    #[test]
    fn disassemble() {
        let hex_script = "71b07aae2337ce44f9ebb6169c863ec168046cb35ab4ef7aa9ed4f5f1f669bb74b09e58170ac276657a418820f34036b20ea615302b373c70ac8feab8d30681a3e0f0960e708";
        let script = TaijiScript::from_hex(hex_script).unwrap();
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
        let p_alice = RistrettoPublicKey::from_secret_key(&k_alice); // 9c35e9f0f11cf25ce3ca1182d37682ab5824aa033f2024651e007364d06ec355

        let k_bob =
            RistrettoSecretKey::from_hex("e0689386a018e88993a7bb14cbff5bad8a8858ea101d6e0da047df3ddf499c0e").unwrap();
        let p_bob = RistrettoPublicKey::from_secret_key(&k_bob); // 3a58f371e94da76a8902e81b4b55ddabb7dc006cd8ebde3011c46d0e02e9172f

        let lock_height = 4000u64;

        let script = script!(Dup PushPubKey(Box::new(p_bob.clone())) CheckHeight(lock_height) GeZero IfThen PushPubKey(Box::new(p_alice.clone())) OrVerify(2) Else EqualVerify EndIf );

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
        let mut rng = rand::thread_rng();
        let (k_alice, p_alice) = RistrettoPublicKey::random_keypair(&mut rng);
        let (k_bob, p_bob) = RistrettoPublicKey::random_keypair(&mut rng);
        let (k_eve, _) = RistrettoPublicKey::random_keypair(&mut rng);
        let r1 = RistrettoSecretKey::random(&mut rng);
        let r2 = RistrettoSecretKey::random(&mut rng);
        let r3 = RistrettoSecretKey::random(&mut rng);

        let m = RistrettoSecretKey::random(&mut rng);
        let msg = slice_to_boxed_message(m.as_bytes());

        let s_alice = RistrettoSchnorr::sign_raw(&k_alice, r1, m.as_bytes()).unwrap();
        let s_bob = RistrettoSchnorr::sign_raw(&k_bob, r2, m.as_bytes()).unwrap();
        let s_eve = RistrettoSchnorr::sign_raw(&k_eve, r3, m.as_bytes()).unwrap();

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
        let script = TaijiScript::new(ops);

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
        use crate::StackItem::PublicKey;
        let mut rng = rand::thread_rng();
        let (k_1, p_1) = RistrettoPublicKey::random_keypair(&mut rng);

        use crate::Opcode::ToRistrettoPoint;
        let ops = vec![ToRistrettoPoint];
        let script = TaijiScript::new(ops);

        // Invalid stack type
        let inputs = inputs!(RistrettoPublicKey::default());
        let err = script.execute(&inputs).unwrap_err();
        assert!(matches!(err, ScriptError::IncompatibleTypes));

        // scalar
        let mut scalar = [0u8; 32];
        scalar.copy_from_slice(k_1.as_bytes());
        let inputs = inputs!(scalar);
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, PublicKey(p_1.clone()));

        // hash
        let inputs = ExecutionStack::new(vec![Hash(scalar)]);
        let result = script.execute(&inputs).unwrap();
        assert_eq!(result, PublicKey(p_1));
    }

    #[test]
    fn test_borsh_de_serialization() {
        let hex_script = "71b07aae2337ce44f9ebb6169c863ec168046cb35ab4ef7aa9ed4f5f1f669bb74b09e58170ac276657a418820f34036b20ea615302b373c70ac8feab8d30681a3e0f0960e708";
        let script = TaijiScript::from_hex(hex_script).unwrap();
        let mut buf = Vec::new();
        script.serialize(&mut buf).unwrap();
        buf.extend_from_slice(&[1, 2, 3]);
        let buf = &mut buf.as_slice();
        assert_eq!(script, TaijiScript::deserialize(buf).unwrap());
        assert_eq!(buf, &[1, 2, 3]);
    }
}
