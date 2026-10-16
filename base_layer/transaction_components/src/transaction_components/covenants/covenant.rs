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
    fmt,
    io::{self, Write},
};

use borsh::BorshSerialize;
use integer_encoding::VarIntWriter;
use tari_max_size::{EncodedBytes, ValidatedDecode, impl_validated_decode};
use tari_utilities::hex::Hex;

use super::decoder::CovenantDecodeError;
use crate::transaction_components::{
    TransactionInput,
    TransactionOutput,
    covenants::{
        context::CovenantContext,
        decoder::CovenantTokenDecoder,
        error::CovenantError,
        filters::Filter,
        output_set::OutputSet,
        scan::scan,
        token::{CovenantToken, CovenantTokenCollection},
        work::{OutputHashCache, WorkMeter},
    },
};

/// The maximum encoded size of a covenant.
///
/// The bound is consensus critical: it is applied at decode time, so changing it changes which blocks and transactions
/// a node can decode at all and is a flag-day (hard) fork.
pub(crate) const MAX_COVENANT_BYTES: usize = 4096;

/// The maximum number of tokens in a covenant, counted across all nesting levels: an `ARG_COVENANT` token counts at
/// its own level, and every token inside it counts as well.
///
/// The bound is consensus critical: it is applied at decode time, so changing it changes which blocks and transactions
/// a node can decode at all and is a flag-day (hard) fork.
pub(crate) const MAX_COVENANT_TOKENS: usize = 128;

/// The deepest a covenant may nest another covenant (via `ARG_COVENANT`).
///
/// Decoding a nested covenant recurses, and each nesting level only costs the attacker the byte code plus a length
/// varint. Without this limit a single ~4KB covenant — the size cap is `MAX_COVENANT_BYTES` — nests over a thousand
/// levels deep and overflows the stack of whichever thread is validating it. The limit is set far above anything a
/// real covenant needs so that it bounds the recursion without constraining legitimate use.
pub(crate) const MAX_COVENANT_DEPTH: usize = 16;

/// The most work units that evaluating the covenants of one aggregate body may cost. Each filter, token and output a
/// filter visits costs work units; hashing an output or a set of fields is the most expensive unit.
///
/// The bound is consensus critical: a body whose covenants cost more is invalid, so changing it changes which blocks
/// are valid and is a fork.
pub const MAX_COVENANT_WORK_PER_BODY: u64 = 250_000;

#[derive(Default, Clone, PartialEq, Eq)]
/// A covenant allows a UTXO to specify some restrictions on how it is spent in a future transaction.
/// See https://rfc.tari.com/RFC-0250_Covenants.html for details.
///
/// The covenant is kept as its canonical encoding and is only split into tokens when it is evaluated.
pub struct Covenant {
    /// Canonical encoding, as produced by `scan`. Never more than `MAX_COVENANT_BYTES`.
    bytes: Vec<u8>,
    /// Number of top-level tokens, at most `MAX_COVENANT_TOKENS`.
    num_tokens: u8,
}

impl fmt::Debug for Covenant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Covenant({})", self.bytes.to_hex())
    }
}

impl BorshSerialize for Covenant {
    fn serialize<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        writer.write_varint(self.bytes.len())?;
        writer.write_all(&self.bytes)
    }
}

/// serde: a hex string or a byte array; borsh: the bytes behind a varint length prefix of at most
/// [`MAX_COVENANT_BYTES`]. Both decoders then apply [`Covenant::from_bytes`].
impl ValidatedDecode for Covenant {
    type Error = CovenantDecodeError;
    type Raw = EncodedBytes<MAX_COVENANT_BYTES>;

    fn validate(raw: Self::Raw) -> Result<Self, Self::Error> {
        Self::from_bytes(&mut raw.as_bytes())
    }
}

impl_validated_decode!(Covenant);

impl Covenant {
    pub fn new() -> Self {
        Self::default()
    }

    /// Produces a new `Covenant` instance, out of a byte buffer. It errors
    /// if the byte buffer length is higher than `MAX_COVENANT_BYTES`.
    pub fn from_bytes(bytes: &mut &[u8]) -> Result<Self, CovenantDecodeError> {
        Self::from_bytes_at_depth(bytes, 0)
    }

    /// As [`Covenant::from_bytes`], but tracking how deeply this covenant is nested inside an enclosing one so that
    /// the recursion through `ARG_COVENANT` stays bounded.
    ///
    /// The whole buffer is consumed. The stored bytes are the decoded tokens re-encoded, which is what is hashed.
    pub(super) fn from_bytes_at_depth(bytes: &mut &[u8], depth: usize) -> Result<Self, CovenantDecodeError> {
        let mut out = Vec::with_capacity(bytes.len().min(MAX_COVENANT_BYTES));
        let mut remaining = MAX_COVENANT_TOKENS;
        let num_tokens = scan(bytes, depth, &mut out, &mut remaining)?;
        out.shrink_to_fit();
        *bytes = &[];
        Ok(Self { bytes: out, num_tokens })
    }

