// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! What a one sided metadata signature commits to besides the script, read by the device from the raw output fields.
//!
//! # Why the device reads the fields itself
//!
//! An output's metadata signature is over `H(script || common)`, where `common` is the consensus hash of the output's
//! version, features, covenant, encrypted data and minimum value promise (`TransactionOutput::
//! metadata_signature_message_common_from_parts` on the host). The device has always built the script itself, from
//! the receiver address it shows. `common` used to arrive as an opaque 32 byte hash, so the device could not see what
//! it was signing: a host could have it auto-approve "change" that was really a burn, a coinbase, or a freeze behind a
//! huge maturity.
//!
//! Now the host sends the *preimage* of `common` - the borsh encodings of those five fields, back to back, exactly as
//! the consensus hasher writes them - and the device hashes it itself. A host that lies about any field gets a
//! signature that only verifies on an output carrying exactly the fields the device saw, because consensus recomputes
//! `common` from the output's own fields.
//!
//! # Why the whole preimage is parsed, not just the fields the decision needs
//!
//! Borsh writes no field boundaries into the hash: the preimage is the fields' encodings concatenated. Reading a field
//! somewhere in the middle - the covenant, say - is only sound at the offset where a consensus decoder would read it,
//! and that offset depends on the lengths of everything before it, a sidechain feature included. So
//! [`parse_metadata_preimage`] walks the whole preimage with the same grammar the consensus types' borsh decoders use,
//! field by field, and requires every byte to be consumed. Where it is stricter than consensus (a key prefix that is
//! not 32, say) the device refuses an output no valid transaction could carry anyway; it is never looser in a way
//! that would let it read a field from a different offset than consensus does.
//!
//! The grammar here mirrors these host types, field for field, and `tari_transaction_components` holds the parity
//! tests that check it against their real borsh encodings:
//!
//! - `TransactionOutputVersion`: `u8` (0 or 1);
//! - `OutputFeatures`: `version: u8 (0|1) | output_type: u8 (0..=7) | maturity: u64 | coinbase_extra: u32-len bytes |
//!   sidechain_feature: Option<SideChainFeature> | range_proof_type: u8 (0|1)`;
//! - `SideChainFeature`: `data: enum { ValidatorNodeRegistration, CodeTemplateRegistration, ConfidentialOutput,
//!   ValidatorNodeExit } | sidechain_id: Option<(public key, signature)>`;
//! - `Covenant`: a varint length and the token bytes - only the empty covenant, the single byte `0`, is accepted;
//! - `EncryptedData`: `u32-len bytes`, between [`MIN_ENCRYPTED_DATA_SIZE`] and [`MAX_ENCRYPTED_DATA_SIZE`];
//! - `MicroMinotari`: `u64`.
//!
//! A public key is borsh's slice encoding of 32 bytes (`u32` length 32, then the bytes), and a signature is a public
//! key followed by a scalar in the same encoding.

/// The largest preimage the device accepts. The largest output a ledger wallet signs is a code template
/// registration with every string at its maximum and a sidechain id, plus the largest encrypted data: under 1260
/// bytes. Anything longer is refused before it is buffered.
pub const MAX_METADATA_PREIMAGE_SIZE: usize = 1280;

/// `STATIC_ENCRYPTED_DATA_SIZE_TOTAL` on the host: nonce, value, mask and tag.
pub const MIN_ENCRYPTED_DATA_SIZE: usize = 80;
/// `MAX_ENCRYPTED_DATA_SIZE` on the host: the static part and up to 256 bytes of memo.
pub const MAX_ENCRYPTED_DATA_SIZE: usize = 256 + MIN_ENCRYPTED_DATA_SIZE;
/// `CoinBaseExtra` on the host: `MaxSizeBytes<258>`.
const MAX_COINBASE_EXTRA_SIZE: usize = 258;
/// `CodeTemplateRegistration::template_name`: `MaxSizeString<32>`.
const MAX_TEMPLATE_NAME_SIZE: usize = 32;
/// `CodeTemplateRegistration::binary_url` and `BuildInfo::repo_url`: `MaxSizeString<255>`.
const MAX_URL_SIZE: usize = 255;
/// `BuildInfo::commit_hash`: `MaxSizeBytes<32>`.
const MAX_COMMIT_HASH_SIZE: usize = 32;

