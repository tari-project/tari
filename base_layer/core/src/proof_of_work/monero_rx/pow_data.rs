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
    fmt::{Display, Formatter},
    io,
    io::{Cursor, Write},
};

use borsh::{BorshDeserialize, BorshSerialize};
use monero::{
    VarInt,
    blockdata::transaction::{RawExtraField, TxIn, TxOut},
    consensus::{Decodable, Encodable},
    cryptonote::hash::Hashable,
    util::ringct::{RctSigBase, RctType},
};
use tari_max_size::MaxSizeBytes;
use tari_node_components::blocks::BlockHeader;
use tari_transaction_components::consensus::{
    ConsensusConstants,
    consensus_constants::MAX_MONERO_COINBASE_PREFIX_SIZE,
};
use tari_utilities::{
    ByteArray,
    hex::{Hex, to_hex},
};
use tiny_keccak::{Hasher, Keccak};

use super::{error::MergeMineError, fixed_array::FixedByteArray, merkle_tree::MerkleProof};
use crate::{consensus::BaseNodeConsensusManager, proof_of_work::monero_rx::helpers::create_block_hashing_blob};

// ---------------------------------------------------------------------------------------------------------------
// GHSA-3qmx-q9pv-f3m4: how the Monero coinbase prefix hash gets into a Tari header.
//
// The coinbase prefix hash is what ties a Monero block - and therefore its RandomX solution - to one particular
// Tari header. `is_coinbase_valid_merkle_root` hashes the Monero coinbase transaction prefix together with
// `coinbase_tx_extra` (which is where the Tari merge mining tag lives), rebuilds the coinbase hash from that, and
// checks it against `merkle_root` through `coinbase_merkle_proof`.
//
// Before the fork, `pow_data` carried a *live Keccak sponge* for that hash: a 200-byte state plus `offset`, `rate`
// and `mode`, rebuilt field by field by borsh out of bytes the sender chose, then cloned, fed the extra field and
// finalized. Those 200 bytes are the entire Keccak state, rate and capacity both. The sender was handing the
// verifier a mid-computation state and the verifier was treating it as authoritative.
//
// That is forgeable by arithmetic, not by search. With `offset = 0` the only permutation a short extra field ever
// triggers is the single `fill_block()` after `pad()`, so
//
//     prefix_hash = Keccak-f(B XOR extra@0 XOR 0x01@len(extra) XOR 0x80@135)[0..32]
//
// where `B` is the buffer that arrived on the wire. An attacker fixes the state `A` that reproduces some real
// Monero block's coinbase prefix hash and then, for *any* extra field - including one carrying their own Tari
// merge mining tag - solves `B = A XOR extra XOR pad`. One XOR. No inversion, no collision search. The result is
// a perfectly well-formed sponge: `offset = 0`, `rate = 136`, `mode = Absorbing`. So an attacker takes a real
// Monero block, keeps its genuine header, merkle root and coinbase merkle proof - and with them its genuine
// multi-GH difficulty - and mints Tari blocks at that difficulty having done no work at all.
//
// Constraining `offset`, `rate` and `mode` does not close this: the forgery satisfies all three. Fixing
// `tiny-keccak` does not close it either: the forged state is well formed and the hasher behaves correctly on it.
// The root cause is structural - an attacker-chosen mid-computation state was accepted as authoritative - so the
// fix is structural. From the activation height, `pow_data` carries the raw coinbase transaction prefix bytes
// (`version | unlock_time | inputs | outputs`, the Monero `transaction_prefix` minus its extra field) and the
// verifier builds its own `Keccak::v256()` and absorbs `prefix || extra` itself. No sponge state is on the wire at
// all any more, and the prefix hash depends on `coinbase_tx_extra` by preimage resistance.
//
// The pre-fork shape has to stay parseable forever, because every merge mined block already on chain carries it.
// Which shape is legal is a function of the *height*, not of the bytes, so nothing is added to the wire to tell
// them apart: a discriminant would itself change the pre-fork encoding and invalidate history. Instead
// `from_header` picks the mode from the consensus constants and the mode-aware serialize/deserialize helpers below
// follow it. Below the activation height the legacy shape - and the forgery it permits - stays accepted. That is
// the same grandfathering decision the advisory's Cuckaroo fix made for the mainnet blocks it could not
// retroactively invalidate.
//
// Putting the prefix bytes on the wire raises a second question the sponge never had to answer: where does the
// prefix end and `coinbase_tx_extra` begin? Bounding only the prefix's *length* leaves that split ambiguous, and an
// ambiguous split means one RandomX solution backing several differing Tari headers. The prefix is therefore
// required to be self-delimiting - see `CoinbasePrefix::check_prefix_is_well_formed`, enforced in
// `from_header`, in the `Derived` mode only.
//
// One rule *does* reach backwards over the legacy path, and only one: `check_legacy_sponge_parameters`. A sponge
// with `rate >= 200`, `rate <= 1` or `rate <= offset` makes `tari-tiny-keccak`'s `update()` and `squeeze()`
// `return` without hashing at all, so the prefix hash degenerates to a constant `[0u8; 32]` that does not depend
// on `coinbase_tx_extra` - the tag unbound from the work for free, no arithmetic needed. Requiring
// `rate == 136, offset < rate, mode == Absorbing` at every height is a retroactive tightening, which is normally
// forbidden; it is licensed here by measurement (a full mainnet scan found zero violations) and it is worth being
// exact about how little it buys: the XOR forgery above satisfies all three conditions, so this closes the
// degenerate-state variant and nothing more. See the function for the numbers and the caveats.

/// The raw Monero coinbase transaction prefix carried by a post-fork [`MoneroPowData`].
///
/// The bound is [`MAX_MONERO_COINBASE_PREFIX_SIZE`], derived in `consensus_constants.rs` from what a Monero
/// coinbase prefix can actually be. It is a type-level invariant rather than a runtime check so it holds for every
/// value that exists, including ones decoded straight from a peer.
pub type CoinbaseTxPrefix = MaxSizeBytes<MAX_MONERO_COINBASE_PREFIX_SIZE>;

/// Which of the two coinbase wire formats a [`MoneroPowData`] uses (GHSA-3qmx-q9pv-f3m4).
///
/// Chosen by height, never by content: both formats are raw bytes with no discriminant, so only the consensus
/// constants for the header's height can say which one is legal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoinbasePrefixMode {
    /// Pre-fork: a serialized `tiny_keccak::Keccak` sponge state, taken on trust.
    Legacy,
    /// From the GHSA-3qmx-q9pv-f3m4 activation height: the raw coinbase transaction prefix, from which the
    /// verifier derives the sponge itself.
    Derived,
}

impl CoinbasePrefixMode {
    /// The format a header at `height` must use on this network.
    pub fn for_height(consensus: &BaseNodeConsensusManager, height: u64) -> Self {
        CoinbasePrefixMode::for_constants(consensus.consensus_constants(height))
    }

    /// The format the given constants require. Use this instead of [`CoinbasePrefixMode::for_height`] where the
    /// caller already has the constants for the height in hand, so the lookup only happens once.
    pub fn for_constants(constants: &ConsensusConstants) -> Self {
        if constants.derive_monero_coinbase_hasher() {
            CoinbasePrefixMode::Derived
        } else {
            CoinbasePrefixMode::Legacy
        }
    }
}

impl Display for CoinbasePrefixMode {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> fmt::Result {
        match self {
            CoinbasePrefixMode::Legacy => write!(fmt, "legacy coinbase hasher state"),
            CoinbasePrefixMode::Derived => write!(fmt, "coinbase transaction prefix"),
        }
    }
}

/// Everything of the Monero coinbase transaction that is hashed before `coinbase_tx_extra`, in whichever of the
/// two wire formats the header's height requires. See the note at the top of this file.
#[derive(Clone, Debug)]
pub enum CoinbasePrefix {
    /// Pre-fork: the sender's own Keccak sponge, mid-absorb, used as-is. Kept only so that blocks below the
    /// activation height keep validating bit for bit as they always have.
    Legacy(Keccak),
    /// The raw coinbase transaction prefix. The verifier absorbs this into a fresh sponge of its own.
    Prefix(CoinbaseTxPrefix),
}

impl CoinbasePrefix {
    /// The wire format this value is in.
    pub fn mode(&self) -> CoinbasePrefixMode {
        match self {
            CoinbasePrefix::Legacy(_) => CoinbasePrefixMode::Legacy,
            CoinbasePrefix::Prefix(_) => CoinbasePrefixMode::Derived,
        }
    }

    /// The sponge with everything but the coinbase extra field absorbed, ready to be finalized over it.
    ///
    /// For [`CoinbasePrefix::Prefix`] the state is *derived* here, from bytes, so there is nothing left for a
    /// sender to choose. For [`CoinbasePrefix::Legacy`] it is the sender's own state, cloned - byte for byte what
    /// the pre-fork verifier did, which is what keeps history valid.
    fn seeded_hasher(&self) -> Keccak {
        match self {
            CoinbasePrefix::Legacy(hasher) => hasher.clone(),
            CoinbasePrefix::Prefix(prefix) => {
                let mut hasher = Keccak::v256();
                hasher.update(prefix);
                hasher
            },
        }
    }

    /// Checks everything the wire format this value is in allows to be checked about its own shape.
    ///
    /// The two arms check different things because the two formats *are* different things. For
    /// [`CoinbasePrefix::Legacy`] it is the sponge's `(offset, rate, mode)` - see
    /// [`check_legacy_sponge_parameters`]. For [`CoinbasePrefix::Prefix`] it is the prefix/extra boundary, which
    /// is what the rest of this comment is about.
    ///
    /// # The prefix/extra boundary
    ///
    /// This checks that the boundary between the coinbase prefix and `coinbase_tx_extra` is where the bytes say
    /// it is.
    ///
    /// The hash that ties a Monero block to a Tari header is taken over `prefix || VarInt(len(extra)) || extra`
    /// (see [`MoneroPowData::is_coinbase_valid_merkle_root`]). Bounding only the *length* of the prefix leaves that
    /// concatenation ambiguous: for any `j` with `extra[j] == len(extra) - j - 1`, the split
    ///
    /// ```text
    ///     prefix' = prefix || VarInt(len(extra)) || extra[..j]
    ///     extra'  = extra[j + 1..]
    /// ```
    ///
    /// re-concatenates to the *same byte string*, so it has the same prefix hash, coinbase hash, merkle root and
    /// blockhashing blob - one RandomX solution, two headers. The condition is not exotic: an extra field ending in
    /// a `Nonce` sub-field satisfies it at the nonce's own length byte. The two interpretations can carry different
    /// merge mining tags (`mining_hash()` covers neither `pow` nor `pow_data`, so there is no circularity to stop
    /// it), which hands a miner free equivocation - the same work backing several valid, *differing* Tari headers,
    /// a ready made reorg / selfish mining hedge.
    ///
    /// The fix is to make the prefix self-delimiting: it must decode as a real Monero coinbase transaction prefix -
    /// `version | unlock_time | inputs | outputs`, exactly what `construct_monero_data` encodes - and consume every
    /// byte it was given. Decoding is a deterministic function of the leading bytes, so at most one split point can
    /// satisfy that, and the shifted interpretation above (which leaves `VarInt(len(extra)) || extra[..j]`
    /// unconsumed after a complete prefix) stops being expressible.
    fn check_prefix_is_well_formed(&self) -> Result<(), MergeMineError> {
        match self {
            // The self-delimiting rule above is *not* applied here: the legacy format carries a sponge rather
            // than bytes, so there is nothing to delimit, and the same ambiguity has always been in it - an
            // attacker could equally absorb `prefix || VarInt(n) || extra[..j]` into the sponge they sent.
            // Applying that rule backwards would invalidate history. What is checked here instead is the sponge's
            // own parameters, which is a different and much narrower rule; see
            // [`check_legacy_sponge_parameters`] for why that one *is* safe to apply retroactively.
            CoinbasePrefix::Legacy(hasher) => check_legacy_sponge_parameters(hasher),
            CoinbasePrefix::Prefix(prefix) => check_coinbase_prefix_bytes(prefix.as_ref()),
        }
    }

