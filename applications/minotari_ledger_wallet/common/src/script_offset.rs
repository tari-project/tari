// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Parsing and validation of the `GetScriptOffset` header, and the shared derivation of the sender offset key
//! indexes the reply names.
//!
//! This lives here, rather than in the Ledger application, so that the rules the device relies on can be tested
//! without a device. The application itself only builds for the Ledger targets.

/// The device draws a single random base index and derives `base..base + count` from it, so the reply is a fixed
/// size no matter how many keys were asked for. The bound therefore no longer exists to keep the reply inside one
/// APDU; it exists because the device still performs `count` BIP32 derivations in a single exchange.
pub const MAX_SENDER_OFFSET_KEYS: u64 = 25;

/// Size of the `GetScriptOffset` header, including the account the transport prepends to the first chunk.
pub const SCRIPT_OFFSET_HEADER_SIZE: usize = 32;

/// Size of the `GetScriptOffset` reply: `version(1) | script_offset(32) | base_index(8)`.
pub const SCRIPT_OFFSET_REPLY_SIZE: usize = 41;

/// Why a `GetScriptOffset` header was refused.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ScriptOffsetHeaderError {
    /// The chunk was not [`SCRIPT_OFFSET_HEADER_SIZE`] bytes.
    WrongLength,
    /// The host asked for a script offset that no device generated key would blind.
    NoSenderOffsetKeys,
    /// More keys were asked for than the device will derive in one exchange.
    TooManySenderOffsetKeys,
    /// No script side term of the sum was derived on the device.
    NoDeviceScriptKeys,
}

/// The `GetScriptOffset` header, as sent in chunk 0.
///
/// This type only exists for a header that passed every check, so a caller cannot accidentally act on a rejected
/// one. That matters because the device accumulates a script offset across several exchanges: a header that were
/// written and then rejected would leave the counts the host asked for in place for the next chunk to act on.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct ScriptOffsetHeader {
    pub account: u64,
    pub sender_offset_count: u64,
    pub script_index_count: u64,
    pub derived_script_key_count: u64,
}

/// Check that a script offset will be blinded by at least one key the device generated.
///
/// An unblinded reply is the plain sum of the input script private keys. Those keys are `H("script key", b) + alpha`
/// for blinding factors `b` the host chose, so the host can subtract the hashes it already knows and be left with
/// `alpha`, the wallet's root spend key. The count must therefore be re-checked at the moment the value would leave
/// the device, not only when the header is parsed.
pub fn check_sender_offset_key_count(sender_offset_count: u64) -> Result<(), ScriptOffsetHeaderError> {
    if sender_offset_count == 0 {
        return Err(ScriptOffsetHeaderError::NoSenderOffsetKeys);
    }
    if sender_offset_count > MAX_SENDER_OFFSET_KEYS {
        return Err(ScriptOffsetHeaderError::TooManySenderOffsetKeys);
    }
    Ok(())
}

/// Check that at least one script side term of the sum was derived on the device.
///
/// This is the mirror of [`check_sender_offset_key_count`]. With no script keys at all the reply is `-k_sender` for
/// a key the device generated, which hands the host a `OneSidedSenderOffset` private key: enough to recompute the
/// one sided Diffie-Hellman secrets for that output and re-sign its metadata signature without the device, which is
/// the whole point of the approval the device asked the user for.
///
/// `partial_script_key_sum` is deliberately not counted. It is one opaque 32 byte scalar, and a host with no script
/// keys sends the zero scalar, which is indistinguishable from a legitimate sum that happens to be zero. Only
/// pre-mine indexes and alpha derived blinding factors are terms the device itself turned into key material.
pub fn check_script_key_count(
    script_index_count: u64,
    derived_script_key_count: u64,
) -> Result<(), ScriptOffsetHeaderError> {
    if script_index_count.saturating_add(derived_script_key_count) == 0 {
        return Err(ScriptOffsetHeaderError::NoDeviceScriptKeys);
    }
    Ok(())
}

/// The chunk number of the first payload chunk, i.e. the first chunk after the header and the host's partial sum.
const FIRST_PAYLOAD_CHUNK: u64 = 2;

/// Which section of the `GetScriptOffset` payload a chunk number falls in.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ScriptKeySection {
    /// A pre-mine script key named by index. The device derives it, so it counts towards the blinding.
    IndexedScriptKey,
    /// A blinding factor the device folds into `alpha`. The device derives the key, so it counts too.
    DerivedScriptKey,
    /// No section: this chunk carries nothing that is folded into the script offset.
    ///
    /// A host is free to send one of these and then terminate the exchange, which is exactly how a request can
    /// declare script keys and fold none - see [`check_offset_is_blinded`].
    None,
}