/// `OutputType` on the host, by discriminant.
pub const OUTPUT_TYPE_STANDARD: u8 = 0;
pub const OUTPUT_TYPE_COINBASE: u8 = 1;
pub const OUTPUT_TYPE_BURN: u8 = 2;
const OUTPUT_TYPE_MAX: u8 = 7;

/// The borsh encoding of `OutputFeatures::default()`: version 0, `Standard`, maturity 0, no coinbase extra, no
/// sidechain feature, `BulletProofPlus`.
pub const DEFAULT_OUTPUT_FEATURES: [u8; 16] = [0; 16];

/// What a sidechain feature is, for the review screen.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum SideChainKind {
    ValidatorNodeRegistration,
    CodeTemplateRegistration,
    ConfidentialOutput,
    ValidatorNodeExit,
}

impl SideChainKind {
    pub fn name(self) -> &'static str {
        match self {
            SideChainKind::ValidatorNodeRegistration => "VN registration",
            SideChainKind::CodeTemplateRegistration => "Code template",
            SideChainKind::ConfidentialOutput => "Confidential output",
            SideChainKind::ValidatorNodeExit => "VN exit",
        }
    }
}

/// The name of an output type, for the review screen.
pub fn output_type_name(output_type: u8) -> &'static str {
    match output_type {
        0 => "Standard",
        1 => "Coinbase",
        2 => "Burn",
        3 => "VN registration",
        4 => "Code template",
        5 => "Sidechain checkpoint",
        6 => "Sidechain proof",
        7 => "VN exit",
        _ => "Unknown",
    }
}

/// What the device needs to know about the preimage it is about to hash and sign.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataPreimage {
    pub output_version: u8,
    /// Whether the features are exactly `OutputFeatures::default()`.
    pub has_default_features: bool,
    pub output_type: u8,
    pub maturity: u64,
    pub coinbase_extra_size: usize,
    pub sidechain: Option<SideChainKind>,
    /// The validator node's public key, for a validator node registration or exit.
    pub validator_node_public_key: Option<[u8; 32]>,
    /// `RangeProofType`: 0 `BulletProofPlus`, 1 `RevealedValue`.
    pub range_proof_type: u8,
    pub minimum_value_promise: u64,
}

/// Why a preimage is not one the device signs.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum MetadataPreimageError {
    /// Not a preimage any valid output could have: a field out of range, a length out of bounds, a byte left over.
    Malformed,
    /// A non-empty covenant. It can restrict how the output is spent, and the device cannot show what it says.
    CovenantNotEmpty,
    /// A burn. Burns are not supported on a ledger wallet.
    Burn,
    /// A coinbase, or coinbase extra data on any output. The device never signs a coinbase.
    Coinbase,
}

impl MetadataPreimage {
    /// Whether this output may be signed without a review when it goes to the wallet's own address: default
    /// features (and so a standard output, maturity 0, no sidechain feature) and a minimum value promise of 0. The
    /// covenant is always empty here; anything else was refused by [`parse_metadata_preimage`].
    pub fn may_skip_review(&self) -> bool {
        self.has_default_features && self.minimum_value_promise == 0
    }
}