    /// Writes whichever format this value is in. There is no discriminant: the legacy encoding has to stay exactly
    /// the bytes the pre-fork node wrote.
    fn serialize<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        match self {
            CoinbasePrefix::Legacy(hasher) => BorshSerialize::serialize(hasher, writer),
            CoinbasePrefix::Prefix(prefix) => BorshSerialize::serialize(prefix, writer),
        }
    }

    /// Reads the format `mode` calls for. The bytes cannot choose; see [`CoinbasePrefixMode`].
    fn deserialize_reader<R: io::Read>(reader: &mut R, mode: CoinbasePrefixMode) -> Result<Self, io::Error> {
        match mode {
            CoinbasePrefixMode::Legacy => Ok(CoinbasePrefix::Legacy(BorshDeserialize::deserialize_reader(reader)?)),
            // `CoinbaseTxPrefix` refuses an over-long length before reading any of it, so a peer cannot make the
            // node allocate on the strength of a length prefix alone.
            CoinbasePrefixMode::Derived => Ok(CoinbasePrefix::Prefix(BorshDeserialize::deserialize_reader(reader)?)),
        }
    }
}

/// Bytes of Keccak state the legacy wire format carries: the rate *and* the capacity, all of it.
const KECCAK_STATE_LEN: usize = 200;
/// The borsh encoding of a `tiny_keccak::Keccak` is the 200-byte state (25 little-endian `u64`s) followed by
/// `offset`, `rate` and `mode`, one byte each. `the_legacy_sponge_encoding_is_exactly_203_bytes` pins this, because
/// [`legacy_sponge_params`] reads the last three bytes on the strength of it.
const KECCAK_SERIALIZED_LEN: usize = KECCAK_STATE_LEN + 3;
/// The sponge rate of `Keccak::v256()`, i.e. `200 - 2 * 256 / 8`. Every honest legacy sponge has exactly this,
/// because `Keccak::v256()` is what produced it.
const KECCAK_256_RATE: u8 = 136;
/// `tiny_keccak::Mode::Absorbing` as borsh writes it: `Mode` is `#[repr(u8)]` with `Absorbing = 1`.
const KECCAK_MODE_ABSORBING: u8 = 1;

/// Reads `(offset, rate, mode)` back out of a legacy sponge.
///
/// `tiny_keccak` exposes no accessor for any of the three, but the values are not hidden either: they are the last
/// three bytes of the sponge's borsh encoding, which is exactly what the pre-fork `pow_data` carried on the wire.
/// Re-serializing is therefore reading the same bytes the sender wrote.
///
/// Anything other than a [`KECCAK_SERIALIZED_LEN`]-byte encoding is a hard error rather than a pass. If a future
/// `tari-tiny-keccak` changes the encoding, this must fail loudly instead of silently reading three bytes that no
/// longer mean `(offset, rate, mode)` - or, worse, quietly waving every sponge through.
fn legacy_sponge_params(hasher: &Keccak) -> Result<(u8, u8, u8), MergeMineError> {
    let mut encoded = Vec::with_capacity(KECCAK_SERIALIZED_LEN);
    BorshSerialize::serialize(hasher, &mut encoded).map_err(|e| {
        MergeMineError::DegenerateLegacyCoinbaseHasher(format!("the hasher state would not re-encode: {e}"))
    })?;
    let Some(&[offset, rate, mode]) = encoded.get(KECCAK_STATE_LEN..) else {
        return Err(MergeMineError::DegenerateLegacyCoinbaseHasher(format!(
            "the hasher state encodes to {} bytes, not the {KECCAK_SERIALIZED_LEN} that `(offset, rate, mode)` is \
             read out of - the `tari-tiny-keccak` encoding has changed and this check must be revisited",
            encoded.len()
        )));
    };
    Ok((offset, rate, mode))
}

/// Requires a legacy sponge to have the parameters `Keccak::v256()` produces: `rate == 136`, `offset < rate` and
/// `mode == Absorbing`. **Applied at every height, including below the GHSA-3qmx-q9pv-f3m4 activation height.**
///
/// # What it closes
///
/// `tari-tiny-keccak`'s `update()` and `squeeze()` both open with
///
/// ```text
///     if self.rate >= 200 || self.rate <= 1 || self.rate <= self.offset { return; }
/// ```
///
/// so on such a state they both `return` without hashing anything. A sender who puts one on the wire makes
/// [`MoneroPowData::is_coinbase_valid_merkle_root`] absorb `coinbase_tx_extra` into nothing and finalize into
/// nothing, so the coinbase prefix hash comes out as a constant `[0u8; 32]` that does not depend on
/// `coinbase_tx_extra` at all. The Tari merge mining tag lives in `coinbase_tx_extra`, so that unbinds the tag
/// from the Monero proof of work with no arithmetic whatsoever: one such sponge backs *every* Tari header, for the
/// same Monero solution. `mode == Squeezing` is required for the same reason - an already-squeezing sponge does
/// not absorb what a fresh one would, so it is not a state any honest coinbase hash passed through.
///
/// # Why it is safe to apply retroactively
///
/// Tightening a pre-fork consensus rule is normally forbidden: it can invalidate blocks that are already on
/// chain, which is a chain split, not a fix. It is safe here for one reason only - **it was measured.** A full
/// read-only scan of MainNet history (347,681 headers, 99,595 RandomXM blocks, heights `0..=347_680`)
/// found **zero** blocks violating any of the three conditions. That is not luck: every honest merge miner's
/// sponge comes out of `Keccak::v256()` with an ordinary coinbase prefix absorbed into it, and `rate = 136`,
/// `offset < 136`, `mode = Absorbing` is simply what that leaves behind. There is no honest way to produce
/// anything else.
///
/// The measurement was **MainNet only**. Testnets were not scanned; that was a deliberate, accepted scoping
/// decision, on the grounds that a testnet re-sync is not a chain split worth gating a real hardening rule on.
///
/// # What it does *not* close
///
/// **This is not the fix for GHSA-3qmx-q9pv-f3m4 and must not be mistaken for it.** The advisory's main forgery -
/// `buffer = target_state ^ extra ^ padding`, one XOR, no search - produces a sponge with `rate = 136`,
/// `offset = 0`, `mode = Absorbing`: parameters this check finds perfectly valid, because they *are* perfectly
/// valid. Constraining `(offset, rate, mode)` was the first attempt at the fix and it does not touch that attack.
/// What is closed here is only the *degenerate-state* variant, a strictly weaker cousin. The structural fix is the
/// [`CoinbasePrefix::Prefix`] format - which carries no sponge at all, so there is nothing left to choose - and,
/// for the history that cannot be re-validated, the deep-reorg anchor.
fn check_legacy_sponge_parameters(hasher: &Keccak) -> Result<(), MergeMineError> {
    let (offset, rate, mode) = legacy_sponge_params(hasher)?;
    if rate != KECCAK_256_RATE {
        return Err(MergeMineError::DegenerateLegacyCoinbaseHasher(format!(
            "the sponge rate is {rate}, but a Keccak-256 sponge always has a rate of {KECCAK_256_RATE}"
        )));
    }
    if offset >= rate {
        return Err(MergeMineError::DegenerateLegacyCoinbaseHasher(format!(
            "the sponge offset is {offset}, which is not inside the {rate} byte rate"
        )));
    }
    if mode != KECCAK_MODE_ABSORBING {
        return Err(MergeMineError::DegenerateLegacyCoinbaseHasher(format!(
            "the sponge mode is {mode}, but a sponge that has only absorbed a coinbase prefix is still absorbing \
             (mode {KECCAK_MODE_ABSORBING})"
        )));
    }
    Ok(())
}

/// Decodes one component of a Monero coinbase transaction prefix, naming it if it will not decode.
fn decode_prefix_component<T: Decodable>(cursor: &mut Cursor<&[u8]>, what: &str) -> Result<T, MergeMineError> {
    T::consensus_decode(cursor).map_err(|e| MergeMineError::NonCanonicalCoinbasePrefix(format!("{what}: {e}")))
}

/// Re-encodes one component, for the canonicality comparison in [`check_coinbase_prefix_bytes`].
fn encode_prefix_component<T: Encodable>(value: &T, into: &mut Vec<u8>) -> Result<(), MergeMineError> {
    value
        .consensus_encode(into)
        .map(|_| ())
        .map_err(|e| MergeMineError::NonCanonicalCoinbasePrefix(format!("could not re-encode the prefix: {e}")))
}

/// Decodes a `VarInt`-counted vector the way monero's `impl Decodable for Vec<T>` does, but without believing the
/// count before any of it has been read.
///
/// Monero's own impl calls `Vec::with_capacity(count)` up front, bounded only by its 32 MiB
/// `MAX_VEC_MEM_ALLOC_SIZE`. That is a sane bound for a whole block but not for this path, where a peer supplied
/// prefix of at most [`MAX_MONERO_COINBASE_PREFIX_SIZE`] bytes would otherwise turn a 10-byte VarInt into a 32 MiB
/// allocation on every header validated. Every element costs at least one byte, so a count larger than the bytes
/// remaining cannot be honest and is refused before anything is allocated on it.
///
/// Be precise about what that covers: **this bounds the count of this vector only.** It says nothing about what
/// `T::consensus_decode` goes on to allocate per element, and monero's `Decodable` impls are free to call the
/// blanket `Vec<T>` impl - with its 32 MiB bound - on nested fields. The prefix has exactly two counted vectors
/// and they are handled differently for that reason:
///
///   * inputs - `TxIn::ToKey` holds `key_offsets: Vec<VarInt>`, which is nested and unbounded here: `8 * 4_194_304 ==
///     33_554_432` is exactly `MAX_VEC_MEM_ALLOC_SIZE`, so a count of `4_194_304` passes monero's `>` test and reserves
///     the full 32 MiB off a ~9-byte prefix. That vector is therefore *not* decoded with this function - see
///     [`decode_coinbase_inputs`], which never reaches the `ToKey` branch at all.
///   * outputs - `TxOut` is `amount: VarInt` plus `TxOutTarget`, and `TxOutTarget` is a one-byte tag plus a `[u8; 32]`
///     key and, for `ToTaggedKey`, a `u8` view tag. Every field is fixed width and nothing nests a `Vec`, so the count
///     bound here is the whole bound. An element costs at least 34 bytes, so the vector that is actually built is
///     smaller still.
fn decode_counted_vec<T: Decodable>(cursor: &mut Cursor<&[u8]>, what: &str) -> Result<Vec<T>, MergeMineError> {
    let count = decode_prefix_component::<VarInt>(cursor, what)?.0;
    let remaining = u64::try_from(cursor.get_ref().len())
        .unwrap_or(u64::MAX)
        .saturating_sub(cursor.position());
    if count > remaining {
        return Err(MergeMineError::NonCanonicalCoinbasePrefix(format!(
            "{what}: a count of {count} cannot fit in the {remaining} bytes that are left"
        )));
    }
    let mut items = Vec::new();
    for _ in 0..count {
        items.push(decode_prefix_component::<T>(cursor, what)?);
    }
    Ok(items)
}

/// The tag monero gives a coinbase (`Gen`) input. See `impl Decodable for TxIn` in `monero-0.21.0`.
const MONERO_COINBASE_INPUT_TAG: u8 = 0xff;