/// Decide which section a chunk number belongs to.
///
/// The sections are laid out back to back after the header and the partial sum:
/// `[2, 2 + script_indexes)` then `[2 + script_indexes, 2 + script_indexes + derived_script_keys)`.
///
/// The counts are host supplied and unbounded, so the bounds saturate rather than wrap. A wrapped bound would move
/// which chunk numbers land in which section, and could make an out of range chunk appear in range. Saturation can
/// only ever *widen* a section up to `u64::MAX`, which at worst folds a device derived key for a chunk the host did
/// not mean - it can never cause a chunk to fold nothing while the caller believes it did, and it can never make the
/// two sections overlap, because the second starts where the first ends.
pub fn script_key_section(
    chunk_number: u64,
    total_script_indexes: u64,
    total_derived_script_keys: u64,
) -> ScriptKeySection {
    let end_script_indexes = FIRST_PAYLOAD_CHUNK.saturating_add(total_script_indexes);
    if (FIRST_PAYLOAD_CHUNK..end_script_indexes).contains(&chunk_number) {
        return ScriptKeySection::IndexedScriptKey;
    }
    let end_derived_script_keys = end_script_indexes.saturating_add(total_derived_script_keys);
    if (end_script_indexes..end_derived_script_keys).contains(&chunk_number) {
        return ScriptKeySection::DerivedScriptKey;
    }
    ScriptKeySection::None
}

/// Decide whether an accumulated script offset is safe to hand back.
///
/// This is the check that guards the reply, and it is deliberately not the same check the header passes. A header
/// carries the counts the *host declared*; this takes `device_script_keys_folded`, the number of script keys the
/// device actually turned into key material and added to the sum. The two are independent, and only the second one
/// says anything about whether the reply is blinded.
///
/// The gap between them is exploitable in two APDUs. A host declares `derived_script_key_count = 1`, then sends a
/// terminating chunk whose number falls *outside* the range that section's chunks occupy: nothing is folded, the
/// script side of the sum is still zero, and a check on the declared count would still pass. The reply would be
/// `-k_sender` for a key the device had just generated, and the base index that names it is in the same reply -
/// which is precisely the sender offset private key the device exists to keep from the host.
///
/// `partial_script_key_sum` must not count towards `device_script_keys_folded`. It is one opaque scalar the host
/// computed itself, so it blinds nothing against the host; a host with no script keys sends zero, which the device
/// cannot tell apart from a legitimate sum of zero.
pub fn check_offset_is_blinded(
    sender_offset_count: u64,
    device_script_keys_folded: u64,
) -> Result<(), ScriptOffsetHeaderError> {
    check_sender_offset_key_count(sender_offset_count)?;
    if device_script_keys_folded == 0 {
        return Err(ScriptOffsetHeaderError::NoDeviceScriptKeys);
    }
    Ok(())
}

/// The index of the `i`th sender offset key derived from `base_index`.
///
/// The base is drawn from the device RNG and may sit anywhere in `u64`, so the walk wraps rather than saturating.
/// Both sides call this, so a base near `u64::MAX` needs no special case on either.
pub fn sender_offset_index(base_index: u64, i: u64) -> u64 {
    base_index.wrapping_add(i)
}

fn read_u64(data: &[u8], start: usize) -> Result<u64, ScriptOffsetHeaderError> {
    let bytes = data
        .get(start..start.saturating_add(8))
        .ok_or(ScriptOffsetHeaderError::WrongLength)?;
    let mut field = [0u8; 8];
    field.copy_from_slice(bytes);
    Ok(u64::from_le_bytes(field))
}

/// Parse and validate a `GetScriptOffset` header.
///
/// Layout: `account(8) | sender_offset_count(8) | script_index_count(8) | derived_script_key_count(8)`.
pub fn parse_script_offset_header(data: &[u8]) -> Result<ScriptOffsetHeader, ScriptOffsetHeaderError> {
    if data.len() != SCRIPT_OFFSET_HEADER_SIZE {
        return Err(ScriptOffsetHeaderError::WrongLength);
    }

    let account = read_u64(data, 0)?;
    let sender_offset_count = read_u64(data, 8)?;
    let script_index_count = read_u64(data, 16)?;
    let derived_script_key_count = read_u64(data, 24)?;

    check_sender_offset_key_count(sender_offset_count)?;
    check_script_key_count(script_index_count, derived_script_key_count)?;

    Ok(ScriptOffsetHeader {
        account,
        sender_offset_count,
        script_index_count,
        derived_script_key_count,
    })
}

