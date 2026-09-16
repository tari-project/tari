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
    io::{self, Write},
    iter::FromIterator,
};

use borsh::{BorshDeserialize, BorshSerialize};
use integer_encoding::{VarIntReader, VarIntWriter};
use tari_max_size::MaxSizeVec;

use super::decoder::CovenantDecodeError;
use crate::{
    helpers::byte_counter::ByteCounter,
    transaction_components::{
        TransactionInput,
        TransactionOutput,
        covenants::{
            context::CovenantContext,
            decoder::CovenantTokenDecoder,
            encoder::CovenantTokenEncoder,
            error::CovenantError,
            filters::Filter,
            output_set::OutputSet,
            token::{CovenantToken, CovenantTokenCollection},
        },
    },
};

const MAX_COVENANT_BYTES: usize = 4096;

pub(crate) const MAX_COVENANT_TOKENS: usize = 128;

/// The deepest a covenant may nest another covenant (via `ARG_COVENANT`).
///
/// Decoding a nested covenant recurses, and each nesting level only costs the attacker the byte code plus a length
/// varint. Without this limit a single ~4KB covenant — the size cap is `MAX_COVENANT_BYTES` — nests over a thousand
/// levels deep and overflows the stack of whichever thread is validating it. The limit is set far above anything a
/// real covenant needs so that it bounds the recursion without constraining legitimate use.
pub(crate) const MAX_COVENANT_DEPTH: usize = 16;

pub(crate) type CovenantTokens = MaxSizeVec<CovenantToken, MAX_COVENANT_TOKENS>;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
/// A covenant allows a UTXO to specify some restrictions on how it is spent in a future transaction.
/// See https://rfc.tari.com/RFC-0250_Covenants.html for details.
pub struct Covenant {
    tokens: CovenantTokens,
}

impl BorshSerialize for Covenant {
    fn serialize<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        let bytes = self.to_bytes();
        writer.write_varint(bytes.len())?;
        for b in &bytes {
            b.serialize(writer)?;
        }
        Ok(())
    }
}

impl BorshDeserialize for Covenant {
    fn deserialize_reader<R>(reader: &mut R) -> Result<Self, io::Error>
    where R: io::Read {
        let len = reader.read_varint()?;
        if len > MAX_COVENANT_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Larger than max covenant bytes".to_string(),
            ));
        }
        let mut data = Vec::with_capacity(len);
        for _ in 0..len {
            data.push(u8::deserialize_reader(reader)?);
        }
        let covenant = Self::from_bytes(&mut data.as_slice())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?;
        Ok(covenant)
    }
}

impl Covenant {
    pub fn new() -> Self {
        Self {
            tokens: CovenantTokens::default(),
        }
    }

    /// Produces a new `Covenant` instance, out of a byte buffer. It errors
    /// if the byte buffer length is higher than `MAX_COVENANT_BYTES`.
    pub fn from_bytes(bytes: &mut &[u8]) -> Result<Self, CovenantDecodeError> {
        Self::from_bytes_at_depth(bytes, 0)
    }

    /// As [`Covenant::from_bytes`], but tracking how deeply this covenant is nested inside an enclosing one so that
    /// the recursion through `ARG_COVENANT` stays bounded.
    pub(super) fn from_bytes_at_depth(bytes: &mut &[u8], depth: usize) -> Result<Self, CovenantDecodeError> {
        if depth > MAX_COVENANT_DEPTH {
            return Err(CovenantDecodeError::ExceededMaxDepth {
                max: MAX_COVENANT_DEPTH,
            });
        }
        if bytes.is_empty() {
            return Ok(Self::new());
        }
        if bytes.len() > MAX_COVENANT_BYTES {
            return Err(CovenantDecodeError::ExceededMaxBytes);
        }

        // Collect into a plain `Vec` and convert: `MaxSizeVec`'s `FromIterator` silently stops at the cap, so
        // collecting straight into `CovenantTokens` would drop every token past the 128th and accept the covenant
        // anyway. That makes the decoded covenant differ from the bytes it came from — two distinct encodings map to
        // the same `Covenant`, and the re-serialised (truncated) form is what gets hashed.
        let tokens = CovenantTokenDecoder::new(bytes, depth).collect::<Result<Vec<_>, _>>()?;
        let tokens = CovenantTokens::try_from(tokens).map_err(|_| CovenantDecodeError::ExceededMaxTokens {
            max: MAX_COVENANT_TOKENS,
        })?;

        // A complete decode consumes the whole buffer; anything left over means the token stream did not describe
        // these bytes and must not be accepted as if it did.
        if !bytes.is_empty() {
            return Err(CovenantDecodeError::TrailingBytes { remaining: bytes.len() });
        }

        Ok(Self { tokens })
    }