/// Decodes the input vector of a Monero *coinbase* transaction, which is always exactly one `TxIn::Gen`.
///
/// This deliberately does not go through `TxIn::consensus_decode`. That function dispatches on a leading tag byte
/// and, for `0x02` (`ToKey`), decodes `key_offsets: Vec<VarInt>` with monero's blanket `impl Decodable for
/// Vec<T>`, which reserves `Vec::with_capacity(count)` up front bounded only by `MAX_VEC_MEM_ALLOC_SIZE`. For
/// `VarInt` that bound is `32 MiB / 8 == 4_194_304` elements and the check is `>`, so a count of exactly
/// `4_194_304` passes and reserves the full 32 MiB. The whole payload needed for that is nine bytes -
/// `02 00 01 02 00 80 80 80 02` - well inside [`MAX_MONERO_COINBASE_PREFIX_SIZE`], and it would run on every
/// peer supplied header. The allocation happens *inside* `consensus_decode`, before any value comes back that
/// could be inspected, so rejecting `ToKey` after the fact is too late: the tag has to be refused before the
/// decoder is handed the rest.
///
/// Requiring one `Gen` input is not just a bound, it is the correct rule. A Monero coinbase has exactly one input
/// and it is always `Gen` - that is what makes it a coinbase - and `construct_monero_data` encodes
/// `block.miner_tx.prefix.inputs` straight from a Monero block, so honest data always satisfies this. Tightening
/// the accepted format here costs nothing and removes the `ToKey` decoder from the attack surface entirely.
fn decode_coinbase_inputs(cursor: &mut Cursor<&[u8]>) -> Result<Vec<TxIn>, MergeMineError> {
    let count = decode_prefix_component::<VarInt>(cursor, "inputs")?.0;
    if count != 1 {
        return Err(MergeMineError::NonCanonicalCoinbasePrefix(format!(
            "a Monero coinbase has exactly one input, but {count} were encoded"
        )));
    }
    let tag = decode_prefix_component::<u8>(cursor, "input tag")?;
    if tag != MONERO_COINBASE_INPUT_TAG {
        return Err(MergeMineError::NonCanonicalCoinbasePrefix(format!(
            "a Monero coinbase's only input must be a `Gen` input (tag {MONERO_COINBASE_INPUT_TAG:#04x}), but the tag \
             was {tag:#04x}"
        )));
    }
    let height = decode_prefix_component::<VarInt>(cursor, "input height")?;
    Ok(vec![TxIn::Gen { height }])
}

/// The self-delimiting check itself: `bytes` must be exactly one canonically encoded Monero coinbase transaction
/// prefix. See [`CoinbasePrefix::check_prefix_is_well_formed`] for why that is what pins the boundary.
///
/// Decoding alone is not enough - a decoder that accepts several spellings of one value would put several byte
/// strings back in play for the same logical prefix - so the decoded value is re-encoded and compared byte for
/// byte, the same way [`MoneroPowData::from_header`] does for the whole `pow_data`. Monero's `VarInt` decoder does
/// already reject the only non-minimal spelling its format admits, a zero group in any position but the first
/// (`monero-0.21.0`, `impl Decodable for VarInt`); `non_canonical_varints_are_rejected` pins that rather than
/// assuming it. The comparison stays as the thing that is actually load bearing, so this does not depend on a
/// third party decoder keeping that property.
fn check_coinbase_prefix_bytes(bytes: &[u8]) -> Result<(), MergeMineError> {
    let mut cursor = Cursor::new(bytes);
    // The same four components, in the same order, that `construct_monero_data` encodes.
    let version = decode_prefix_component::<VarInt>(&mut cursor, "version")?;
    let unlock_time = decode_prefix_component::<VarInt>(&mut cursor, "unlock time")?;
    // Not `decode_counted_vec`: the input vector is the one place a nested unbounded allocation is reachable.
    // See `decode_coinbase_inputs`.
    let inputs = decode_coinbase_inputs(&mut cursor)?;
    let outputs = decode_counted_vec::<TxOut>(&mut cursor, "outputs")?;

    // Nothing may be left over. This is the boundary check: bytes after a complete prefix are exactly what the
    // shifted interpretation needs somewhere to put `VarInt(len(extra)) || extra[..j]`.
    let consumed = usize::try_from(cursor.position()).unwrap_or(usize::MAX);
    if consumed != bytes.len() {
        return Err(MergeMineError::NonCanonicalCoinbasePrefix(format!(
            "a complete coinbase prefix ends after {consumed} bytes but {} bytes were supplied",
            bytes.len()
        )));
    }

    let mut re_encoded = Vec::with_capacity(bytes.len());
    encode_prefix_component(&version, &mut re_encoded)?;
    encode_prefix_component(&unlock_time, &mut re_encoded)?;
    encode_prefix_component(&inputs, &mut re_encoded)?;
    encode_prefix_component(&outputs, &mut re_encoded)?;
    if re_encoded.as_slice() != bytes {
        return Err(MergeMineError::NonCanonicalCoinbasePrefix(
            "the bytes are not the canonical encoding of the coinbase prefix they decode to".to_string(),
        ));
    }

    Ok(())
}

/// This is a struct to deserialize the data from the pow field into data required for the randomX Monero merged mine
/// pow.
#[derive(Clone, Debug)]
pub struct MoneroPowData {
    /// Monero header fields
    pub header: monero::BlockHeader,
    /// RandomX vm key - the key length varies to a maximum length of 60. We'll allow a up to 63 bytes represented in
    /// fixed 64-byte struct (63 bytes + 1-byte length).
    pub randomx_key: FixedByteArray,
    /// The number of transactions included in this Monero block. This is used to produce the blockhashing_blob
    pub transaction_count: u16,
    /// Transaction root
    pub merkle_root: monero::Hash,
    /// Coinbase merkle proof hashes
    pub coinbase_merkle_proof: MerkleProof,
    /// The part of the coinbase transaction that is hashed before the extra field, in the wire format this
    /// header's height requires (GHSA-3qmx-q9pv-f3m4)
    pub coinbase_prefix: CoinbasePrefix,
    /// extra field of the coinbase
    pub coinbase_tx_extra: RawExtraField,
    /// aux chain merkle proof hashes
    pub aux_chain_merkle_proof: MerkleProof,
}

impl BorshSerialize for MoneroPowData {
    /// Serialization needs no mode: the variant held by `coinbase_prefix` *is* the mode, and it was chosen from the
    /// header's height when the value was built or decoded. That is what makes the re-serialize canonicality check
    /// in [`MoneroPowData::from_header`] compare like with like.
    fn serialize<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        self.header.consensus_encode(writer)?;
        BorshSerialize::serialize(&self.randomx_key, writer)?;
        BorshSerialize::serialize(&self.transaction_count, writer)?;
        self.merkle_root.consensus_encode(writer)?;
        BorshSerialize::serialize(&self.coinbase_merkle_proof, writer)?;
        self.coinbase_prefix.serialize(writer)?;
        BorshSerialize::serialize(&self.coinbase_tx_extra.0, writer)?;
        BorshSerialize::serialize(&self.aux_chain_merkle_proof, writer)?;
        Ok(())
    }
}

impl MoneroPowData {
    /// Deserializes pow data in the given wire format.
    ///
    /// There is deliberately no `BorshDeserialize` impl: which format is legal depends on the header's height and
    /// nothing on the wire says which one it is, so a caller that cannot name a mode cannot safely decode.
    pub fn deserialize_with_mode<R: io::Read>(
        reader: &mut R,
        mode: CoinbasePrefixMode,
    ) -> Result<MoneroPowData, io::Error> {
        let header = monero::BlockHeader::consensus_decode(reader)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        let randomx_key = BorshDeserialize::deserialize_reader(reader)?;
        let transaction_count = BorshDeserialize::deserialize_reader(reader)?;
        let merkle_root = monero::Hash::consensus_decode(reader)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        let coinbase_merkle_proof = BorshDeserialize::deserialize_reader(reader)?;
        let coinbase_prefix = CoinbasePrefix::deserialize_reader(reader, mode)?;
        let coinbase_tx_extra = RawExtraField(BorshDeserialize::deserialize_reader(reader)?);
        let aux_chain_merkle_proof = BorshDeserialize::deserialize_reader(reader)?;
        Ok(Self {
            header,
            randomx_key,
            transaction_count,
            merkle_root,
            coinbase_merkle_proof,
            coinbase_prefix,
            coinbase_tx_extra,
            aux_chain_merkle_proof,
        })
    }

    /// Create a new MoneroPowData struct from the given header, using the consensus rules that apply at the
    /// header's own height.
    ///
    /// This is what consensus validation wants: by the time a header is validated it is linked to a parent, so its
    /// height is a fact about the chain rather than a claim. Callers that are looking at an *unlinked* header -
    /// the gossip pre-validation gate - must use [`MoneroPowData::from_header_at_rules_height`] instead and pass a
    /// height they can corroborate.
    pub fn from_header(
        tari_header: &BlockHeader,
        consensus: &BaseNodeConsensusManager,
    ) -> Result<MoneroPowData, MergeMineError> {
        Self::from_header_at_rules_height(tari_header, consensus, tari_header.height)
    }

    /// As [`MoneroPowData::from_header`], but selects the consensus constants from `rules_height` rather than from
    /// the header's own `height` field.
    ///
    /// Only for callers holding a header that is not yet linked to the chain, where `tari_header.height` is a
    /// peer's unverified assertion and choosing rules from it lets the peer choose its own rules. Pass a height
    /// the node can corroborate.
    pub fn from_header_at_rules_height(
        tari_header: &BlockHeader,
        consensus: &BaseNodeConsensusManager,
        rules_height: u64,
    ) -> Result<MoneroPowData, MergeMineError> {
        let constants = consensus.consensus_constants(rules_height);
        // GHSA-3qmx-q9pv-f3m4. The height alone decides which coinbase wire format this header is allowed to be
        // in; see the note at the top of this file. Doing it here covers all three call paths into `MoneroPowData`
        // - `check_target_difficulty` (header validation), `check_monero_seed_height` (body validation) and
        // `insert_header` - in one place.
        let mode = CoinbasePrefixMode::for_constants(constants);
        let mut v = tari_header.pow.pow_data.as_bytes();
        let pow_data = MoneroPowData::deserialize_with_mode(&mut v, mode).map_err(|e| {
            MergeMineError::DeserializeError(format!("{e:?} (expected the {mode} format at height {rules_height})"))
        })?;
        if pow_data.coinbase_tx_extra.0.len() > constants.max_extra_field_size() {
            return Err(MergeMineError::DeserializeError(format!(
                "Extra size({}) is larger than allowed {} bytes",
                pow_data.coinbase_tx_extra.0.len(),
                constants.max_extra_field_size()
            )));
        }
        if !v.is_empty() {
            return Err(MergeMineError::DeserializeError(format!(
                "{} bytes leftover after deserialize",
                v.len()
            )));
        }
        // Whatever the format this height's rules put on the wire allows to be checked about its own shape.
        //
        // For the derived format that is the prefix/extra boundary, which has to be unambiguous or one RandomX
        // solution backs several differing Tari headers. For the legacy format it is the sponge's
        // `(offset, rate, mode)`, which is checked at *every* height, this one included - see
        // `check_legacy_sponge_parameters` for why a retroactive rule is warranted there and what it does and does
        // not close.
        //
        // Enforced here rather than in `CoinbasePrefix::deserialize_reader` for two reasons: this is the single
        // funnel every validation path goes through (`verify_header`, `block_body_full_validator` and
        // `insert_header` all reach `MoneroPowData` only via `from_header`), and a `MergeMineError` carries a ban
        // reason where an `io::Error` from the deserializer cannot.
        pow_data.coinbase_prefix.check_prefix_is_well_formed()?;
        let mut test_serialized_data = vec![];

        // This is an inefficient test, so maybe it can be removed in future, but because we rely
        // on third party parsing libraries, there could be a case where the data we deserialized
        // can be generated from multiple input data. This way we test that there is only one of those
        // inputs that is allowed. Remember that the data in powdata is used for the hash, so having
        // multiple pow_data that generate the same randomx difficulty could be a problem.
        //
        // This re-serializes in the mode it just deserialized in, because `coinbase_prefix` carries the variant
        // that `mode` selected. Comparing a legacy payload against a re-serialized post-fork one would reject
        // every header ever written.
        BorshSerialize::serialize(&pow_data, &mut test_serialized_data)
            .map_err(|e| MergeMineError::SerializeError(format!("{e:?}")))?;
        if test_serialized_data != tari_header.pow.pow_data.to_vec() {
            return Err(MergeMineError::SerializedPowDataDoesNotMatch(
                "Serialized pow data does not match original pow data".to_string(),
            ));
        }

        Ok(pow_data)
    }