/// Parse a metadata signature preimage: `version | features | covenant | encrypted_data | minimum_value_promise`, as
/// the consensus hasher writes them. See the module docs. Refuses what the device will not sign - a covenant, a burn,
/// a coinbase - with its own reason, and anything that is not a valid preimage as [`MetadataPreimageError::Malformed`].
pub fn parse_metadata_preimage(bytes: &[u8]) -> Result<MetadataPreimage, MetadataPreimageError> {
    if bytes.len() > MAX_METADATA_PREIMAGE_SIZE {
        return Err(MetadataPreimageError::Malformed);
    }
    let mut cursor = Cursor { rest: bytes };

    let output_version = cursor.u8()?;
    if output_version > 1 {
        return Err(MetadataPreimageError::Malformed);
    }

    // `OutputFeatures`.
    let features_start = cursor.rest;
    let features_version = cursor.u8()?;
    if features_version > 1 {
        return Err(MetadataPreimageError::Malformed);
    }
    let output_type = cursor.u8()?;
    if output_type > OUTPUT_TYPE_MAX {
        return Err(MetadataPreimageError::Malformed);
    }
    let maturity = cursor.u64()?;
    let coinbase_extra_size = cursor.bytes(MAX_COINBASE_EXTRA_SIZE)?;
    let (sidechain, validator_node_public_key) = match cursor.u8()? {
        0 => (None, None),
        1 => {
            let (kind, key) = cursor.sidechain_feature()?;
            (Some(kind), key)
        },
        _ => return Err(MetadataPreimageError::Malformed),
    };
    let range_proof_type = cursor.u8()?;
    if range_proof_type > 1 {
        return Err(MetadataPreimageError::Malformed);
    }
    let features_size = features_start.len().saturating_sub(cursor.rest.len());
    let has_default_features = features_start.get(..features_size) == Some(DEFAULT_OUTPUT_FEATURES.as_slice());

    // `Covenant`: only the empty one, a zero length.
    match cursor.u8()? {
        0 => {},
        _ => return Err(MetadataPreimageError::CovenantNotEmpty),
    }

    // `EncryptedData`.
    let encrypted_data_size = cursor.bytes(MAX_ENCRYPTED_DATA_SIZE)?;
    if encrypted_data_size < MIN_ENCRYPTED_DATA_SIZE {
        return Err(MetadataPreimageError::Malformed);
    }

    let minimum_value_promise = cursor.u64()?;
    if !cursor.rest.is_empty() {
        return Err(MetadataPreimageError::Malformed);
    }

    if output_type == OUTPUT_TYPE_BURN {
        return Err(MetadataPreimageError::Burn);
    }
    if output_type == OUTPUT_TYPE_COINBASE || coinbase_extra_size != 0 {
        return Err(MetadataPreimageError::Coinbase);
    }

    Ok(MetadataPreimage {
        output_version,
        has_default_features,
        output_type,
        maturity,
        coinbase_extra_size,
        sidechain,
        validator_node_public_key,
        range_proof_type,
        minimum_value_promise,
    })
}

/// A cursor over the preimage. Never indexes, so a malformed preimage cannot panic the device.
struct Cursor<'a> {
    rest: &'a [u8],
}