#[cfg(test)]
mod test {
    use alloc::vec::Vec;

    use super::*;

    fn header_bytes(account: u64, sender_offset_count: u64, script_indexes: u64, derived_script_keys: u64) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&account.to_le_bytes());
        data.extend_from_slice(&sender_offset_count.to_le_bytes());
        data.extend_from_slice(&script_indexes.to_le_bytes());
        data.extend_from_slice(&derived_script_keys.to_le_bytes());
        data
    }

    #[test]
    fn it_parses_a_valid_header() {
        let header = parse_script_offset_header(&header_bytes(7, 2, 3, 4)).unwrap();
        assert_eq!(header, ScriptOffsetHeader {
            account: 7,
            sender_offset_count: 2,
            script_index_count: 3,
            derived_script_key_count: 4,
        });
    }

    #[test]
    fn it_rejects_a_header_that_is_not_the_expected_size() {
        assert_eq!(
            parse_script_offset_header(&[]).unwrap_err(),
            ScriptOffsetHeaderError::WrongLength
        );
        let mut too_long = header_bytes(1, 1, 0, 1);
        too_long.push(0);
        assert_eq!(
            parse_script_offset_header(&too_long).unwrap_err(),
            ScriptOffsetHeaderError::WrongLength
        );
    }

    #[test]
    fn it_rejects_more_keys_than_the_device_will_derive_in_one_exchange() {
        assert_eq!(
            parse_script_offset_header(&header_bytes(1, MAX_SENDER_OFFSET_KEYS.saturating_add(1), 0, 1)).unwrap_err(),
            ScriptOffsetHeaderError::TooManySenderOffsetKeys
        );
        assert!(parse_script_offset_header(&header_bytes(1, MAX_SENDER_OFFSET_KEYS, 0, 1)).is_ok());
    }

    /// A host that asks for a script offset with no sender offset keys is asking for the plain sum of the input
    /// script private keys, which reveals the wallet's spend key. The rejection must yield no header at all: if a
    /// rejected header could still be committed to the device's long lived context, the host could follow it with a
    /// chunk that folds an alpha derived script key into the sum and, because the (rejected) count was zero, read
    /// that sum back unblinded. Returning `Err` with no `ScriptOffsetHeader` is what makes that sequence impossible.
    #[test]
    fn it_rejects_a_header_with_no_sender_offset_keys() {
        let rejected = parse_script_offset_header(&header_bytes(1, 0, 0, 1));
        assert_eq!(rejected.unwrap_err(), ScriptOffsetHeaderError::NoSenderOffsetKeys);
        assert!(rejected.is_err(), "a rejected header must not yield anything to act on");
    }

    /// The mirror case: with no device derived script key the reply is `-k_sender` for a key the device just made,
    /// which hands the host a one sided sender offset private key. The rejection must again yield no header, for
    /// the same reason - otherwise a follow up chunk could resume the accumulation the rejection withheld.
    #[test]
    fn it_rejects_a_header_with_no_device_script_keys() {
        let rejected = parse_script_offset_header(&header_bytes(1, 1, 0, 0));
        assert_eq!(rejected.unwrap_err(), ScriptOffsetHeaderError::NoDeviceScriptKeys);
        assert!(rejected.is_err(), "a rejected header must not yield anything to act on");

        // One of either kind of device derived script key is enough.
        assert!(parse_script_offset_header(&header_bytes(1, 1, 1, 0)).is_ok());
        assert!(parse_script_offset_header(&header_bytes(1, 1, 0, 1)).is_ok());
    }

    /// The same checks guard the point at which the offset would actually leave the device, so that a count carried
    /// over from an earlier exchange cannot be trusted on its own.
    #[test]
    fn the_key_count_is_checked_independently_of_parsing() {
        assert_eq!(
            check_sender_offset_key_count(0).unwrap_err(),
            ScriptOffsetHeaderError::NoSenderOffsetKeys
        );
        assert!(check_sender_offset_key_count(1).is_ok());
    }

    /// ...and the two checks are independent of each other: neither count can stand in for the other.
    #[test]
    fn the_script_key_count_is_checked_independently_of_parsing() {
        assert_eq!(
            check_script_key_count(0, 0).unwrap_err(),
            ScriptOffsetHeaderError::NoDeviceScriptKeys
        );
        assert!(check_script_key_count(1, 0).is_ok());
        assert!(check_script_key_count(0, 1).is_ok());
        assert!(check_script_key_count(3, 4).is_ok());
        // A large count is not this check's business; it only asks whether anything was derived on the device.
        assert!(check_script_key_count(u64::MAX, u64::MAX).is_ok());
    }

    /// The two checks do not shadow each other: a header can fail either one on its own.
    #[test]
    fn the_two_checks_are_independent_of_each_other() {
        assert!(check_sender_offset_key_count(1).is_ok() && check_script_key_count(0, 0).is_err());
        assert!(check_sender_offset_key_count(0).is_err() && check_script_key_count(1, 0).is_ok());
    }

    /// The section a chunk falls in decides whether it folds a device derived key, so it decides whether the reply
    /// is blinded. This is the arithmetic the critical leak turned on: chunk 3 with `(0 indexes, 1 derived)` is
    /// outside every section, so a host that terminates there folds nothing.
    #[test]
    fn the_poc_boundary_is_where_the_sections_end() {
        // (0 indexes, 1 derived): the derived section is exactly chunk 2.
        assert_eq!(script_key_section(2, 0, 1), ScriptKeySection::DerivedScriptKey);
        assert_eq!(script_key_section(3, 0, 1), ScriptKeySection::None);
        assert_eq!(script_key_section(250, 0, 1), ScriptKeySection::None);

        // (1 index, 0 derived): the indexed section is exactly chunk 2.
        assert_eq!(script_key_section(2, 1, 0), ScriptKeySection::IndexedScriptKey);
        assert_eq!(script_key_section(3, 1, 0), ScriptKeySection::None);
    }

    /// Chunks 0 and 1 carry the header and the host's partial sum. Neither is ever a script key section, whatever
    /// the counts say - which is what keeps `partial_script_key_sum` from counting towards the blinding.
    #[test]
    fn the_header_and_partial_sum_chunks_are_never_a_script_key_section() {
        for counts in [(0, 0), (1, 1), (u64::MAX, u64::MAX)] {
            assert_eq!(script_key_section(0, counts.0, counts.1), ScriptKeySection::None);
            assert_eq!(script_key_section(1, counts.0, counts.1), ScriptKeySection::None);
        }
    }

    /// Empty sections on either side must not swallow chunks meant for the other one.
    #[test]
    fn an_empty_section_folds_nothing() {
        // Both empty: nothing is ever folded, whatever the host sends.
        for chunk in 0..8 {
            assert_eq!(script_key_section(chunk, 0, 0), ScriptKeySection::None);
        }
        // An empty indexed section leaves the derived section starting at the first payload chunk.
        assert_eq!(script_key_section(2, 0, 2), ScriptKeySection::DerivedScriptKey);
        assert_eq!(script_key_section(3, 0, 2), ScriptKeySection::DerivedScriptKey);
        assert_eq!(script_key_section(4, 0, 2), ScriptKeySection::None);
        // An empty derived section leaves nothing after the indexed one.
        assert_eq!(script_key_section(3, 2, 0), ScriptKeySection::IndexedScriptKey);
        assert_eq!(script_key_section(4, 2, 0), ScriptKeySection::None);
    }

    /// Adjacent sections must abut exactly: no gap that silently folds nothing, and no overlap that would make a
    /// chunk ambiguous.
    #[test]
    fn adjacent_sections_abut_without_a_gap_or_an_overlap() {
        // 2 indexed then 3 derived: chunks 2,3 indexed; 4,5,6 derived; 7 onwards nothing.
        let expected = [
            (2, ScriptKeySection::IndexedScriptKey),
            (3, ScriptKeySection::IndexedScriptKey),
            (4, ScriptKeySection::DerivedScriptKey),
            (5, ScriptKeySection::DerivedScriptKey),
            (6, ScriptKeySection::DerivedScriptKey),
            (7, ScriptKeySection::None),
        ];
        for (chunk, section) in expected {
            assert_eq!(script_key_section(chunk, 2, 3), section, "chunk {chunk}");
        }
    }

    /// The counts are host supplied and unbounded, so the section bounds saturate. Pin what saturation can and
    /// cannot do: it may widen a section, but it must never make an out of range chunk appear in range in the
    /// *other* section, and the two must not overlap.
    #[test]
    fn saturated_bounds_cannot_be_played_against_each_other() {
        // A saturated indexed section swallows every payload chunk; the derived section is then empty, so no chunk
        // can be claimed by both.
        for chunk in 2..8 {
            assert_eq!(
                script_key_section(chunk, u64::MAX, u64::MAX),
                ScriptKeySection::IndexedScriptKey
            );
        }
        // Every chunk it swallows still folds a device derived key, which is the safe direction: a chunk can be
        // attributed to the wrong section, but it can never be attributed to no section while folding one.
        assert_ne!(script_key_section(2, u64::MAX, 0), ScriptKeySection::None);

        // A saturated derived section behaves the same way once the indexed one is empty.
        for chunk in 2..8 {
            assert_eq!(
                script_key_section(chunk, 0, u64::MAX),
                ScriptKeySection::DerivedScriptKey
            );
        }

        // And a count that only just reaches the top of the range still ends the section where it should.
        assert_eq!(
            script_key_section(2, u64::MAX.saturating_sub(2), 0),
            ScriptKeySection::IndexedScriptKey
        );
    }

    /// The device widens a `u8` chunk number to call this, so every value it can pass is covered.
    #[test]
    fn every_chunk_number_the_wire_can_carry_is_classified() {
        for chunk in 0..=u8::MAX {
            let section = script_key_section(u64::from(chunk), 0, 1);
            if chunk == 2 {
                assert_eq!(section, ScriptKeySection::DerivedScriptKey);
            } else {
                assert_eq!(section, ScriptKeySection::None, "chunk {chunk}");
            }
        }
    }

    /// The check that guards the reply must look at what was folded, not at what was declared. A host that declares
    /// device script keys and then terminates the exchange on a chunk number outside every section's range folds
    /// nothing at all, and the reply would be a bare `-k_sender` for a key the device had just generated.
    #[test]
    fn a_declared_script_key_that_was_never_folded_does_not_unblind_the_reply() {
        // The proof of concept: header declares `derived_script_key_count = 1`, nothing is ever folded.
        assert!(
            check_script_key_count(0, 1).is_ok(),
            "the declared counts pass, which is exactly why they cannot be what guards the reply"
        );
        assert_eq!(
            check_offset_is_blinded(1, 0).unwrap_err(),
            ScriptOffsetHeaderError::NoDeviceScriptKeys
        );

        // One key actually folded, of either kind, is enough.
        assert!(check_offset_is_blinded(1, 1).is_ok());
        assert!(check_offset_is_blinded(25, 3).is_ok());
    }

    /// The reply is still refused when no sender offset key would blind it, whatever was folded on the script side.
    #[test]
    fn the_reply_check_covers_both_sides_of_the_sum() {
        assert_eq!(
            check_offset_is_blinded(0, 5).unwrap_err(),
            ScriptOffsetHeaderError::NoSenderOffsetKeys
        );
        assert_eq!(
            check_offset_is_blinded(MAX_SENDER_OFFSET_KEYS.saturating_add(1), 5).unwrap_err(),
            ScriptOffsetHeaderError::TooManySenderOffsetKeys
        );
        assert_eq!(
            check_offset_is_blinded(0, 0).unwrap_err(),
            ScriptOffsetHeaderError::NoSenderOffsetKeys
        );
    }

    /// The reply names the sender offset keys by a single base index, so both sides have to walk it identically.
    #[test]
    fn the_index_walk_is_shared_by_both_sides() {
        assert_eq!(sender_offset_index(10, 0), 10);
        assert_eq!(sender_offset_index(10, 3), 13);

        // A base within `count` of the end of the range wraps rather than saturating, so the device and the host
        // still name the same keys.
        let base = u64::MAX.saturating_sub(1);
        assert_eq!(sender_offset_index(base, 0), u64::MAX - 1);
        assert_eq!(sender_offset_index(base, 1), u64::MAX);
        assert_eq!(sender_offset_index(base, 2), 0);
        assert_eq!(sender_offset_index(base, 3), 1);

        // Every index in a full sized request from that base is distinct.
        let mut seen = Vec::new();
        for i in 0..MAX_SENDER_OFFSET_KEYS {
            let index = sender_offset_index(base, i);
            assert!(!seen.contains(&index), "index {index} was derived twice");
            seen.push(index);
        }
    }
}