    /// Returns true if the coinbase merkle proof produces the `merkle_root` hash, otherwise false
    pub fn is_coinbase_valid_merkle_root(&self) -> bool {
        // Legacy: the sender's sponge, cloned - exactly what the pre-fork verifier did.
        // Post-fork: a fresh `Keccak::v256()` with the coinbase prefix bytes absorbed, so the state is derived
        // here rather than accepted from the wire (GHSA-3qmx-q9pv-f3m4).
        let mut finalised_prefix_keccak = self.coinbase_prefix.seeded_hasher();
        let mut encoder_extra_field = Vec::new();
        // Encoding into a `Vec` cannot actually fail, but an unprovable hash is never a valid one, so answer `false`
        // rather than panicking on a path that runs while validating peer supplied headers.
        if self
            .coinbase_tx_extra
            .consensus_encode(&mut encoder_extra_field)
            .is_err()
        {
            return false;
        }
        finalised_prefix_keccak.update(&encoder_extra_field);
        let mut prefix_hash: [u8; 32] = [0; 32];
        finalised_prefix_keccak.finalize(&mut prefix_hash);

        let final_prefix_hash = monero::Hash::from_slice(&prefix_hash);

        // let mut finalised_keccak = Keccak::v256();
        let rct_sig_base = RctSigBase {
            rct_type: RctType::Null,
            txn_fee: Default::default(),
            pseudo_outs: vec![],
            ecdh_info: vec![],
            out_pk: vec![],
        };
        let hashes = vec![final_prefix_hash, rct_sig_base.hash(), monero::Hash::null()];
        let encoder_final: Vec<u8> = hashes.into_iter().flat_map(|h| Vec::from(&h.to_bytes()[..])).collect();
        let coinbase_hash = monero::Hash::new(encoder_final);

        let merkle_root = self.coinbase_merkle_proof.calculate_root(&coinbase_hash);
        (self.merkle_root == merkle_root) && self.coinbase_merkle_proof.check_coinbase_path()
    }

    /// Returns the blockhashing_blob for the Monero block
    pub fn to_blockhashing_blob(&self) -> Vec<u8> {
        create_block_hashing_blob(&self.header, &self.merkle_root, u64::from(self.transaction_count))
    }

    /// Returns the RandomX vm key
    pub fn randomx_key(&self) -> &[u8] {
        self.randomx_key.as_slice()
    }
}

impl Display for MoneroPowData {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> fmt::Result {
        writeln!(fmt, "MoneroBlockHeader: {} ", self.header)?;
        writeln!(fmt, "RandomX vm key: {}", self.randomx_key.to_hex())?;
        writeln!(fmt, "Monero tx count: {}", self.transaction_count)?;
        writeln!(fmt, "Coinbase format: {}", self.coinbase_prefix.mode())?;
        writeln!(fmt, "Monero tx root: {}", to_hex(self.merkle_root.as_bytes()))
    }
}

#[cfg(test)]
mod test {
    use borsh::{BorshDeserialize, BorshSerialize};
    use monero::{
        BlockHeader,
        Hash,
        VarInt,
        blockdata::transaction::{ExtraField, RawExtraField, SubField, TxIn, TxOut, TxOutTarget},
        consensus::{Decodable, Encodable},
    };
    use tari_common::configuration::Network;
    use tari_common_types::types::PrivateKey;
    use tari_crypto::keys::SecretKey;
    use tari_node_components::blocks::BlockHeader as TariBlockHeader;
    use tari_transaction_components::{
        consensus::{
            NetworkConsensus,
            consensus_constants::{
                ESMERALDA_DERIVED_MONERO_COINBASE_ACTIVATION_HEIGHT,
                MAX_MONERO_COINBASE_PREFIX_SIZE,
                UNSCHEDULED_ACTIVATION_HEIGHT,
            },
        },
        tari_proof_of_work::{PowAlgorithm, PowData, ProofOfWork},
    };
    use tari_utilities::ByteArray;
    use tiny_keccak::{Hasher, Keccak, Mode};

    use super::{
        CoinbasePrefix,
        CoinbasePrefixMode,
        KECCAK_256_RATE,
        KECCAK_MODE_ABSORBING,
        KECCAK_SERIALIZED_LEN,
        KECCAK_STATE_LEN,
        MoneroPowData,
    };
    use crate::{
        consensus::BaseNodeConsensusManager,
        proof_of_work::monero_rx::{FixedByteArray, MergeMineError, merkle_tree::MerkleProof},
    };

    // -----------------------------------------------------------------------------------------------------------
    // Building blocks
    // -----------------------------------------------------------------------------------------------------------

    /// `version | unlock_time | inputs | outputs` for a minimal Monero coinbase - the bytes the post-fork wire
    /// format carries and the pre-fork one absorbed into a sponge before sending.
    ///
    /// `monero::Transaction::default()` has an *empty* input vector, which no real coinbase ever has, so the single
    /// `TxIn::Gen` is put back: `check_coinbase_prefix_bytes` requires exactly one `Gen` input, and a fixture that
    /// did not have one would be testing the fixture rather than the format.
    fn default_coinbase_prefix() -> Vec<u8> {
        let mut coinbase: monero::Transaction = Default::default();
        coinbase.prefix.inputs = vec![TxIn::Gen { height: VarInt(0) }];
        let mut encoded = Vec::new();
        coinbase.prefix.version.consensus_encode(&mut encoded).unwrap();
        coinbase.prefix.unlock_time.consensus_encode(&mut encoded).unwrap();
        coinbase.prefix.inputs.consensus_encode(&mut encoded).unwrap();
        coinbase.prefix.outputs.consensus_encode(&mut encoded).unwrap();
        encoded
    }

    /// A coinbase prefix of the shape a real Monero block has: a version, a timelock, the single `TxIn::Gen` every
    /// coinbase carries, and `outputs` tagged-key outputs. `construct_monero_data` encodes exactly these four
    /// values, in this order.
    fn realistic_coinbase_prefix(outputs: usize) -> Vec<u8> {
        let version = VarInt(2);
        let unlock_time = VarInt(3_000_060);
        let inputs = vec![TxIn::Gen {
            height: VarInt(3_000_000),
        }];
        let outputs = (0..outputs)
            .map(|i| TxOut {
                amount: VarInt(600_000_000_000),
                target: TxOutTarget::ToTaggedKey {
                    key: [u8::try_from(i % 251).expect("a remainder below 251 fits in a u8"); 32],
                    view_tag: 0x5a,
                },
            })
            .collect::<Vec<_>>();
        let mut encoded = Vec::new();
        version.consensus_encode(&mut encoded).unwrap();
        unlock_time.consensus_encode(&mut encoded).unwrap();
        inputs.consensus_encode(&mut encoded).unwrap();
        outputs.consensus_encode(&mut encoded).unwrap();
        encoded
    }

    fn encoded_extra(extra: &RawExtraField) -> Vec<u8> {
        let mut encoded = Vec::new();
        extra.consensus_encode(&mut encoded).unwrap();
        encoded
    }

    fn encoded_subfield(subfield: &SubField) -> Vec<u8> {
        let mut encoded = Vec::new();
        subfield.consensus_encode(&mut encoded).unwrap();
        encoded
    }

    fn encoded_varint(value: usize) -> Vec<u8> {
        let mut encoded = Vec::new();
        VarInt(u64::try_from(value).expect("test values are small"))
            .consensus_encode(&mut encoded)
            .unwrap();
        encoded
    }

    /// Every merge mining root an extra field parses to, the way `verify_header` reads it: a partial parse still
    /// counts, and one tag is what it demands.
    fn merge_mining_roots(extra: &RawExtraField) -> Vec<Hash> {
        let parsed = ExtraField::try_parse(extra).unwrap_or_else(|partial| partial);
        parsed
            .0
            .into_iter()
            .filter_map(|field| match field {
                SubField::MergeMining(_, root) => Some(root),
                _ => None,
            })
            .collect()
    }

    /// The pre-fork wire value: a sponge with `absorbed` already in it.
    fn legacy_sponge(absorbed: &[u8]) -> Keccak {
        let mut keccak = Keccak::v256();
        keccak.update(absorbed);
        keccak
    }

    /// Rebuilds a sponge from a raw `(state, offset, rate, mode)` through borsh.
    ///
    /// `tiny_keccak` exposes no constructor for this, but a peer does not need one: the borsh encoding of a
    /// `Keccak` is the 200-byte state as 25 little-endian `u64`s, then `offset`, `rate` and `mode`, and that is
    /// precisely what the pre-fork `pow_data` carried. So this is the attacker's path, byte for byte.
    fn keccak_from_state(state: &[u8; KECCAK_STATE_LEN], offset: u8, rate: u8, mode: Mode) -> Keccak {
        let mut encoded = Vec::with_capacity(KECCAK_STATE_LEN + 3);
        encoded.extend_from_slice(state);
        encoded.push(offset);
        encoded.push(rate);
        encoded.push(mode as u8);
        Keccak::deserialize(&mut encoded.as_slice()).unwrap()
    }

    /// The Keccak state that finalizing over `absorbed` permutes: `absorbed` at offset 0, the `0x01` domain
    /// separator at `absorbed.len()` and the `0x80` end marker at `rate - 1`. Only valid while everything still
    /// fits in one block, which is all any of these tests need.
    fn padded_state(absorbed: &[u8]) -> [u8; KECCAK_STATE_LEN] {
        assert!(
            absorbed.len() < usize::from(KECCAK_256_RATE),
            "test data must fit in a single block"
        );
        let mut state = [0u8; KECCAK_STATE_LEN];
        for (slot, byte) in state.iter_mut().zip(absorbed.iter()) {
            *slot ^= *byte;
        }
        *state
            .get_mut(absorbed.len())
            .expect("the assert above keeps the delimiter inside the state") ^= 0x01;
        *state
            .get_mut(usize::from(KECCAK_256_RATE).saturating_sub(1))
            .expect("the rate is inside the state") ^= 0x80;
        state
    }

    /// GHSA-3qmx-q9pv-f3m4, the forgery itself.
    ///
    /// With `offset = 0` the only permutation a short extra field triggers is the single one at `finalize`, so the
    /// state that gets permuted is `buffer ^ extra ^ padding`. Solving for the buffer is one XOR - no inversion,
    /// no collision search - and yields a sponge that reproduces `target_state`, and therefore whatever prefix
    /// hash `target_state` produces, for *any* extra field at all.
    fn xor_forged_sponge(target_state: &[u8; KECCAK_STATE_LEN], extra: &RawExtraField) -> Keccak {
        let mut buffer = padded_state(&encoded_extra(extra));
        for (slot, byte) in buffer.iter_mut().zip(target_state.iter()) {
            *slot ^= *byte;
        }
        keccak_from_state(&buffer, 0, KECCAK_256_RATE, Mode::Absorbing)
    }

    fn pow_data_with(coinbase_prefix: CoinbasePrefix, extra: RawExtraField) -> MoneroPowData {
        MoneroPowData {
            header: BlockHeader {
                major_version: VarInt(1),
                minor_version: VarInt(2),
                timestamp: VarInt(3),
                prev_id: Hash::new([4; 32]),
                nonce: 5,
            },
            randomx_key: FixedByteArray::from_canonical_bytes(&[6, 7, 8]).unwrap(),
            transaction_count: 9,
            merkle_root: Hash::new([10; 32]),
            coinbase_merkle_proof: MerkleProof::default(),
            coinbase_tx_extra: extra,
            coinbase_prefix,
            aux_chain_merkle_proof: MerkleProof::default(),
        }
    }

    /// A Tari header at `height` carrying `pow_data`, serialized the way a peer would send it.
    fn header_carrying(pow_data: &MoneroPowData, height: u64) -> TariBlockHeader {
        let mut serialized = Vec::new();
        pow_data.serialize(&mut serialized).unwrap();
        let mut header = TariBlockHeader::new(0);
        header.height = height;
        header.pow = ProofOfWork {
            pow_algo: PowAlgorithm::RandomXM,
            pow_data: PowData::try_from(serialized).unwrap(),
        };
        header
    }

    /// The prefix hash `is_coinbase_valid_merkle_root` computes - the value that ties the Monero coinbase, and so
    /// the RandomX solution, to this Tari header.
    fn prefix_hash(pow_data: &MoneroPowData) -> [u8; 32] {
        let mut hasher = pow_data.coinbase_prefix.seeded_hasher();
        hasher.update(&encoded_extra(&pow_data.coinbase_tx_extra));
        let mut out = [0u8; 32];
        hasher.finalize(&mut out);
        out
    }