impl Cursor<'_> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N], MetadataPreimageError> {
        let (field, rest) = self
            .rest
            .split_first_chunk::<N>()
            .ok_or(MetadataPreimageError::Malformed)?;
        self.rest = rest;
        Ok(*field)
    }

    fn u8(&mut self) -> Result<u8, MetadataPreimageError> {
        self.take::<1>().map(|[byte]| byte)
    }

    fn u16(&mut self) -> Result<u16, MetadataPreimageError> {
        self.take::<2>().map(u16::from_le_bytes)
    }

    fn u32(&mut self) -> Result<u32, MetadataPreimageError> {
        self.take::<4>().map(u32::from_le_bytes)
    }

    fn u64(&mut self) -> Result<u64, MetadataPreimageError> {
        self.take::<8>().map(u64::from_le_bytes)
    }

    fn skip(&mut self, size: usize) -> Result<(), MetadataPreimageError> {
        let rest = self.rest.get(size..).ok_or(MetadataPreimageError::Malformed)?;
        self.rest = rest;
        Ok(())
    }

    /// A `u32` length prefixed byte string of at most `max` bytes; returns its length.
    fn bytes(&mut self, max: usize) -> Result<usize, MetadataPreimageError> {
        let size = usize::try_from(self.u32()?).map_err(|_| MetadataPreimageError::Malformed)?;
        if size > max {
            return Err(MetadataPreimageError::Malformed);
        }
        self.skip(size)?;
        Ok(size)
    }

    /// A compressed public key: borsh's slice encoding of exactly 32 bytes.
    fn public_key(&mut self) -> Result<[u8; 32], MetadataPreimageError> {
        if self.u32()? != 32 {
            return Err(MetadataPreimageError::Malformed);
        }
        self.take::<32>()
    }

    /// A compressed signature: a public nonce and a scalar, each a 32 byte slice.
    fn signature(&mut self) -> Result<(), MetadataPreimageError> {
        self.public_key()?;
        self.public_key()?;
        Ok(())
    }

    /// A `SideChainFeature`; returns its kind and, for a validator node registration or exit, the validator node's
    /// public key.
    fn sidechain_feature(&mut self) -> Result<(SideChainKind, Option<[u8; 32]>), MetadataPreimageError> {
        let (kind, key) = match self.u8()? {
            0 => {
                // `ValidatorNodeRegistration`: signature (public key, signature), claim public key, max epoch.
                let key = self.public_key()?;
                self.signature()?;
                self.public_key()?;
                self.u64()?;
                (SideChainKind::ValidatorNodeRegistration, Some(key))
            },
            1 => {
                // `CodeTemplateRegistration`: author public key, author signature, template name, template version,
                // template type, build info (repo url, commit hash), binary sha, binary url.
                self.public_key()?;
                self.signature()?;
                self.bytes(MAX_TEMPLATE_NAME_SIZE)?;
                self.u16()?;
                match self.u8()? {
                    // `Wasm { abi_version: u16 }`
                    0 => {
                        self.u16()?;
                    },
                    // `Flow`, `Manifest`
                    1 | 2 => {},
                    _ => return Err(MetadataPreimageError::Malformed),
                }
                self.bytes(MAX_URL_SIZE)?;
                self.bytes(MAX_COMMIT_HASH_SIZE)?;
                self.take::<32>()?;
                self.bytes(MAX_URL_SIZE)?;
                (SideChainKind::CodeTemplateRegistration, None)
            },
            2 => {
                // `ConfidentialOutput`: claim public key.
                self.public_key()?;
                (SideChainKind::ConfidentialOutput, None)
            },
            3 => {
                // `ValidatorNodeExit`: signature (public key, signature), activation epoch, max epoch.
                let key = self.public_key()?;
                self.signature()?;
                self.u64()?;
                self.u64()?;
                (SideChainKind::ValidatorNodeExit, Some(key))
            },
            _ => return Err(MetadataPreimageError::Malformed),
        };
        // `sidechain_id: Option<SideChainId>`: a public key and a knowledge proof signature.
        match self.u8()? {
            0 => {},
            1 => {
                self.public_key()?;
                self.signature()?;
            },
            _ => return Err(MetadataPreimageError::Malformed),
        }
        Ok((kind, key))
    }
}

#[cfg(test)]
mod test {
    // A panic is the desired failure mode in a test.
    #![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]

    use alloc::{vec, vec::Vec};

    use super::*;

    fn key(byte: u8) -> Vec<u8> {
        let mut out = vec![32, 0, 0, 0];
        out.extend_from_slice(&[byte; 32]);
        out
    }

    fn preimage(features: &[u8], covenant: &[u8], encrypted_data_size: usize, minimum_value_promise: u64) -> Vec<u8> {
        let mut out = vec![0];
        out.extend_from_slice(features);
        out.extend_from_slice(covenant);
        out.extend_from_slice(&u32::try_from(encrypted_data_size).unwrap().to_le_bytes());
        out.extend_from_slice(&vec![0xee; encrypted_data_size]);
        out.extend_from_slice(&minimum_value_promise.to_le_bytes());
        out
    }

