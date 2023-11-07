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

use std::{convert::TryFrom, io};

use borsh::{BorshDeserialize, BorshSerialize};
use integer_encoding::{VarIntReader, VarIntWriter};
use tari_crypto::ristretto::{pedersen::PedersenCommitment, RistrettoPublicKey, RistrettoSchnorr, RistrettoSecretKey};
use tari_utilities::{
    hex::{from_hex, to_hex, Hex, HexError},
    ByteArray,
};

use crate::{
    error::ScriptError,
    op_codes::{HashValue, ScalarValue},
};

pub const MAX_STACK_SIZE: usize = 255;

#[macro_export]
macro_rules! inputs {
    ($($input:expr),+) => {{
        use $crate::{ExecutionStack, StackItem};

        let items = vec![$(StackItem::from($input)),+];
        ExecutionStack::new(items)
    }}
}

macro_rules! stack_item_from {
    ($from_type:ty => $variant:ident) => {
        impl From<$from_type> for StackItem {
            fn from(item: $from_type) -> Self {
                StackItem::$variant(item)
            }
        }
    };
}

pub const TYPE_NUMBER: u8 = 1;
pub const TYPE_HASH: u8 = 2;
pub const TYPE_COMMITMENT: u8 = 3;
pub const TYPE_PUBKEY: u8 = 4;
pub const TYPE_SIG: u8 = 5;
pub const TYPE_SCALAR: u8 = 6;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StackItem {
    Number(i64),
    Hash(HashValue),
    Scalar(ScalarValue),
    Commitment(PedersenCommitment),
    PublicKey(RistrettoPublicKey),
    Signature(RistrettoSchnorr),
}

impl StackItem {
    /// Convert an input item into its binary representation and append it to the array. The function returns the byte
    /// slice that matches the item as a convenience
    pub fn to_bytes<'a>(&self, array: &'a mut Vec<u8>) -> &'a [u8] {
        let n = array.len();
        match self {
            StackItem::Number(v) => {
                array.push(TYPE_NUMBER);
                array.extend_from_slice(&v.to_le_bytes());
            },
            StackItem::Hash(h) => {
                array.push(TYPE_HASH);
                array.extend_from_slice(&h[..]);
            },
            StackItem::Commitment(c) => {
                array.push(TYPE_COMMITMENT);
                array.extend_from_slice(c.as_bytes());
            },
            StackItem::PublicKey(p) => {
                array.push(TYPE_PUBKEY);
                array.extend_from_slice(p.as_bytes());
            },
            StackItem::Signature(s) => {
                array.push(TYPE_SIG);
                array.extend_from_slice(s.get_public_nonce().as_bytes());
                array.extend_from_slice(s.get_signature().as_bytes());
            },
            StackItem::Scalar(scalar) => {
                array.push(TYPE_SCALAR);
                array.extend_from_slice(scalar);
            },
        };
        &array[n..]
    }

    /// Take a byte slice and read the next stack item from it, including any associated data. `read_next` returns a
    /// tuple of the deserialised item, and an updated slice that has the Opcode and data removed.
    pub fn read_next(bytes: &[u8]) -> Option<(Self, &[u8])> {
        let code = bytes.first()?;
        match *code {
            TYPE_NUMBER => StackItem::b_to_number(&bytes[1..]),
            TYPE_HASH => StackItem::b_to_hash(&bytes[1..]),
            TYPE_COMMITMENT => StackItem::b_to_commitment(&bytes[1..]),
            TYPE_PUBKEY => StackItem::b_to_pubkey(&bytes[1..]),
            TYPE_SIG => StackItem::b_to_sig(&bytes[1..]),
            TYPE_SCALAR => StackItem::b_to_scalar(&bytes[1..]),
            _ => None,
        }
    }

    fn b_to_number(b: &[u8]) -> Option<(Self, &[u8])> {
        if b.len() < 8 {
            return None;
        }
        let mut arr = [0u8; 8];
        arr.copy_from_slice(&b[..8]);
        Some((StackItem::Number(i64::from_le_bytes(arr)), &b[8..]))
    }

    fn b_to_hash(b: &[u8]) -> Option<(Self, &[u8])> {
        if b.len() < 32 {
            return None;
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&b[..32]);
        Some((StackItem::Hash(arr), &b[32..]))
    }

    fn b_to_scalar(b: &[u8]) -> Option<(Self, &[u8])> {
        if b.len() < 32 {
            return None;
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&b[..32]);
        Some((StackItem::Scalar(arr), &b[32..]))
    }

    fn b_to_commitment(b: &[u8]) -> Option<(Self, &[u8])> {
        if b.len() < 32 {
            return None;
        }
        let c = PedersenCommitment::from_canonical_bytes(&b[..32]).ok()?;
        Some((StackItem::Commitment(c), &b[32..]))
    }

    fn b_to_pubkey(b: &[u8]) -> Option<(Self, &[u8])> {
        if b.len() < 32 {
            return None;
        }
        let p = RistrettoPublicKey::from_canonical_bytes(&b[..32]).ok()?;
        Some((StackItem::PublicKey(p), &b[32..]))
    }

    fn b_to_sig(b: &[u8]) -> Option<(Self, &[u8])> {
        if b.len() < 64 {
            return None;
        }
        let r = RistrettoPublicKey::from_canonical_bytes(&b[..32]).ok()?;
        let s = RistrettoSecretKey::from_canonical_bytes(&b[32..64]).ok()?;
        let sig = RistrettoSchnorr::new(r, s);
        Some((StackItem::Signature(sig), &b[64..]))
    }
}