    /// Builds a covenant from a sequence of tokens. The encoding goes through the same checks as decoding.
    pub fn from_tokens(tokens: &[CovenantToken]) -> Result<Self, CovenantDecodeError> {
        let mut encoded = Vec::new();
        for token in tokens {
            token.write_to(&mut encoded)?;
        }
        let covenant = Self::from_bytes(&mut encoded.as_slice())?;
        debug_assert_eq!(covenant.bytes, encoded);
        Ok(covenant)
    }

    /// Returns the canonical encoding of this covenant.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.bytes.clone()
    }

    /// Returns the canonical encoding of this covenant.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Writes a `Covenant` instance byte to a writer.
    pub(super) fn write_to<W: io::Write>(&self, writer: &mut W) -> Result<(), io::Error> {
        writer.write_all(&self.bytes)
    }

    /// Gets the byte lenght of the underlying byte buffer
    pub(super) fn get_byte_length(&self) -> usize {
        self.bytes.len()
    }

    /// It executes the covenant on the transaction input being spent, it filters the transaction outputs which should
    /// generate at least one match. An empty covenant is an identity and matches all outputs.
    pub fn execute(
        &self,
        block_height: u64,
        input: &TransactionInput,
        outputs: &[TransactionOutput],
    ) -> Result<usize, CovenantError> {
        if self.is_empty() {
            // Empty covenants always pass
            return Ok(outputs.len());
        }
        let matching = self.matching_outputs(block_height, input, outputs, &mut WorkMeter::unlimited(), None)?;
        Ok(matching.len())
    }

    /// Indexes (ascending, into `outputs`) of the outputs this covenant admits. Err(NoMatchingOutputs) if none. The
    /// work is charged to `meter`. Must not be called on an empty covenant.
    pub(crate) fn matching_outputs(
        &self,
        block_height: u64,
        input: &TransactionInput,
        outputs: &[TransactionOutput],
        meter: &mut WorkMeter,
        hashes: Option<&OutputHashCache>,
    ) -> Result<Vec<u32>, CovenantError> {
        meter.charge(self.num_tokens() as u64)?;
        let tokens = CovenantTokenCollection::from(self.tokens()?);
        meter.charge(outputs.len() as u64)?;
        let mut cx = CovenantContext::with_limits(tokens, input, block_height, *meter, hashes);
        let root = cx.require_next_filter()?;
        let mut output_set = OutputSet::new(outputs);
        root.filter(&mut cx, &mut output_set)?;
        if cx.has_more_tokens() {
            return Err(CovenantError::RemainingTokens);
        }
        if output_set.is_empty() {
            return Err(CovenantError::NoMatchingOutputs);
        }
        let mut indexes = Vec::with_capacity(output_set.len());
        for index in output_set.indexes() {
            indexes.push(u32::try_from(index).map_err(|_| CovenantError::WorkLimitExceeded)?);
        }
        *meter = cx.into_meter();
        Ok(indexes)
    }

    /// Builds a covenant from a sequence of tokens, failing with [`CovenantDecodeError::ExceededMaxTokens`] if there
    /// are more than `MAX_COVENANT_TOKENS`. The iterator is abandoned as soon as the limit is exceeded.
    ///
    /// There is intentionally no `FromIterator` impl: it can not fail, so it would have to silently drop tokens and
    /// the resulting covenant would not match its input.
    pub fn try_from_iter<I: IntoIterator<Item = CovenantToken>>(iter: I) -> Result<Self, CovenantDecodeError> {
        let mut encoded = Vec::new();
        let mut count = 0usize;
        for token in iter {
            count = count.saturating_add(1);
            if count > MAX_COVENANT_TOKENS {
                return Err(CovenantDecodeError::ExceededMaxTokens {
                    max: MAX_COVENANT_TOKENS,
                });
            }
            token.write_to(&mut encoded)?;
        }
        Self::from_bytes(&mut encoded.as_slice())
    }

    /// Appends a `CovenantToken` to this covenant.
    pub fn push_token(&mut self, token: CovenantToken) -> Result<(), CovenantError> {
        let mut encoded = self.bytes.clone();
        token.write_to(&mut encoded).map_err(CovenantDecodeError::from)?;
        *self = Self::from_bytes(&mut encoded.as_slice())?;
        Ok(())
    }

    /// Splits the covenant into its top-level tokens.
    pub(super) fn tokens(&self) -> Result<Vec<CovenantToken>, CovenantError> {
        let mut reader = self.bytes.as_slice();
        let tokens = CovenantTokenDecoder::new(&mut reader, 0).collect::<Result<Vec<_>, _>>()?;
        Ok(tokens)
    }

    /// Outputs the number of top-level tokens.
    pub fn num_tokens(&self) -> usize {
        usize::from(self.num_tokens)
    }

    /// Checks if the covenant is empty.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
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
    fn try_from_iter_accepts_up_to_the_maximum_and_rejects_more() {
        use crate::transaction_components::covenants::token::CovenantToken;

        let covenant =
            Covenant::try_from_iter(std::iter::repeat_n(CovenantToken::identity(), MAX_COVENANT_TOKENS)).unwrap();
        assert_eq!(covenant.num_tokens(), MAX_COVENANT_TOKENS);

        let err = Covenant::try_from_iter(std::iter::repeat_n(CovenantToken::identity(), MAX_COVENANT_TOKENS + 1))
            .unwrap_err();
        assert!(matches!(err, CovenantDecodeError::ExceededMaxTokens {
            max: MAX_COVENANT_TOKENS
        }));

        // An unbounded iterator is abandoned as soon as the limit is exceeded
        let err = Covenant::try_from_iter(std::iter::repeat_with(CovenantToken::identity)).unwrap_err();
        assert!(matches!(err, CovenantDecodeError::ExceededMaxTokens { .. }));
    }

    #[test]
    fn it_rejects_more_tokens_than_the_maximum() {
        // `MaxSizeVec` used to have a `FromIterator` that stopped at the cap rather than failing, so decoding used to
        // silently drop every token past the 128th and accept the covenant. The extra tokens then vanished from the
        // re-serialised form, which is what gets hashed — two different encodings for one covenant.
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
    fn covenant_canonical_bytes_match_previous_encoding() {
        use tari_common_types::types::{CompressedCommitment, CompressedPublicKey, FixedHash};
        use tari_script::script;

        use crate::transaction_components::covenants::{arguments::BytesArg, encoder::CovenantTokenEncoder};

        let hash = FixedHash::from([7u8; 32]);
        let script = script!(Nop Dup Drop).unwrap();
        let next_cov = covenant!(and(identity(), or(field_eq(@field::features_maturity, @uint(42))))).unwrap();
        let covenants = vec![
            covenant!().unwrap(),
            covenant!(identity()).unwrap(),
            covenant!(and(identity(), or(identity()))).unwrap(),
            covenant!(and(identity(), identity())).unwrap(),
            covenant!(absolute_height(@uint(42))).unwrap(),
            covenant!(field_eq(@field::features_output_type, @output_type(Coinbase))).unwrap(),
            covenant!(field_eq(@field::script, @script(script.clone()))).unwrap(),
            covenant!(field_eq(@field::features, @uint(42))).unwrap(),
            covenant!(field_eq(@field::features, @hash(hash))).unwrap(),
            covenant!(field_eq(@field::covenant, @covenant(next_cov.clone()))).unwrap(),
            covenant!(field_eq(@field::sender_offset_public_key, @public_key(CompressedPublicKey::default()))).unwrap(),
            covenant!(field_eq(@field::commitment, @commitment(CompressedCommitment::default()))).unwrap(),
            covenant!(field_eq(@field::features, @bytes(BytesArg::try_from(vec![1u8, 2, 3]).unwrap()))).unwrap(),
            covenant!(fields_hashed_eq(@fields(@field::commitment), @hash(hash))).unwrap(),
            covenant!(fields_preserved(@fields(@field::features_maturity, @field::features_output_type))).unwrap(),
            covenant!(output_hash_eq(@hash(hash))).unwrap(),
            covenant!(not(or(field_eq(@field::features_maturity, @uint(42),), field_eq(@field::script, @script(script.clone()))))).unwrap(),
            covenant!(xor(field_eq(@field::features_maturity, @uint(42),), field_eq(@field::script, @script(script)))).unwrap(),
            covenant!(or(absolute_height(@uint(42),), field_eq(@field::features_maturity, @uint(8)))).unwrap(),
        ];
        for covenant in covenants {
            let mut previous = Vec::new();
            CovenantTokenEncoder::new(&covenant.tokens().unwrap())
                .write_to(&mut previous)
                .unwrap();
            assert_eq!(covenant.to_bytes(), previous);
            assert_eq!(
                Covenant::from_bytes(&mut covenant.to_bytes().as_slice()).unwrap(),
                covenant
            );
        }
    }

    /// Builds a covenant of `identity()` tokens, one entry of `levels` per nesting level (outermost first). Every
    /// level but the innermost ends in an `ARG_COVENANT` token holding the next level.
    fn identity_levels(levels: &[usize]) -> Vec<u8> {
        let mut buf = Vec::new();
        let mut innermost = true;
        for count in levels.iter().rev() {
            let mut level = vec![byte_codes::FILTER_IDENTITY; *count];
            if !innermost {
                level.push(byte_codes::ARG_COVENANT);
                level.write_varint(buf.len()).unwrap();
                level.extend_from_slice(&buf);
            }
            innermost = false;
            buf = level;
        }
        buf
    }

    fn borsh_field(bytes: &[u8]) -> Vec<u8> {
        let mut field = Vec::new();
        field.write_varint(bytes.len()).unwrap();
        field.extend_from_slice(bytes);
        field
    }

    #[test]
    fn covenant_total_token_limit_boundary() {
        // 63 + 1 (ARG_COVENANT) + 64 = 128 tokens in total
        let at_limit = identity_levels(&[63, 64]);
        let covenant = Covenant::from_bytes(&mut at_limit.as_slice()).unwrap();
        assert_eq!(covenant.num_tokens(), 64);
        assert_eq!(covenant.to_bytes(), at_limit);
        let mut buf = Vec::new();
        covenant.serialize(&mut buf).unwrap();
        assert_eq!(Covenant::deserialize(&mut buf.as_slice()).unwrap().to_bytes(), at_limit);

        // Spread over three levels: 40 + 1 + 40 + 1 + 46 = 128
        let three_levels = identity_levels(&[40, 40, 46]);
        assert_eq!(
            Covenant::from_bytes(&mut three_levels.as_slice()).unwrap().to_bytes(),
            three_levels
        );

        for over_limit in [identity_levels(&[63, 65]), identity_levels(&[40, 40, 47])] {
            let err = Covenant::from_bytes(&mut over_limit.as_slice()).unwrap_err();
            assert!(
                matches!(err, CovenantDecodeError::ExceededMaxTokens {
                    max: MAX_COVENANT_TOKENS
                }),
                "{err}"
            );
        }

        let single_level = identity_levels(&[MAX_COVENANT_TOKENS]);
        assert_eq!(
            Covenant::from_bytes(&mut single_level.as_slice()).unwrap().num_tokens(),
            MAX_COVENANT_TOKENS
        );
    }

    #[test]
    fn decode_borsh_covenant_matches_borsh_and_keeps_the_error() {
        use crate::transaction_components::covenants::decode_borsh_covenant;

        let fields = [
            borsh_field(&identity_levels(&[63, 65])),
            borsh_field(&[]),
            borsh_field(&identity_levels(&[63, 64])),
            borsh_field(&[0xff]),
            borsh_field(&vec![byte_codes::FILTER_IDENTITY; 4097]),
            borsh_field(&[byte_codes::ARG_COVENANT, 0x81, 0x00, byte_codes::FILTER_IDENTITY]),
            // A malformed length prefix
            vec![0xff],
        ];
        for field in &fields {
            let typed = decode_borsh_covenant(&mut field.as_slice());
            let borsh = Covenant::deserialize(&mut field.as_slice());
            match (&typed, &borsh) {
                (Ok(a), Ok(b)) => assert_eq!(a, b),
                (Err(a), Err(b)) => assert_eq!(a.to_string(), b.to_string()),
                _ => panic!("decoders disagree on {field:?}: {typed:?} vs {borsh:?}"),
            }
        }

        assert!(matches!(
            decode_borsh_covenant(&mut fields[0].as_slice()),
            Err(CovenantDecodeError::ExceededMaxTokens { .. })
        ));
        for field in &fields[1..] {
            assert!(!matches!(
                decode_borsh_covenant(&mut field.as_slice()),
                Err(CovenantDecodeError::ExceededMaxTokens { .. })
            ));
        }
    }

    #[test]
    fn covenant_output_hash_lookups_are_cached() {
        use tari_common_types::types::FixedHash;

        use crate::transaction_components::covenants::{OutputHashCache, WorkMeter};

        let key_manager = KeyManager::new_random().unwrap();
        let outputs = create_outputs(50, UtxoTestParams::default(), &key_manager);
        let hashes = OutputHashCache::new(outputs.len());
        let mut meter = WorkMeter::unlimited();
        for hash in [outputs[49].hash(), outputs[10].hash(), FixedHash::zero()] {
            let input = create_input(&key_manager);
            let covenant = covenant!(output_hash_eq(@hash(hash))).unwrap();
            let _matching = covenant.matching_outputs(0, &input, &outputs, &mut meter, Some(&hashes));
        }
        assert_eq!(hashes.initialized(), 50);
        assert_eq!(hashes.computed(), 50);
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
