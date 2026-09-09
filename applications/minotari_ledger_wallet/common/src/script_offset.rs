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