stack_item_from!(i64 => Number);
stack_item_from!(PedersenCommitment => Commitment);
stack_item_from!(RistrettoPublicKey => PublicKey);
stack_item_from!(RistrettoSchnorr => Signature);
stack_item_from!(ScalarValue => Scalar);

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ExecutionStack {
    items: Vec<StackItem>,
}

impl BorshSerialize for ExecutionStack {
    fn serialize<W: io::Write>(&self, writer: &mut W) -> io::Result<()> {
        let bytes = self.to_bytes();
        writer.write_varint(bytes.len())?;
        for b in &bytes {
            b.serialize(writer)?;
        }
        Ok(())
    }
}

impl BorshDeserialize for ExecutionStack {
    fn deserialize_reader<R>(reader: &mut R) -> Result<Self, io::Error>
    where R: io::Read {
        let len = reader.read_varint()?;
        if len > MAX_STACK_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Larger than max execution stack bytes".to_string(),
            ));
        }
        let mut data = Vec::with_capacity(len);
        for _ in 0..len {
            data.push(u8::deserialize_reader(reader)?);
        }
        let stack = Self::from_bytes(data.as_slice())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?;
        Ok(stack)
    }
}

impl ExecutionStack {
    /// Return a new `ExecutionStack` using the vector of [StackItem] in `items`
    pub fn new(items: Vec<StackItem>) -> Self {
        ExecutionStack { items }
    }

    /// Returns the number of entries in the execution stack
    pub fn size(&self) -> usize {
        self.items.len()
    }

    /// Returns a reference to the top entry in the stack without affecting the stack
    pub fn peek(&self) -> Option<&StackItem> {
        self.items.last()
    }

    /// Returns true if the stack is empty
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Pops the top item in the stack. If the stack is not empty, `pop` returns the item, otherwise return `None` if
    /// it is empty.
    pub fn pop(&mut self) -> Option<StackItem> {
        self.items.pop()
    }

    /// Pops the top item in the stack and applies TryFrom for the given generic type. If the stack is not empty, and is
    /// a StackItem::Number, `pop_into_number` returns the parsed number. Returns an error if the stack is empty or if
    /// the top item is a different variant.
    pub fn pop_into_number<T: TryFrom<i64>>(&mut self) -> Result<T, ScriptError> {
        let item = self.items.pop().ok_or(ScriptError::StackUnderflow)?;

        let number = match item {
            StackItem::Number(n) => T::try_from(n).map_err(|_| ScriptError::ValueExceedsBounds)?,
            _ => return Err(ScriptError::InvalidInput),
        };

        Ok(number)
    }