    fn features(output_type: u8, maturity: u64, sidechain: Option<&[u8]>) -> Vec<u8> {
        let mut out = vec![0, output_type];
        out.extend_from_slice(&maturity.to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        match sidechain {
            None => out.push(0),
            Some(feature) => {
                out.push(1);
                out.extend_from_slice(feature);
            },
        }
        out.push(0);
        out
    }

    fn vn_registration(vn_key: u8) -> Vec<u8> {
        let mut out = vec![0];
        out.extend(key(vn_key));
        out.extend(key(0x51));
        out.extend(key(0x52));
        out.extend(key(0x53));
        out.extend_from_slice(&9u64.to_le_bytes());
        out.push(0);
        out
    }

    #[test]
    fn default_features_to_a_change_shaped_output_may_skip_the_review() {
        let parsed = parse_metadata_preimage(&preimage(&DEFAULT_OUTPUT_FEATURES, &[0], 80, 0)).unwrap();
        assert!(parsed.has_default_features);
        assert!(parsed.may_skip_review());
        assert_eq!(parsed.output_type, OUTPUT_TYPE_STANDARD);

        let promised = parse_metadata_preimage(&preimage(&DEFAULT_OUTPUT_FEATURES, &[0], 80, 5)).unwrap();
        assert!(!promised.may_skip_review());
        assert_eq!(promised.minimum_value_promise, 5);
    }

    #[test]
    fn maturity_and_a_validator_node_registration_are_read_and_must_be_reviewed() {
        let parsed = parse_metadata_preimage(&preimage(&features(0, 1_000, None), &[0], 80, 0)).unwrap();
        assert!(!parsed.may_skip_review());
        assert_eq!(parsed.maturity, 1_000);

        let parsed =
            parse_metadata_preimage(&preimage(&features(3, 0, Some(&vn_registration(0x77))), &[0], 80, 0)).unwrap();
        assert!(!parsed.may_skip_review());
        assert_eq!(parsed.output_type, 3);
        assert_eq!(parsed.sidechain, Some(SideChainKind::ValidatorNodeRegistration));
        assert_eq!(parsed.validator_node_public_key, Some([0x77; 32]));
    }

    #[test]
    fn a_covenant_a_burn_and_a_coinbase_are_refused() {
        assert_eq!(
            parse_metadata_preimage(&preimage(&DEFAULT_OUTPUT_FEATURES, &[1, 0], 80, 0)),
            Err(MetadataPreimageError::CovenantNotEmpty)
        );
        assert_eq!(
            parse_metadata_preimage(&preimage(&features(OUTPUT_TYPE_BURN, 0, None), &[0], 80, 0)),
            Err(MetadataPreimageError::Burn)
        );
        assert_eq!(
            parse_metadata_preimage(&preimage(&features(OUTPUT_TYPE_COINBASE, 0, None), &[0], 80, 0)),
            Err(MetadataPreimageError::Coinbase)
        );
        let mut extra = features(0, 0, None);
        extra.splice(10..14, [2, 0, 0, 0, 0xab, 0xcd]);
        assert_eq!(
            parse_metadata_preimage(&preimage(&extra, &[0], 80, 0)),
            Err(MetadataPreimageError::Coinbase)
        );
    }

    #[test]
    fn a_preimage_that_is_short_long_or_out_of_range_is_malformed() {
        let good = preimage(&DEFAULT_OUTPUT_FEATURES, &[0], 80, 0);
        for size in 0..good.len() {
            assert_eq!(
                parse_metadata_preimage(&good[..size]),
                Err(MetadataPreimageError::Malformed),
                "{size} bytes"
            );
        }
        let mut long = good.clone();
        long.push(0);
        assert_eq!(parse_metadata_preimage(&long), Err(MetadataPreimageError::Malformed));

        for (offset, value) in [(0, 2), (1, 2), (2, 8), (15, 2), (16, 2)] {
            let mut bad = good.clone();
            bad[offset] = value;
            assert_eq!(
                parse_metadata_preimage(&bad),
                Err(MetadataPreimageError::Malformed),
                "byte {offset} = {value}"
            );
        }
        // Encrypted data below the static size, and above the maximum.
        assert_eq!(
            parse_metadata_preimage(&preimage(&DEFAULT_OUTPUT_FEATURES, &[0], 79, 0)),
            Err(MetadataPreimageError::Malformed)
        );
        assert_eq!(
            parse_metadata_preimage(&preimage(
                &DEFAULT_OUTPUT_FEATURES,
                &[0],
                MAX_ENCRYPTED_DATA_SIZE + 1,
                0
            )),
            Err(MetadataPreimageError::Malformed)
        );
        // A public key that is not 32 bytes.
        let mut short_key = vn_registration(0x77);
        short_key[1] = 31;
        assert_eq!(
            parse_metadata_preimage(&preimage(&features(3, 0, Some(&short_key)), &[0], 80, 0)),
            Err(MetadataPreimageError::Malformed)
        );
    }
}