    fn esmeralda() -> BaseNodeConsensusManager {
        BaseNodeConsensusManager::builder(Network::Esmeralda).build().unwrap()
    }

    // -----------------------------------------------------------------------------------------------------------
    // Wire format round trips
    // -----------------------------------------------------------------------------------------------------------

    #[test]
    fn test_borsh_de_serialization() {
        let coinbase: monero::Transaction = Default::default();
        let monero_pow_data = pow_data_with(
            CoinbasePrefix::Prefix(default_coinbase_prefix().try_into().unwrap()),
            coinbase.prefix.extra.clone(),
        );
        let mut buf = Vec::new();
        monero_pow_data.serialize(&mut buf).unwrap();
        buf.extend_from_slice(&[1, 2, 3]);
        let buf = &mut buf.as_slice();
        MoneroPowData::deserialize_with_mode(buf, CoinbasePrefixMode::Derived).unwrap();
        assert_eq!(buf, &[1, 2, 3]);
    }

    #[test]
    fn test_borsh_de_serialization_legacy() {
        let coinbase: monero::Transaction = Default::default();
        let monero_pow_data = pow_data_with(
            CoinbasePrefix::Legacy(legacy_sponge(&default_coinbase_prefix())),
            coinbase.prefix.extra.clone(),
        );
        let mut buf = Vec::new();
        monero_pow_data.serialize(&mut buf).unwrap();
        buf.extend_from_slice(&[1, 2, 3]);
        let buf = &mut buf.as_slice();
        MoneroPowData::deserialize_with_mode(buf, CoinbasePrefixMode::Legacy).unwrap();
        assert_eq!(buf, &[1, 2, 3]);
    }

    #[test]
    fn max_monero_pow_data_bytes_fits_inside_proof_of_work_pow_data() {
        // A merkle proof is at most `MAX_MERKLE_TREE_PROOF_SIZE - 1` hashes, plus its length varint and path
        // bitmap. The proofs below are empty, so allow for two maximal ones on top of what is measured.
        const MAX_MERKLE_PROOF_BYTES: usize = 1 + 31 * 32 + 4;

        for network in [
            Network::MainNet,
            Network::StageNet,
            Network::LocalNet,
            Network::NextNet,
            Network::Igor,
            Network::Esmeralda,
        ] {
            for consensus_constants in NetworkConsensus::from(network).create_consensus_constants() {
                for coinbase_prefix in [
                    CoinbasePrefix::Legacy(legacy_sponge(&default_coinbase_prefix())),
                    CoinbasePrefix::Prefix(vec![7u8; MAX_MONERO_COINBASE_PREFIX_SIZE].try_into().unwrap()),
                ] {
                    let mut monero_pow_data = pow_data_with(
                        coinbase_prefix,
                        RawExtraField(vec![1u8; consensus_constants.max_extra_field_size()]),
                    );
                    monero_pow_data.header = BlockHeader {
                        major_version: VarInt(u64::MAX),
                        minor_version: VarInt(u64::MAX),
                        timestamp: VarInt(u64::MAX),
                        prev_id: Hash::new(PrivateKey::random(&mut rand::rng()).to_vec()),
                        nonce: u32::MAX,
                    };
                    // The longest key the deserializer will accept, so that this fixture really is the worst
                    // case: every other field here is maximal, and a default (empty) key understated the
                    // bound by up to `MAX_ARR_SIZE` bytes.
                    monero_pow_data.randomx_key = FixedByteArray::from_canonical_bytes(&[1u8; 60]).unwrap();
                    monero_pow_data.transaction_count = u16::MAX;
                    monero_pow_data.merkle_root = Hash::new(PrivateKey::random(&mut rand::rng()).to_vec());

                    let mut buf = Vec::new();
                    monero_pow_data.serialize(&mut buf).unwrap();
                    assert!(
                        buf.len().saturating_add(2 * MAX_MERKLE_PROOF_BYTES) <= PowData::default().max_size(),
                        "{network} pow data does not fit"
                    );
                }
            }
        }
    }

    /// The canonicality check in `from_header` re-serializes what it deserialized and demands the bytes match. It
    /// has to do that in the mode it read in, or it would reject every header ever written. Both modes, both sides
    /// of the fork.
    #[test]
    fn pow_data_round_trips_byte_for_byte_in_both_formats() {
        let rules = esmeralda();
        let extra = RawExtraField(vec![1, 2, 3]);

        let legacy = pow_data_with(
            CoinbasePrefix::Legacy(legacy_sponge(&default_coinbase_prefix())),
            extra.clone(),
        );
        for height in [0, ESMERALDA_DERIVED_MONERO_COINBASE_ACTIVATION_HEIGHT.saturating_sub(1)] {
            let header = header_carrying(&legacy, height);
            let decoded = MoneroPowData::from_header(&header, &rules).unwrap();
            assert_eq!(decoded.coinbase_prefix.mode(), CoinbasePrefixMode::Legacy);
            let mut re_serialized = Vec::new();
            decoded.serialize(&mut re_serialized).unwrap();
            assert_eq!(re_serialized, header.pow.pow_data.to_vec(), "legacy at {height}");
        }

        let derived = pow_data_with(
            CoinbasePrefix::Prefix(default_coinbase_prefix().try_into().unwrap()),
            extra,
        );
        for height in [
            ESMERALDA_DERIVED_MONERO_COINBASE_ACTIVATION_HEIGHT,
            ESMERALDA_DERIVED_MONERO_COINBASE_ACTIVATION_HEIGHT.saturating_add(1),
        ] {
            let header = header_carrying(&derived, height);
            let decoded = MoneroPowData::from_header(&header, &rules).unwrap();
            assert_eq!(decoded.coinbase_prefix.mode(), CoinbasePrefixMode::Derived);
            let mut re_serialized = Vec::new();
            decoded.serialize(&mut re_serialized).unwrap();
            assert_eq!(re_serialized, header.pow.pow_data.to_vec(), "derived at {height}");
            assert_eq!(prefix_hash(&decoded), prefix_hash(&derived));
        }
    }

    /// Each format is only legal on its own side of the fork, and there is nothing on the wire that lets the bytes
    /// argue otherwise: the height decides, and a payload in the wrong shape is rejected with a bannable error.
    #[test]
    fn each_coinbase_format_is_rejected_on_the_wrong_side_of_the_fork() {
        let rules = esmeralda();
        let extra = RawExtraField(vec![1, 2, 3]);

        let legacy = pow_data_with(
            CoinbasePrefix::Legacy(legacy_sponge(&default_coinbase_prefix())),
            extra.clone(),
        );
        let err = MoneroPowData::from_header(
            &header_carrying(&legacy, ESMERALDA_DERIVED_MONERO_COINBASE_ACTIVATION_HEIGHT),
            &rules,
        )
        .unwrap_err();
        assert!(err.get_ban_reason().is_some(), "legacy above the fork must be bannable");

        let derived = pow_data_with(
            CoinbasePrefix::Prefix(default_coinbase_prefix().try_into().unwrap()),
            extra,
        );
        let err = MoneroPowData::from_header(&header_carrying(&derived, 0), &rules).unwrap_err();
        assert!(
            err.get_ban_reason().is_some(),
            "derived below the fork must be bannable"
        );
    }

    // -----------------------------------------------------------------------------------------------------------
    // GHSA-3qmx-q9pv-f3m4
    // -----------------------------------------------------------------------------------------------------------

    /// The property the whole fix exists for: under the new format, changing `coinbase_tx_extra` - which is where
    /// the Tari merge mining tag lives - changes the coinbase prefix hash. The verifier derives the sponge from
    /// the prefix bytes, so the hash is `Keccak-256(prefix || extra)` and nothing a sender ships can decouple the
    /// two.
    #[test]
    fn ghsa_3qmx_the_prefix_hash_depends_on_the_extra_field_in_the_derived_format() {
        let prefix = default_coinbase_prefix();
        let one = pow_data_with(
            CoinbasePrefix::Prefix(prefix.clone().try_into().unwrap()),
            RawExtraField(vec![1, 2, 3]),
        );
        let other = pow_data_with(
            CoinbasePrefix::Prefix(prefix.clone().try_into().unwrap()),
            RawExtraField(vec![9, 9, 9, 9, 9]),
        );

        assert_ne!(
            prefix_hash(&one),
            prefix_hash(&other),
            "two extra fields must not share a prefix hash"
        );
        assert_ne!(prefix_hash(&one), [0u8; 32]);
        assert_ne!(prefix_hash(&other), [0u8; 32]);

        // And it is the real Keccak-256 of `prefix || extra`, i.e. what Monero itself hashes over that coinbase.
        let mut expected = Keccak::v256();
        expected.update(&prefix);
        expected.update(&encoded_extra(&one.coinbase_tx_extra));
        let mut expected_hash = [0u8; 32];
        expected.finalize(&mut expected_hash);
        assert_eq!(prefix_hash(&one), expected_hash);
    }

    /// The vulnerability, reproduced.
    ///
    /// The pre-fork wire format carried the whole 200-byte Keccak state, so an attacker never had to search for
    /// anything: they pick the state that reproduces some real Monero block's coinbase prefix hash and solve
    /// `buffer = state ^ extra ^ padding` for an extra field carrying *their* merge mining tag. The forged sponge
    /// is entirely well formed - `offset = 0`, `rate = 136`, `mode = Absorbing` - so constraining those three
    /// fields, which is what the first attempt at this fix did, does not touch it. The result is a Tari header
    /// that inherits a real Monero block's difficulty for no work at all.
    #[test]
    fn ghsa_3qmx_an_xor_forged_sponge_reproduces_an_honest_prefix_hash() {
        let prefix = default_coinbase_prefix();
        let honest_extra = RawExtraField(vec![0x42; 20]);
        let honest = pow_data_with(CoinbasePrefix::Legacy(legacy_sponge(&prefix)), honest_extra.clone());
        let honest_hash = prefix_hash(&honest);
        assert_ne!(honest_hash, [0u8; 32], "this is a real Keccak output, not a no-op");

        // Everything the honest miner's sponge will permute, which is what the attacker aims at.
        let mut absorbed = prefix.clone();
        absorbed.extend_from_slice(&encoded_extra(&honest_extra));
        let target_state = padded_state(&absorbed);

        // A different extra field - the attacker's own merge mining tag - and one XOR.
        let forged_extra = RawExtraField(vec![0xAB; 40]);
        assert_ne!(encoded_extra(&forged_extra), encoded_extra(&honest_extra));
        let forged = pow_data_with(
            CoinbasePrefix::Legacy(xor_forged_sponge(&target_state, &forged_extra)),
            forged_extra,
        );

        assert_eq!(
            prefix_hash(&forged),
            honest_hash,
            "a different extra field produced the honest prefix hash"
        );

        // The forged sponge is canonical by every measure the first attempt at this fix checked, and it survives a
        // borsh round trip unchanged, so the re-serialize canonicality check in `from_header` never sees it.
        let mut encoded = Vec::new();
        match &forged.coinbase_prefix {
            CoinbasePrefix::Legacy(hasher) => BorshSerialize::serialize(hasher, &mut encoded).unwrap(),
            CoinbasePrefix::Prefix(_) => panic!("built as legacy"),
        }
        assert_eq!(encoded.len(), KECCAK_STATE_LEN.saturating_add(3));
        assert_eq!(
            encoded.get(KECCAK_STATE_LEN..).expect("length just asserted"),
            [0, KECCAK_256_RATE, Mode::Absorbing as u8],
            "offset, rate and mode are all exactly what the canonicality check demanded"
        );
        assert_ne!(
            encoded.get(..KECCAK_STATE_LEN).expect("length just asserted"),
            [0u8; KECCAK_STATE_LEN],
            "not a zero state"
        );
        let mut re_encoded = Vec::new();
        BorshSerialize::serialize(&Keccak::deserialize(&mut encoded.as_slice()).unwrap(), &mut re_encoded).unwrap();
        assert_eq!(re_encoded, encoded, "the forged state round trips byte for byte");
    }