    /// Pops n + 1 items from the stack. Checks if the last popped item matches any of the first n items. Returns an
    /// error if all n + 1 items aren't of the same variant, or if there are not n + 1 items on the stack.
    pub fn pop_n_plus_one_contains(&mut self, n: u8) -> Result<bool, ScriptError> {
        let items = self.pop_num_items(n as usize)?;
        let item = self.pop().ok_or(ScriptError::StackUnderflow)?;

        // check that all popped items are of the same variant
        // first count each variant
        let counts = items.iter().fold([0; 6], counter);
        // also check the n + 1 item
        let counts = counter(counts, &item);

        // then filter those with more than 0
        let num_distinct_variants = counts.iter().filter(|&c| *c > 0).count();

        if num_distinct_variants > 1 {
            return Err(ScriptError::InvalidInput);
        }

        Ok(items.contains(&item))
    }

    /// Pops the top n items in the stack. If the stack has at least n items, `pop_num_items` returns the items in stack
    /// order (ie. bottom first), otherwise returns an error.
    pub fn pop_num_items(&mut self, num_items: usize) -> Result<Vec<StackItem>, ScriptError> {
        let stack_size = self.size();

        if stack_size < num_items {
            Err(ScriptError::StackUnderflow)
        } else {
            let at = stack_size - num_items;
            let items = self.items.split_off(at);

            Ok(items)
        }
    }

    /// Return a binary array representation of the input stack
    pub fn to_bytes(&self) -> Vec<u8> {
        self.items.iter().fold(Vec::new(), |mut bytes, item| {
            item.to_bytes(&mut bytes);
            bytes
        })
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ScriptError> {
        let mut stack = ExecutionStack { items: Vec::new() };
        let mut byte_str = bytes;
        while !byte_str.is_empty() {
            match StackItem::read_next(byte_str) {
                Some((item, b)) => {
                    stack.push(item)?;
                    byte_str = b;
                },
                None => return Err(ScriptError::InvalidInput),
            }
        }
        Ok(stack)
    }

    /// Pushes the item onto the top of the stack. This function will only error if the new stack size exceeds the
    /// maximum allowed stack size, given by [MAX_STACK_SIZE]
    pub fn push(&mut self, item: StackItem) -> Result<(), ScriptError> {
        if self.size() >= MAX_STACK_SIZE {
            return Err(ScriptError::StackOverflow);
        }
        self.items.push(item);
        Ok(())
    }

    /// Pushes the top stack element down `depth` positions
    pub(crate) fn push_down(&mut self, depth: usize) -> Result<(), ScriptError> {
        let n = self.size();
        if n < depth + 1 {
            return Err(ScriptError::StackUnderflow);
        }
        if depth == 0 {
            return Ok(());
        }
        let top = self.pop().unwrap();
        self.items.insert(n - depth - 1, top);
        Ok(())
    }
}

impl Hex for ExecutionStack {
    fn from_hex(hex: &str) -> Result<Self, HexError>
    where Self: Sized {
        let b = from_hex(hex)?;
        ExecutionStack::from_bytes(&b).map_err(|_| HexError::HexConversionError {})
    }

    fn to_hex(&self) -> String {
        to_hex(&self.to_bytes())
    }
}

/// Utility function that given a count of `StackItem` variants, adds 1 for the given item.
#[allow(clippy::many_single_char_names)]
fn counter(values: [u8; 6], item: &StackItem) -> [u8; 6] {
    let [n, h, c, p, s, z] = values;
    #[allow(clippy::enum_glob_use)]
    use StackItem::*;
    match item {
        Number(_) => {
            let n = n + 1;
            [n, h, c, p, s, z]
        },
        Hash(_) => {
            let h = h + 1;
            [n, h, c, p, s, z]
        },
        Commitment(_) => {
            let c = c + 1;
            [n, h, c, p, s, z]
        },
        PublicKey(_) => {
            let p = p + 1;
            [n, h, c, p, s, z]
        },
        Signature(_) => {
            let s = s + 1;
            [n, h, c, p, s, z]
        },
        Scalar(_) => {
            let z = z + 1;
            [n, h, c, p, s, z]
        },
    }
}

#[cfg(test)]
mod test {
    use blake2::Blake2b;
    use borsh::{BorshDeserialize, BorshSerialize};
    use digest::{
        consts::{U32, U64},
        Digest,
    };
    use rand::rngs::OsRng;
    use tari_crypto::{
        keys::{PublicKey, SecretKey},
        ristretto::{pedersen::PedersenCommitment, RistrettoPublicKey, RistrettoSchnorr, RistrettoSecretKey},
    };
    use tari_utilities::{
        hex::{from_hex, Hex},
        message_format::MessageFormat,
        ByteArray,
    };