    /// Given a `Covenant` instance, it writes its bytes content to a
    /// new byte buffer.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(self.get_byte_length());
        self.write_to(&mut buf).unwrap();
        buf
    }

    /// Writes a `Covenant` instance byte to a writer.
    pub(super) fn write_to<W: io::Write>(&self, writer: &mut W) -> Result<(), io::Error> {
        CovenantTokenEncoder::new(&self.tokens).write_to(writer)
    }

    /// Gets the byte lenght of the underlying byte buffer
    pub(super) fn get_byte_length(&self) -> usize {
        let mut counter = ByteCounter::new();
        self.write_to(&mut counter).unwrap();
        counter.get()
    }

    /// It executes the covenant on the transaction input being spent, it filters the transaction outputs which should
    /// generate at least one match. An empty covenant is an identity and matches all outputs.
    pub fn execute(
        &self,
        block_height: u64,
        input: &TransactionInput,
        outputs: &[TransactionOutput],
    ) -> Result<usize, CovenantError> {
        if self.tokens.is_empty() {
            // Empty covenants always pass
            return Ok(outputs.len());
        }

        let tokens = CovenantTokenCollection::from_iter(self.tokens.clone());
        let mut cx = CovenantContext::new(tokens, input, block_height);
        let root = cx.require_next_filter()?;
        let mut output_set = OutputSet::new(outputs);
        root.filter(&mut cx, &mut output_set)?;
        if cx.has_more_tokens() {
            return Err(CovenantError::RemainingTokens);
        }
        if output_set.is_empty() {
            return Err(CovenantError::NoMatchingOutputs);
        }

        Ok(output_set.len())
    }

    /// Adds a new `CovenantToken` to the current `tokens` vector field.
    pub fn push_token(&mut self, token: CovenantToken) -> Result<(), CovenantError> {
        Ok(self.tokens.push(token)?)
    }

    #[cfg(test)]
    /// Outputs a slice of the instance existing `CovenantToken`'s.
    pub(super) fn tokens(&self) -> &[CovenantToken] {
        &self.tokens
    }

    /// Outputs the length of `tokens` field.
    pub fn num_tokens(&self) -> usize {
        self.tokens.len()
    }

    /// Checks if the `tokens` field is empty.
    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }
}

impl FromIterator<CovenantToken> for Covenant {
    /// Creates a new `CovenantToken` instance from an iterator with `Item = CovenantToken`.
    ///
    /// NOTE: `FromIterator` cannot fail, so anything past `MAX_COVENANT_TOKENS` is silently dropped. Never use this to
    /// build a covenant from untrusted bytes — the result would not match its input. `Covenant::from_bytes` converts
    /// with `TryFrom` for exactly this reason.
    fn from_iter<T: IntoIterator<Item = CovenantToken>>(iter: T) -> Self {
        Self {
            tokens: iter.into_iter().collect(),
        }
    }
}

#[cfg(test)]
mod test {
    #![allow(clippy::indexing_slicing)]
    use borsh::{BorshDeserialize, BorshSerialize};
    use integer_encoding::VarIntWriter;

    use super::{MAX_COVENANT_DEPTH, MAX_COVENANT_TOKENS};
    use crate::{
        covenant,
        key_manager::KeyManager,
        test_helpers::UtxoTestParams,
        transaction_components::covenants::{
            Covenant,
            byte_codes,
            decoder::CovenantDecodeError,
            test::{create_input, create_outputs},
        },
    };

    #[test]
    fn it_succeeds_when_empty() {
        let key_manager = KeyManager::new_random().unwrap();
        let outputs = create_outputs(10, UtxoTestParams::default(), &key_manager);
        let input = create_input(&key_manager);
        let covenant = covenant!().unwrap();
        let num_matching_outputs = covenant.execute(0, &input, &outputs).unwrap();
        assert_eq!(num_matching_outputs, 10);
    }

    #[test]
    fn it_executes_the_covenant() {
        let key_manager = KeyManager::new_random().unwrap();
        let mut outputs = create_outputs(10, UtxoTestParams::default(), &key_manager);
        outputs[4].features.maturity = 42;
        outputs[5].features.maturity = 42;
        outputs[7].features.maturity = 42;
        let mut input = create_input(&key_manager);
        input.set_maturity(42).unwrap();
        let covenant = covenant!(fields_preserved(@fields(
            @field::features_output_type,
            @field::features_maturity))
        )
        .unwrap();
        let num_matching_outputs = covenant.execute(0, &input, &outputs).unwrap();
        assert_eq!(num_matching_outputs, 3);
    }