    /// The forgery above is grandfathered below the fork - knowingly, because every merge mined block on chain is
    /// in that format and cannot be re-validated any other way - and is unrepresentable at and above it, because
    /// the new format carries no sponge state for anyone to forge.
    #[test]
    fn ghsa_3qmx_the_forged_sponge_survives_below_the_fork_and_cannot_be_expressed_above_it() {
        assert_ne!(
            ESMERALDA_DERIVED_MONERO_COINBASE_ACTIVATION_HEIGHT, UNSCHEDULED_ACTIVATION_HEIGHT,
            "this test needs a network with a scheduled activation height"
        );
        let rules = esmeralda();
        let prefix = default_coinbase_prefix();
        let honest_extra = RawExtraField(vec![0x42; 20]);
        let mut absorbed = prefix.clone();
        absorbed.extend_from_slice(&encoded_extra(&honest_extra));
        let target_state = padded_state(&absorbed);
        let forged_extra = RawExtraField(vec![0xAB; 40]);
        let forged = pow_data_with(
            CoinbasePrefix::Legacy(xor_forged_sponge(&target_state, &forged_extra)),
            forged_extra.clone(),
        );

        // Below the fork: still accepted. This is the accepted cost of not invalidating history.
        for height in [0, ESMERALDA_DERIVED_MONERO_COINBASE_ACTIVATION_HEIGHT.saturating_sub(1)] {
            let decoded = MoneroPowData::from_header(&header_carrying(&forged, height), &rules).unwrap();
            assert_eq!(
                prefix_hash(&decoded),
                prefix_hash(&pow_data_with(
                    CoinbasePrefix::Legacy(legacy_sponge(&prefix)),
                    honest_extra.clone()
                )),
                "the forgery still works at height {height}"
            );
        }

        // At and above the fork the very same header no longer parses: the bytes are a sponge and a sponge is not
        // what this height's format is.
        for height in [
            ESMERALDA_DERIVED_MONERO_COINBASE_ACTIVATION_HEIGHT,
            ESMERALDA_DERIVED_MONERO_COINBASE_ACTIVATION_HEIGHT.saturating_add(1),
        ] {
            let err = MoneroPowData::from_header(&header_carrying(&forged, height), &rules).unwrap_err();
            assert!(err.get_ban_reason().is_some(), "at {height}: {err}");
        }

        // And there is nothing to forge in the new format. The state is derived from the prefix bytes, so the only
        // way to reach the honest prefix hash with the attacker's extra field would be a Keccak-256 preimage: even
        // handing the attacker's own forged buffer over as the "prefix" gets nowhere.
        let forged_buffer = vec![0xCDu8; KECCAK_STATE_LEN];
        let derived_forgery = pow_data_with(CoinbasePrefix::Prefix(forged_buffer.try_into().unwrap()), forged_extra);
        assert_ne!(
            prefix_hash(&derived_forgery),
            prefix_hash(&pow_data_with(
                CoinbasePrefix::Prefix(prefix.clone().try_into().unwrap()),
                honest_extra
            ))
        );
    }

    /// The prefix is peer supplied, so it is bounded. The bound is a type-level invariant, which means a header
    /// carrying an over-long prefix dies in the deserializer before anything is allocated on its say-so.
    #[test]
    fn a_coinbase_prefix_over_the_consensus_bound_is_rejected() {
        let rules = esmeralda();
        let height = ESMERALDA_DERIVED_MONERO_COINBASE_ACTIVATION_HEIGHT;

        // The largest prefix the bound was derived for - a coinbase paying the 1,000 recipients the derivation in
        // `consensus_constants.rs` budgets for - fits inside it, and round trips.
        let biggest_real_prefix = realistic_coinbase_prefix(1_000);
        assert!(
            biggest_real_prefix.len() <= MAX_MONERO_COINBASE_PREFIX_SIZE,
            "a 1,000 output coinbase prefix is {} bytes, over the {MAX_MONERO_COINBASE_PREFIX_SIZE} byte bound",
            biggest_real_prefix.len()
        );
        let at_bound = pow_data_with(
            CoinbasePrefix::Prefix(biggest_real_prefix.try_into().unwrap()),
            RawExtraField(vec![1, 2, 3]),
        );
        let header = header_carrying(&at_bound, height);
        let decoded = MoneroPowData::from_header(&header, &rules).unwrap();
        let mut re_serialized = Vec::new();
        decoded.serialize(&mut re_serialized).unwrap();
        assert_eq!(re_serialized, header.pow.pow_data.to_vec());

        // One byte past it cannot even be constructed...
        assert!(
            super::CoinbaseTxPrefix::try_from(vec![7u8; MAX_MONERO_COINBASE_PREFIX_SIZE.saturating_add(1)]).is_err()
        );

        // ...and a peer that writes the oversized length onto the wire by hand is rejected, bannably.
        let mut oversized = Vec::new();
        at_bound.serialize(&mut oversized).unwrap();
        let mut hand_rolled = Vec::new();
        at_bound.header.consensus_encode(&mut hand_rolled).unwrap();
        BorshSerialize::serialize(&at_bound.randomx_key, &mut hand_rolled).unwrap();
        BorshSerialize::serialize(&at_bound.transaction_count, &mut hand_rolled).unwrap();
        at_bound.merkle_root.consensus_encode(&mut hand_rolled).unwrap();
        BorshSerialize::serialize(&at_bound.coinbase_merkle_proof, &mut hand_rolled).unwrap();
        let too_long = u32::try_from(MAX_MONERO_COINBASE_PREFIX_SIZE.saturating_add(1)).unwrap();
        BorshSerialize::serialize(&too_long, &mut hand_rolled).unwrap();
        hand_rolled.extend_from_slice(&vec![7u8; MAX_MONERO_COINBASE_PREFIX_SIZE.saturating_add(1)]);
        BorshSerialize::serialize(&at_bound.coinbase_tx_extra.0, &mut hand_rolled).unwrap();
        BorshSerialize::serialize(&at_bound.aux_chain_merkle_proof, &mut hand_rolled).unwrap();

        let mut header = TariBlockHeader::new(0);
        header.height = height;
        header.pow = ProofOfWork {
            pow_algo: PowAlgorithm::RandomXM,
            pow_data: PowData::try_from(hand_rolled).unwrap(),
        };
        let err = MoneroPowData::from_header(&header, &rules).unwrap_err();
        assert!(matches!(err, MergeMineError::DeserializeError(_)), "{err}");
        assert!(err.get_ban_reason().is_some());
    }

    // -----------------------------------------------------------------------------------------------------------
    // GHSA-3qmx-q9pv-f3m4 follow-up: degenerate legacy sponges, refused at every height
    // -----------------------------------------------------------------------------------------------------------

    /// `legacy_sponge_params` reads the last three bytes of the borsh encoding as `(offset, rate, mode)`. That is
    /// only true while the encoding is the 200-byte state plus three one-byte fields, so pin it: a
    /// `tari-tiny-keccak` bump that changes the layout has to break this test rather than silently make the check
    /// read the wrong bytes.
    #[test]
    fn the_legacy_sponge_encoding_is_exactly_203_bytes() {
        assert_eq!(KECCAK_SERIALIZED_LEN, 203);
        assert_eq!(KECCAK_STATE_LEN, 200);
        assert_eq!(KECCAK_MODE_ABSORBING, Mode::Absorbing as u8);

        let mut encoded = Vec::new();
        BorshSerialize::serialize(&legacy_sponge(&default_coinbase_prefix()), &mut encoded).unwrap();
        assert_eq!(encoded.len(), KECCAK_SERIALIZED_LEN);

        // And the three trailing bytes really are what `Keccak::v256()` leaves behind after absorbing a prefix.
        let prefix_len = u8::try_from(default_coinbase_prefix().len()).expect("the fixture prefix is short");
        assert_eq!(encoded.get(KECCAK_STATE_LEN..).expect("length just asserted"), [
            prefix_len,
            KECCAK_256_RATE,
            KECCAK_MODE_ABSORBING
        ]);
        assert_eq!(
            super::legacy_sponge_params(&legacy_sponge(&default_coinbase_prefix())).unwrap(),
            (prefix_len, KECCAK_256_RATE, KECCAK_MODE_ABSORBING)
        );
    }

    /// The degenerate-state variant of GHSA-3qmx-q9pv-f3m4, demonstrated before it is closed.
    ///
    /// `tari-tiny-keccak`'s `update()` and `squeeze()` both `return` without hashing when
    /// `rate >= 200 || rate <= 1 || rate <= offset`, so a sponge with such parameters produces a prefix hash of
    /// `[0u8; 32]` no matter what `coinbase_tx_extra` says. The merge mining tag lives in `coinbase_tx_extra`, so
    /// that is the tag unbound from the Monero proof of work for free - one sponge backing every Tari header.
    #[test]
    fn ghsa_3qmx_a_degenerate_sponge_makes_the_prefix_hash_ignore_the_extra_field() {
        for (what, offset, rate) in [
            ("a rate of 0", 0u8, 0u8),
            ("a rate of 1", 0, 1),
            ("a rate of 200", 0, 200),
            ("an offset at the rate", KECCAK_256_RATE, KECCAK_256_RATE),
            (
                "an offset past the rate",
                KECCAK_256_RATE.saturating_add(1),
                KECCAK_256_RATE,
            ),
        ] {
            let sponge = keccak_from_state(&[0x11; KECCAK_STATE_LEN], offset, rate, Mode::Absorbing);
            let one = pow_data_with(CoinbasePrefix::Legacy(sponge.clone()), RawExtraField(vec![1, 2, 3]));
            let other = pow_data_with(CoinbasePrefix::Legacy(sponge), RawExtraField(vec![9; 40]));
            assert_eq!(prefix_hash(&one), [0u8; 32], "{what}");
            assert_eq!(
                prefix_hash(&one),
                prefix_hash(&other),
                "{what}: the prefix hash must be shown not to depend on the extra field"
            );
        }
    }

    /// ...and closed, at *every* height. This rule is deliberately not gated on the activation constant: a full
    /// scan of mainnet history found zero blocks that violate it, so tightening it retroactively invalidates
    /// nothing that was ever mined. See `check_legacy_sponge_parameters`.
    #[test]
    fn a_degenerate_legacy_sponge_is_rejected_at_every_height() {
        let rules = esmeralda();
        assert_ne!(
            ESMERALDA_DERIVED_MONERO_COINBASE_ACTIVATION_HEIGHT, UNSCHEDULED_ACTIVATION_HEIGHT,
            "this test needs a network with a scheduled activation height"
        );

        let cases: Vec<(&str, u8, u8, Mode)> = vec![
            ("a rate of 0", 0, 0, Mode::Absorbing),
            ("a rate of 1", 0, 1, Mode::Absorbing),
            ("a rate of 200", 0, 200, Mode::Absorbing),
            (
                "an offset at the rate",
                KECCAK_256_RATE,
                KECCAK_256_RATE,
                Mode::Absorbing,
            ),
            (
                "an offset past the rate",
                KECCAK_256_RATE.saturating_add(1),
                KECCAK_256_RATE,
                Mode::Absorbing,
            ),
            ("a squeezing sponge", 0, KECCAK_256_RATE, Mode::Squeezing),
        ];

        for (what, offset, rate, mode) in cases {
            let pow_data = pow_data_with(
                CoinbasePrefix::Legacy(keccak_from_state(&[0x11; KECCAK_STATE_LEN], offset, rate, mode)),
                RawExtraField(vec![1, 2, 3]),
            );
            // Below the activation height, where the legacy format is the legal one, the parameter check is what
            // does the rejecting - and it is the point of the test that it does so at height 0 too.
            for height in [0, ESMERALDA_DERIVED_MONERO_COINBASE_ACTIVATION_HEIGHT.saturating_sub(1)] {
                let err = MoneroPowData::from_header(&header_carrying(&pow_data, height), &rules)
                    .expect_err("a degenerate sponge must be refused below the activation height too");
                assert!(
                    matches!(err, MergeMineError::DegenerateLegacyCoinbaseHasher(_)),
                    "{what} at {height}: {err}"
                );
                assert!(err.get_ban_reason().is_some(), "{what} at {height}: {err}");
            }
            // At and above it the payload is not even the right shape, so it dies earlier - but it still dies,
            // bannably.
            for height in [
                ESMERALDA_DERIVED_MONERO_COINBASE_ACTIVATION_HEIGHT,
                ESMERALDA_DERIVED_MONERO_COINBASE_ACTIVATION_HEIGHT.saturating_add(1),
            ] {
                let err = MoneroPowData::from_header(&header_carrying(&pow_data, height), &rules)
                    .expect_err("a sponge is not the format at or above the activation height at all");
                assert!(err.get_ban_reason().is_some(), "{what} at {height}: {err}");
            }
        }
    }