    use crate::{op_codes::ScalarValue, ExecutionStack, HashValue, StackItem};

    #[test]
    fn as_bytes_roundtrip() {
        use crate::StackItem::{Number, PublicKey, Signature};
        let k = RistrettoSecretKey::random(&mut rand::thread_rng());
        let p = RistrettoPublicKey::from_secret_key(&k);
        let s = RistrettoSchnorr::sign(&k, b"hi", &mut OsRng).unwrap();
        let items = vec![Number(5432), Number(21), Signature(s), PublicKey(p)];
        let stack = ExecutionStack::new(items);
        let bytes = stack.to_bytes();
        let stack2 = ExecutionStack::from_bytes(&bytes).unwrap();
        assert_eq!(stack, stack2);
    }

    #[test]
    fn deserialisation() {
        let k =
            RistrettoSecretKey::from_hex("7212ac93ee205cdbbb57c4f0f815fbf8db25b4d04d3532e2262e31907d82c700").unwrap();
        let r =
            RistrettoSecretKey::from_hex("193ee873f3de511eda8ae387db6498f3d194d31a130a94cdf13dc5890ec1ad0f").unwrap();
        let p = RistrettoPublicKey::from_secret_key(&k);
        let m = RistrettoSecretKey::from_uniform_bytes(&Blake2b::<U64>::digest(b"Hello Tari Script")).unwrap();
        let sig = RistrettoSchnorr::sign_raw_canonical(&k, r, m.as_bytes()).unwrap();
        let mut scalar: ScalarValue = [0u8; 32];
        scalar.copy_from_slice(m.as_bytes());
        let inputs = inputs!(sig, p, scalar);
        assert_eq!(inputs.to_hex(), "0500f7c695528c858cde76dab3076908e01228b6dbdd5f671bed1b03b89e170c315c4a28c0202dec8769e7a6cc5b407e90664ce73c57404ab9c288bfe6a72d0d090456c0fa32558d6edc0916baa26b48e745de834571534ca253ea82435f08ebbc7c067c8f42406bb109bfcf5aadf0c72d9324a49b9f4758c83fb2f3364baf562f7d00");
    }

    #[test]
    fn serialisation() {
        // let p =
        //     RistrettoPublicKey::from_hex("56c0fa32558d6edc0916baa26b48e745de834571534ca253ea82435f08ebbc7c").
        // unwrap(); let r =
        //     RistrettoPublicKey::from_hex("00f7c695528c858cde76dab3076908e01228b6dbdd5f671bed1b03b89e170c31").
        // unwrap(); let s =
        //     RistrettoSecretKey::from_hex("6db1023d5c46d78a97da8eb6c5a37e00d5f2fee182dcb38c1b6c65e90a43c109").
        // unwrap(); let sig = RistrettoSchnorr::new(r, s);
        // let m: HashValue = Blake2b::<U32>::digest(b"Hello Tari Script").into();
        // let inputs = inputs!(m, sig, p);
        // eprintln!("to_hex(&m) = {:?}", tari_utilities::hex::to_hex(&m));
        // eprintln!("inputs.to_hex() = {:?}", inputs.to_hex());

        let s = "06fdf9fc345d2cdd8aff624a55f824c7c9ce3cc972e011b4e750e417a90ecc5da50500f7c695528c858cde76dab3076908e0122\
        8b6dbdd5f671bed1b03b89e170c316db1023d5c46d78a97da8eb6c5a37e00d5f2fee182dcb38c1b6c65e90a43c1090456c0fa32558d6edc0916baa2\
        6b48e745de834571534ca253ea82435f08ebbc7c";
        let mut stack = ExecutionStack::from_hex(s).unwrap();
        assert_eq!(stack.size(), 3);
        if let Some(StackItem::PublicKey(p)) = stack.pop() {
            assert_eq!(
                p.to_hex(),
                "56c0fa32558d6edc0916baa26b48e745de834571534ca253ea82435f08ebbc7c"
            );
        } else {
            panic!("Expected pubkey")
        }
        if let Some(StackItem::Signature(s)) = stack.pop() {
            assert_eq!(
                s.get_public_nonce().to_hex(),
                "00f7c695528c858cde76dab3076908e01228b6dbdd5f671bed1b03b89e170c31"
            );
            assert_eq!(
                s.get_signature().to_hex(),
                "6db1023d5c46d78a97da8eb6c5a37e00d5f2fee182dcb38c1b6c65e90a43c109"
            );
        } else {
            panic!("Expected signature")
        }
        if let Some(StackItem::Scalar(s)) = stack.pop() {
            assert_eq!(
                s.as_slice(),
                from_hex("fdf9fc345d2cdd8aff624a55f824c7c9ce3cc972e011b4e750e417a90ecc5da5").unwrap()
            );
        } else {
            panic!("Expected scalar")
        }
    }