    #[test]
    fn test_borsh_de_serialization() {
        let key_manager = KeyManager::new_random().unwrap();
        let mut outputs = create_outputs(10, UtxoTestParams::default(), &key_manager);
        outputs[4].features.maturity = 42;
        outputs[5].features.maturity = 42;
        outputs[7].features.maturity = 42;
        let mut input = create_input(&key_manager);
        input.set_maturity(42).unwrap();
        let covenant = covenant!(fields_preserved(@fields(
            @field::features_output_type,
            @field::features_maturity))
        )
        .unwrap();
        let mut buf = Vec::new();
        covenant.serialize(&mut buf).unwrap();
        buf.extend_from_slice(&[1, 2, 3]);
        let buf = &mut buf.as_slice();
        assert_eq!(covenant, Covenant::deserialize(buf).unwrap());
        assert_eq!(buf, &[1, 2, 3]);
    }

    #[test]
    fn test_borsh_de_serialization_too_large() {
        // We dont care about the actual convent here, just that its not too large on the varint size
        // We lie about the size to try and get a mem panic, and say this covenant is u64::max large.
        let buf = vec![255, 255, 255, 255, 255, 255, 255, 255, 255, 1, 49, 8, 2, 5, 6];
        let buf = &mut buf.as_slice();
        assert!(Covenant::deserialize(buf).is_err());
    }

    /// Builds `depth` levels of `covenant(covenant(covenant(... identity() ...)))` as raw bytes.
    ///
    /// Each level costs three bytes — the `ARG_COVENANT` byte code, a one byte length varint and the enclosed
    /// covenant — so the whole thing comfortably fits inside the 4096 byte covenant size limit.
    fn nested_covenant_bytes(depth: usize) -> Vec<u8> {
        // `identity()` is a single filter byte code
        let mut buf = vec![byte_codes::FILTER_IDENTITY];
        for _ in 0..depth {
            let mut next = vec![byte_codes::ARG_COVENANT];
            next.write_varint(buf.len()).unwrap();
            next.extend_from_slice(&buf);
            buf = next;
        }
        buf
    }

    #[test]
    fn it_rejects_deeply_nested_covenants() {
        // A single message under the size limit that nests far deeper than the recursion limit allows. Before the
        // depth limit this recursed once per level and overflowed the stack of the validating thread.
        let bytes = nested_covenant_bytes(1300);
        assert!(
            bytes.len() <= 4096,
            "the payload must stay within the covenant size limit to show that size alone does not bound the recursion"
        );
        let err = Covenant::from_bytes(&mut bytes.as_slice()).unwrap_err();
        assert!(
            matches!(err, CovenantDecodeError::ExceededMaxDepth { .. }),
            "expected a depth error, got {err}"
        );
    }

    #[test]
    fn it_accepts_nesting_up_to_the_depth_limit() {
        Covenant::from_bytes(&mut nested_covenant_bytes(MAX_COVENANT_DEPTH).as_slice()).unwrap();
        let err = Covenant::from_bytes(&mut nested_covenant_bytes(MAX_COVENANT_DEPTH + 1).as_slice()).unwrap_err();
        assert!(matches!(err, CovenantDecodeError::ExceededMaxDepth { .. }));
    }

    #[test]
    fn it_rejects_more_tokens_than_the_maximum() {
        // `MaxSizeVec`'s `FromIterator` stops at the cap rather than failing, so decoding used to silently drop every
        // token past the 128th and accept the covenant. The extra tokens then vanished from the re-serialised form,
        // which is what gets hashed — two different encodings for one covenant.
        let bytes = vec![byte_codes::FILTER_IDENTITY; MAX_COVENANT_TOKENS + 1];
        let err = Covenant::from_bytes(&mut bytes.as_slice()).unwrap_err();
        assert!(
            matches!(err, CovenantDecodeError::ExceededMaxTokens { .. }),
            "expected a token count error, got {err}"
        );

        // Exactly at the limit still decodes, and round-trips to the same bytes
        let covenant =
            Covenant::from_bytes(&mut vec![byte_codes::FILTER_IDENTITY; MAX_COVENANT_TOKENS].as_slice()).unwrap();
        assert_eq!(covenant.num_tokens(), MAX_COVENANT_TOKENS);
        assert_eq!(covenant.to_bytes(), vec![
            byte_codes::FILTER_IDENTITY;
            MAX_COVENANT_TOKENS
        ]);
    }

    #[test]
    fn it_is_not_malleable_by_appending_tokens() {
        // Truncation made these two distinct encodings decode to the same covenant, so an appended token changed the
        // wire bytes without changing the covenant that gets hashed into the output.
        let base = vec![byte_codes::FILTER_IDENTITY; MAX_COVENANT_TOKENS];
        let mut extended = base.clone();
        extended.push(byte_codes::FILTER_IDENTITY);

        let decoded = Covenant::from_bytes(&mut base.as_slice()).unwrap();
        assert_eq!(decoded.to_bytes(), base);
        assert!(Covenant::from_bytes(&mut extended.as_slice()).is_err());
    }
}