    /// The other half of the rule: an honest legacy sponge - the thing every merge mined block on chain carries -
    /// is still accepted, unchanged, everywhere the legacy format is legal, and its prefix hash does not move.
    /// If this ever fails, the retroactive rule does not generalise and must be reverted rather than forced.
    #[test]
    fn an_honest_legacy_sponge_is_still_accepted_at_every_height_below_the_fork() {
        let rules = esmeralda();
        let extra = RawExtraField(encoded_subfield(&SubField::MergeMining(
            VarInt(0),
            Hash::from_slice(&[0x11; 32]),
        )));

        for prefix in [
            default_coinbase_prefix(),
            realistic_coinbase_prefix(1),
            realistic_coinbase_prefix(2),
            // Over one rate block, so the sponge has permuted and `offset` is no longer just the prefix length.
            realistic_coinbase_prefix(16),
        ] {
            let pow_data = pow_data_with(CoinbasePrefix::Legacy(legacy_sponge(&prefix)), extra.clone());
            super::check_legacy_sponge_parameters(&legacy_sponge(&prefix))
                .unwrap_or_else(|e| panic!("an honest {} byte prefix must absorb cleanly: {e}", prefix.len()));

            for height in [0, ESMERALDA_DERIVED_MONERO_COINBASE_ACTIVATION_HEIGHT.saturating_sub(1)] {
                let decoded = MoneroPowData::from_header(&header_carrying(&pow_data, height), &rules)
                    .unwrap_or_else(|e| panic!("an honest legacy sponge must be accepted at {height}: {e}"));
                let mut expected = Keccak::v256();
                expected.update(&prefix);
                expected.update(&encoded_extra(&extra));
                let mut expected_hash = [0u8; 32];
                expected.finalize(&mut expected_hash);
                assert_eq!(prefix_hash(&decoded), expected_hash, "at {height}");
                assert_ne!(prefix_hash(&decoded), [0u8; 32]);
            }
        }
    }

    /// The scope of the new rule, stated as a test so nobody mistakes it for the fix: the XOR forgery has
    /// `rate = 136`, `offset = 0`, `mode = Absorbing`, so the parameter check passes it, exactly as it always
    /// would have. Only the degenerate-state variant is closed. The forgery itself stays grandfathered below the
    /// activation height and unrepresentable above it, which is what
    /// `ghsa_3qmx_the_forged_sponge_survives_below_the_fork_and_cannot_be_expressed_above_it` covers.
    #[test]
    fn the_sponge_parameter_check_does_not_close_the_xor_forgery() {
        let prefix = default_coinbase_prefix();
        let honest_extra = RawExtraField(vec![0x42; 20]);
        let mut absorbed = prefix.clone();
        absorbed.extend_from_slice(&encoded_extra(&honest_extra));
        let forged_extra = RawExtraField(vec![0xAB; 40]);
        let forged = xor_forged_sponge(&padded_state(&absorbed), &forged_extra);

        assert_eq!(
            super::legacy_sponge_params(&forged).unwrap(),
            (0, KECCAK_256_RATE, KECCAK_MODE_ABSORBING),
            "the forgery's parameters are the honest ones"
        );
        super::check_legacy_sponge_parameters(&forged)
            .expect("the parameter check cannot and does not close the XOR forgery");
    }

    // -----------------------------------------------------------------------------------------------------------
    // GHSA-3qmx-q9pv-f3m4 follow-up: the prefix/extra boundary
    // -----------------------------------------------------------------------------------------------------------

    /// The boundary bug, and the fix for it, in one test.
    ///
    /// `is_coinbase_valid_merkle_root` hashes `prefix || VarInt(len(extra)) || extra`. Bounding only the *length*
    /// of the prefix left the split point free: wherever `extra[j] == len(extra) - j - 1`, the byte at the cut can
    /// be re-read as the length VarInt of a shorter extra field, and
    ///
    ///     prefix' = prefix || VarInt(len(extra)) || extra[..j]      extra' = extra[j + 1..]
    ///
    /// re-concatenates to the identical byte string. The condition is not contrived: an extra field that ends in a
    /// `Nonce` sub-field meets it at the nonce's own length byte, which is what this test uses. Both readings parse
    /// to exactly one merge mining tag - a different one each - so one RandomX solution backed two valid Tari
    /// headers with different content. That is free equivocation, a ready made reorg hedge.
    ///
    /// The first half of the test shows the ambiguity is real (so it would have failed against the old behaviour);
    /// the second half shows `from_header` now takes the honest reading and refuses the shifted one.
    #[test]
    fn ghsa_3qmx_the_prefix_extra_boundary_admits_only_one_split() {
        let rules = esmeralda();
        let height = ESMERALDA_DERIVED_MONERO_COINBASE_ACTIVATION_HEIGHT;
        let prefix = realistic_coinbase_prefix(1);

        // An ordinary looking extra field: the Tari merge mining tag, then a nonce. A nonce is opaque bytes, so
        // nothing stops those bytes from themselves being a second, differently rooted merge mining tag.
        let honest_root = Hash::from_slice(&[0x11; 32]);
        let hidden_root = Hash::from_slice(&[0x22; 32]);
        let honest_tag = encoded_subfield(&SubField::MergeMining(VarInt(0), honest_root));
        let hidden_tag = encoded_subfield(&SubField::MergeMining(VarInt(0), hidden_root));
        let mut extra_bytes = honest_tag.clone();
        extra_bytes.extend_from_slice(&encoded_subfield(&SubField::Nonce(hidden_tag.clone())));
        let extra = RawExtraField(extra_bytes.clone());
        assert!(
            extra_bytes.len() <= rules.consensus_constants(height).max_extra_field_size(),
            "the example has to be an extra field a header is actually allowed to carry"
        );

        // The cut: the nonce's length byte, one past the honest tag.
        let cut = honest_tag.len().saturating_add(1);
        let total = extra_bytes.len();
        assert_eq!(
            usize::from(*extra_bytes.get(cut).expect("the cut is inside the extra field")),
            total.saturating_sub(cut).saturating_sub(1),
            "the realignment condition `extra[j] == len(extra) - j - 1`"
        );

        let mut shifted_prefix = prefix.clone();
        shifted_prefix.extend_from_slice(&encoded_varint(total));
        shifted_prefix.extend_from_slice(extra_bytes.get(..cut).expect("the cut is inside the extra field"));
        let shifted_extra = RawExtraField(
            extra_bytes
                .get(cut.saturating_add(1)..)
                .expect("the cut is inside the extra field")
                .to_vec(),
        );

        // (a) The ambiguity is real: the two readings hash the very same bytes...
        let mut honest_stream = prefix.clone();
        honest_stream.extend_from_slice(&encoded_extra(&extra));
        let mut shifted_stream = shifted_prefix.clone();
        shifted_stream.extend_from_slice(&encoded_extra(&shifted_extra));
        assert_eq!(
            honest_stream, shifted_stream,
            "the two splits must be the same byte string, or there is nothing to equivocate with"
        );

        let honest = pow_data_with(
            CoinbasePrefix::Prefix(prefix.clone().try_into().unwrap()),
            extra.clone(),
        );
        let shifted = pow_data_with(
            CoinbasePrefix::Prefix(shifted_prefix.clone().try_into().unwrap()),
            shifted_extra.clone(),
        );
        assert_eq!(
            prefix_hash(&honest),
            prefix_hash(&shifted),
            "same bytes, same prefix hash: one solution, two headers"
        );

        // ...while carrying different merge mining tags, which is what makes it equivocation rather than a curio.
        assert_eq!(merge_mining_roots(&extra), vec![honest_root]);
        assert_eq!(merge_mining_roots(&shifted_extra), vec![hidden_root]);

        // (b) Only the honest reading is a complete coinbase prefix, so only it is accepted.
        MoneroPowData::from_header(&header_carrying(&honest, height), &rules)
            .expect("the honest split is a real coinbase prefix");
        let err = MoneroPowData::from_header(&header_carrying(&shifted, height), &rules).unwrap_err();
        assert!(matches!(err, MergeMineError::NonCanonicalCoinbasePrefix(_)), "{err}");
        assert!(err.get_ban_reason().is_some(), "{err}");
    }

    /// The same shifted split below the activation height, where the legacy format still rules. It is still
    /// accepted, and still produces the honest prefix hash: the new check has not leaked backwards onto history.
    /// The ambiguity is grandfathered there along with the sponge forgery - an attacker could always have absorbed
    /// `prefix || VarInt(n) || extra[..j]` into the sponge they sent, so this is the old exposure, not a new one.
    #[test]
    fn the_boundary_check_does_not_reach_below_the_activation_height() {
        let rules = esmeralda();
        let prefix = realistic_coinbase_prefix(1);
        let honest_tag = encoded_subfield(&SubField::MergeMining(VarInt(0), Hash::from_slice(&[0x11; 32])));
        let hidden_tag = encoded_subfield(&SubField::MergeMining(VarInt(0), Hash::from_slice(&[0x22; 32])));
        let mut extra_bytes = honest_tag.clone();
        extra_bytes.extend_from_slice(&encoded_subfield(&SubField::Nonce(hidden_tag)));
        let extra = RawExtraField(extra_bytes.clone());

        let cut = honest_tag.len().saturating_add(1);
        let mut shifted_prefix = prefix.clone();
        shifted_prefix.extend_from_slice(&encoded_varint(extra_bytes.len()));
        shifted_prefix.extend_from_slice(extra_bytes.get(..cut).expect("the cut is inside the extra field"));
        let shifted_extra = RawExtraField(
            extra_bytes
                .get(cut.saturating_add(1)..)
                .expect("the cut is inside the extra field")
                .to_vec(),
        );

        let shifted = pow_data_with(CoinbasePrefix::Legacy(legacy_sponge(&shifted_prefix)), shifted_extra);
        let honest = pow_data_with(CoinbasePrefix::Legacy(legacy_sponge(&prefix)), extra);
        for height in [0, ESMERALDA_DERIVED_MONERO_COINBASE_ACTIVATION_HEIGHT.saturating_sub(1)] {
            let decoded = MoneroPowData::from_header(&header_carrying(&shifted, height), &rules)
                .unwrap_or_else(|e| panic!("the legacy path must be untouched at {height}: {e}"));
            assert_eq!(
                prefix_hash(&decoded),
                prefix_hash(&honest),
                "the shifted split still works at height {height}"
            );
        }
    }

    /// Trailing bytes after an otherwise complete prefix are the core of the fix: they are where the shifted
    /// reading has to put `VarInt(len(extra)) || extra[..j]`.
    #[test]
    fn trailing_bytes_after_a_complete_coinbase_prefix_are_rejected() {
        let rules = esmeralda();
        let height = ESMERALDA_DERIVED_MONERO_COINBASE_ACTIVATION_HEIGHT;

        for trailing in [vec![0x00], vec![0x01, 0x02, 0x03], vec![0xff; 16]] {
            let mut prefix = realistic_coinbase_prefix(2);
            prefix.extend_from_slice(&trailing);
            let pow_data = pow_data_with(
                CoinbasePrefix::Prefix(prefix.try_into().unwrap()),
                RawExtraField(vec![1, 2, 3]),
            );
            let err = MoneroPowData::from_header(&header_carrying(&pow_data, height), &rules).unwrap_err();
            assert!(
                matches!(err, MergeMineError::NonCanonicalCoinbasePrefix(_)),
                "{trailing:?}: {err}"
            );
            assert!(err.get_ban_reason().is_some(), "{err}");
        }
    }