    #[test]
    fn serde_serialization_non_breaking() {
        const SERDE_ENCODED_BYTES: &str = "ce0000000000000006fdf9fc345d2cdd8aff624a55f824c7c9ce3cc9\
        72e011b4e750e417a90ecc5da50456c0fa32558d6edc0916baa26b48e745de834571534ca253ea82435f08ebbc\
        7c0556c0fa32558d6edc0916baa26b48e745de834571534ca253ea82435f08ebbc7c6db1023d5c46d78a97da8eb\
        6c5a37e00d5f2fee182dcb38c1b6c65e90a43c10906fdf9fc345d2cdd8aff624a55f824c7c9ce3cc972e011b4e7\
        50e417a90ecc5da501d2040000000000000356c0fa32558d6edc0916baa26b48e745de834571534ca253ea82435\
        f08ebbc7c";
        let p =
            RistrettoPublicKey::from_hex("56c0fa32558d6edc0916baa26b48e745de834571534ca253ea82435f08ebbc7c").unwrap();
        let s =
            RistrettoSecretKey::from_hex("6db1023d5c46d78a97da8eb6c5a37e00d5f2fee182dcb38c1b6c65e90a43c109").unwrap();
        let sig = RistrettoSchnorr::new(p.clone(), s);
        let m: HashValue = Blake2b::<U32>::digest(b"Hello Tari Script").into();
        let s: ScalarValue = m;
        let commitment = PedersenCommitment::from_public_key(&p);

        // Includes all variants for StackItem
        let mut expected_inputs = inputs!(s, p, sig, m, 1234, commitment);
        let stack = ExecutionStack::from_binary(&from_hex(SERDE_ENCODED_BYTES).unwrap()).unwrap();

        for (i, item) in stack.items.into_iter().enumerate().rev() {
            assert_eq!(
                item,
                expected_inputs.pop().unwrap(),
                "Stack items did not match at index {}",
                i
            );
        }

        assert!(expected_inputs.is_empty());
    }

    #[test]
    fn test_borsh_de_serialization() {
        let s = "06fdf9fc345d2cdd8aff624a55f824c7c9ce3cc972e011b4e750e417a90ecc5da50500f7c695528c858cde76dab3076908e0122\
        8b6dbdd5f671bed1b03b89e170c316db1023d5c46d78a97da8eb6c5a37e00d5f2fee182dcb38c1b6c65e90a43c1090456c0fa32558d6edc0916baa2\
        6b48e745de834571534ca253ea82435f08ebbc7c";
        let stack = ExecutionStack::from_hex(s).unwrap();
        let mut buf = Vec::new();
        stack.serialize(&mut buf).unwrap();
        buf.extend_from_slice(&[1, 2, 3]);
        let buf = &mut buf.as_slice();
        assert_eq!(stack, ExecutionStack::deserialize(buf).unwrap());
        assert_eq!(buf, &[1, 2, 3]);
    }

    #[test]
    fn test_borsh_de_serialization_too_large() {
        // We dont care about the actual stack here, just that its not too large on the varint size
        // We lie about the size to try and get a mem panic, and say this stack is u64::max large.
        let buf = vec![255, 255, 255, 255, 255, 255, 255, 255, 255, 1, 49, 8, 2, 5, 6];
        let buf = &mut buf.as_slice();
        assert!(ExecutionStack::deserialize(buf).is_err());
    }
}