    /// Bytes that are not a coinbase prefix at all do not get to be one, and saying so is bannable.
    #[test]
    fn a_prefix_that_is_not_a_coinbase_prefix_is_rejected() {
        let rules = esmeralda();
        let height = ESMERALDA_DERIVED_MONERO_COINBASE_ACTIVATION_HEIGHT;

        let cases: Vec<(&str, Vec<u8>)> = vec![
            ("empty", vec![]),
            ("a VarInt that never ends", vec![0xff; 32]),
            ("truncated after the version", vec![0x02]),
            ("an input type that does not exist", vec![0x02, 0x00, 0x01, 0x09]),
            ("an input count with no inputs behind it", vec![0x02, 0x00, 0x04]),
            ("an output target tag that does not exist", vec![
                0x02, 0x00, 0x00, 0x01, 0x00, 0x07,
            ]),
            ("the sponge the legacy format carries", vec![0xcd; 203]),
        ];
        for (what, prefix) in cases {
            let pow_data = pow_data_with(
                CoinbasePrefix::Prefix(prefix.try_into().unwrap()),
                RawExtraField(vec![1, 2, 3]),
            );
            let err = MoneroPowData::from_header(&header_carrying(&pow_data, height), &rules).unwrap_err();
            assert!(
                matches!(err, MergeMineError::NonCanonicalCoinbasePrefix(_)),
                "{what}: {err}"
            );
            assert!(err.get_ban_reason().is_some(), "{what}: {err}");
        }
    }

    /// Non-canonical VarInts would put several byte strings back in play for one logical prefix, so they are
    /// refused. This also *verifies* - rather than assumes - that monero's own VarInt decoder already rejects the
    /// only non-minimal spelling its format admits, which is what the comment on `check_coinbase_prefix_bytes`
    /// claims; the re-encode comparison is what the fix actually leans on either way.
    #[test]
    fn non_canonical_varints_are_rejected() {
        // `0x80 0x00` is 0 written in two groups instead of one. monero's decoder refuses it outright...
        assert!(
            VarInt::consensus_decode(&mut [0x80u8, 0x00].as_slice()).is_err(),
            "monero's VarInt decoder is expected to reject a zero group in a non-first position"
        );
        // ...while the minimal spelling of the same value decodes.
        assert_eq!(VarInt::consensus_decode(&mut [0x00u8].as_slice()).unwrap(), VarInt(0));

        let rules = esmeralda();
        let height = ESMERALDA_DERIVED_MONERO_COINBASE_ACTIVATION_HEIGHT;

        // The honest prefix, with its leading `version` VarInt re-spelled non-minimally. Nothing else changes.
        let honest = realistic_coinbase_prefix(1);
        let mut padded = vec![0x82, 0x00];
        padded.extend_from_slice(honest.get(1..).expect("the version is one byte"));
        assert_ne!(padded, honest);

        let pow_data = pow_data_with(
            CoinbasePrefix::Prefix(padded.try_into().unwrap()),
            RawExtraField(vec![1, 2, 3]),
        );
        let err = MoneroPowData::from_header(&header_carrying(&pow_data, height), &rules).unwrap_err();
        assert!(matches!(err, MergeMineError::NonCanonicalCoinbasePrefix(_)), "{err}");
        assert!(err.get_ban_reason().is_some(), "{err}");
    }

    /// Honest prefixes - the default fixture and a realistic coinbase - stay accepted, unchanged, and the prefix
    /// hash is still plain `Keccak-256(prefix || VarInt(len(extra)) || extra)`, so no difficulty moves.
    #[test]
    fn honest_coinbase_prefixes_are_still_accepted_unchanged() {
        let rules = esmeralda();
        let height = ESMERALDA_DERIVED_MONERO_COINBASE_ACTIVATION_HEIGHT;
        let extra = RawExtraField(encoded_subfield(&SubField::MergeMining(
            VarInt(0),
            Hash::from_slice(&[0x11; 32]),
        )));

        for prefix in [
            default_coinbase_prefix(),
            realistic_coinbase_prefix(1),
            realistic_coinbase_prefix(2),
            realistic_coinbase_prefix(16),
        ] {
            let pow_data = pow_data_with(
                CoinbasePrefix::Prefix(prefix.clone().try_into().unwrap()),
                extra.clone(),
            );
            let header = header_carrying(&pow_data, height);
            let decoded = MoneroPowData::from_header(&header, &rules)
                .unwrap_or_else(|e| panic!("an honest {} byte prefix must be accepted: {e}", prefix.len()));

            let mut expected = Keccak::v256();
            expected.update(&prefix);
            expected.update(&encoded_extra(&extra));
            let mut expected_hash = [0u8; 32];
            expected.finalize(&mut expected_hash);
            assert_eq!(prefix_hash(&decoded), expected_hash);
            assert_eq!(decoded.to_blockhashing_blob(), pow_data.to_blockhashing_blob());
        }
    }

    // -----------------------------------------------------------------------------------------------------------
    // GHSA-3qmx-q9pv-f3m4: the coinbase input vector
    // -----------------------------------------------------------------------------------------------------------

    /// A coinbase prefix built by hand, so that inputs that `monero::TxIn`'s own encoder would never produce can be
    /// expressed. `input_bytes` is spliced in where the input vector goes, count and all.
    fn prefix_with_raw_inputs(input_bytes: &[u8]) -> Vec<u8> {
        let mut encoded = Vec::new();
        VarInt(2).consensus_encode(&mut encoded).unwrap();
        VarInt(0).consensus_encode(&mut encoded).unwrap();
        encoded.extend_from_slice(input_bytes);
        Vec::<TxOut>::new().consensus_encode(&mut encoded).unwrap();
        encoded
    }

    fn prefix_rejection(prefix: Vec<u8>) -> MergeMineError {
        let pow_data = pow_data_with(
            CoinbasePrefix::Prefix(prefix.try_into().unwrap()),
            RawExtraField(vec![1, 2, 3]),
        );
        MoneroPowData::from_header(
            &header_carrying(&pow_data, ESMERALDA_DERIVED_MONERO_COINBASE_ACTIVATION_HEIGHT),
            &esmeralda(),
        )
        .unwrap_err()
    }

    /// A Monero coinbase has exactly one input. Anything else is refused, and refusing it is bannable.
    #[test]
    fn a_coinbase_prefix_must_carry_exactly_one_input() {
        for count in [0u64, 2, 3, 255] {
            let mut inputs = Vec::new();
            VarInt(count).consensus_encode(&mut inputs).unwrap();
            for _ in 0..count {
                TxIn::Gen { height: VarInt(7) }.consensus_encode(&mut inputs).unwrap();
            }
            let err = prefix_rejection(prefix_with_raw_inputs(&inputs));
            assert!(
                matches!(&err, MergeMineError::NonCanonicalCoinbasePrefix(e) if e.contains("exactly one input")),
                "{count} inputs: {err}"
            );
            assert!(err.get_ban_reason().is_some(), "{count} inputs: {err}");
        }

        // ...and exactly one *is* accepted, so the test above is not passing for the wrong reason.
        let mut inputs = Vec::new();
        vec![TxIn::Gen { height: VarInt(7) }]
            .consensus_encode(&mut inputs)
            .unwrap();
        let prefix = prefix_with_raw_inputs(&inputs);
        let pow_data = pow_data_with(
            CoinbasePrefix::Prefix(prefix.try_into().unwrap()),
            RawExtraField(vec![1, 2, 3]),
        );
        MoneroPowData::from_header(
            &header_carrying(&pow_data, ESMERALDA_DERIVED_MONERO_COINBASE_ACTIVATION_HEIGHT),
            &esmeralda(),
        )
        .unwrap();
    }

    /// The only input a coinbase may carry is `TxIn::Gen`. `TxIn::ToKey` is a spend, it cannot appear in a
    /// coinbase, and it is the variant that holds the nested `Vec<VarInt>` the next test is about.
    #[test]
    fn a_coinbase_prefix_with_a_spend_input_is_rejected() {
        let mut inputs = Vec::new();
        VarInt(1).consensus_encode(&mut inputs).unwrap();
        inputs.push(0x02); // TxIn::ToKey
        VarInt(0).consensus_encode(&mut inputs).unwrap(); // amount
        vec![VarInt(1)].consensus_encode(&mut inputs).unwrap(); // key_offsets
        inputs.extend_from_slice(&[0x44u8; 32]); // k_image

        let err = prefix_rejection(prefix_with_raw_inputs(&inputs));
        assert!(
            matches!(&err, MergeMineError::NonCanonicalCoinbasePrefix(e) if e.contains("`Gen` input")),
            "{err}"
        );
        assert!(err.get_ban_reason().is_some(), "{err}");
    }

    /// GHSA-3qmx-q9pv-f3m4 follow up: the nested `key_offsets` allocation.
    ///
    /// `decode_counted_vec` bounds the *outer* vector's count by the bytes remaining, but it hands each element to
    /// `TxIn::consensus_decode`, which for tag `0x02` decodes `key_offsets: Vec<VarInt>` with monero's blanket
    /// `Decodable for Vec<T>`. That reserves `Vec::with_capacity(count)` up front and the only bound is
    /// `size_of::<VarInt>() * count > MAX_VEC_MEM_ALLOC_SIZE`, i.e. `8 * count > 32 MiB`. A count of exactly
    /// `4_194_304` makes that `33_554_432 > 33_554_432`, which is false, so it passes and reserves 32 MiB - off
    /// the nine bytes below, on every peer supplied header.
    ///
    /// The fix does not let the decoder get that far: the input tag is checked before anything is handed to
    /// `TxIn::consensus_decode` at all. The assertion on the message is what pins that down - a message about the
    /// tag means the `ToKey` branch was never entered, whereas one about `key_offsets` would mean the allocation
    /// had already happened and we only rejected the result.
    #[test]
    fn ghsa_3qmx_a_key_offsets_count_cannot_reserve_memory() {
        // Exactly the payload from the advisory review: version, unlock time, one input, `ToKey`, amount 0, then a
        // `key_offsets` count of 4_194_304.
        const KEY_OFFSETS_AT_THE_BOUNDARY: [u8; 9] = [0x02, 0x00, 0x01, 0x02, 0x00, 0x80, 0x80, 0x80, 0x02];
        // Sanity: that really is the count that lands on monero's limit rather than over it.
        assert_eq!(
            VarInt::consensus_decode(&mut [0x80u8, 0x80, 0x80, 0x02].as_slice()).unwrap(),
            VarInt(4_194_304)
        );
        assert_eq!(8 * 4_194_304usize, 32 * 1024 * 1024);

        let err = prefix_rejection(KEY_OFFSETS_AT_THE_BOUNDARY.to_vec());
        assert!(
            matches!(&err, MergeMineError::NonCanonicalCoinbasePrefix(e) if e.contains("`Gen` input")),
            "the input tag must be refused before the decoder is handed the rest: {err}"
        );
        assert!(err.get_ban_reason().is_some(), "{err}");
    }

    /// The other counted vector in the prefix, `outputs`, has no nested allocation to worry about - `TxOut` is a
    /// `VarInt` and a fixed width target - so `decode_counted_vec`'s bound is the whole bound there. This pins the
    /// two properties the comment on `decode_counted_vec` claims: a count above the bytes remaining is refused
    /// without being believed, and a `TxOut` really is fixed width once its tag is read.
    #[test]
    fn an_output_count_larger_than_the_bytes_remaining_is_refused() {
        let mut prefix = Vec::new();
        VarInt(2).consensus_encode(&mut prefix).unwrap();
        VarInt(0).consensus_encode(&mut prefix).unwrap();
        vec![TxIn::Gen { height: VarInt(7) }]
            .consensus_encode(&mut prefix)
            .unwrap();
        // 4_194_304 outputs claimed, nothing behind them.
        prefix.extend_from_slice(&[0x80, 0x80, 0x80, 0x02]);

        let err = prefix_rejection(prefix);
        assert!(
            matches!(&err, MergeMineError::NonCanonicalCoinbasePrefix(e) if e.contains("cannot fit in")),
            "{err}"
        );
        assert!(err.get_ban_reason().is_some(), "{err}");
    }
}
