// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! The branch whitelist for `GetRawSchnorrSignatureLegacyNonce`, and what allowing it costs.
//!
//! **This module is the canonical description of the pre-mine nonce gap. Every other site that touches the legacy
//! instruction points here rather than restating it.**
//!
//! # What the legacy instruction is
//!
//! `GetRawSchnorrSignature` takes a nonce handle: the device draws the nonce, keeps it, and consumes it on use, so
//! a nonce cannot be signed with twice. `GetRawSchnorrSignatureLegacyNonce` is the instruction it replaced - the
//! host names the nonce by branch and index, and the device re-derives the same scalar every time it is asked.
//!
//! It still exists for exactly one caller: the pre-mine spend flow. That flow reserves its nonces in step 2 and
//! signs with them in step 3, with N of them outstanding across a *session file* rather than a device session. The
//! device's nonce store is eight entries of RAM that vanish when the application exits, so it cannot serve them.
//!
//! # What allowing it costs
//!
//! A deterministic nonce is a key disclosure waiting for a second challenge. A compromised host can ask for two
//! legacy signatures over the same `(key index, nonce index)` pair with different challenges `e1 != e2`, and then:
//!
//! - recover the nonce as `k = (s1 - s2) / (e1 - e2)`,
//! - recover the private key as `x = (s1 - k) / e1`.
//!
//! So every key the whitelist below admits is a key a compromised host can extract.
//!
//! ## Before: this reached `alpha`
//!
//! The whitelist used to admit [`LedgerKeyBranch::OneSidedSenderOffset`] (and `Random`) private keys, at any index.
//! That did not stop at sender offset keys. `GetScriptOffset` answers `s = H(b) + alpha - k_i` for any blinding
//! factor `b` the host names, and returns the index `i` of the sender offset key `k_i` it just derived - on the
//! `OneSidedSenderOffset` branch. Three APDUs then gave up the wallet's root spend key, with nothing on the screen:
//!
//! 1. `GetScriptOffset` over one alpha derived script key with a known `b` yields `s` and `i`;
//! 2. two legacy signatures under `(OneSidedSenderOffset, i)` and one `(Random, j)` nonce over different challenges
//!    yield `k_i`;
//! 3. `alpha = s - H(b) + k_i`.
//!
//! ## After: pre-mine script and sender offset keys only
//!
//! - The whitelist admits [`LedgerKeyBranch::PreMine`] private keys and nothing else.
//! - `GetScriptOffset` derives sender offset keys on `PreMine` only when no alpha derived script key is in the sum (see
//!   `crate::script_offset::sender_offset_branch`). A reply with `alpha` in it is blinded by a `OneSidedSenderOffset`
//!   key, which this instruction cannot sign, and a reply blinded by a `PreMine` key has no `alpha` in it. Nothing the
//!   legacy instruction can sign is a term of a reply that contains `alpha`.
//! - The device shows a review for every legacy signature - key and nonce, branch and index, and which pre-mine
//!   signature it is (see [`legacy_signature_purpose`]). The indexes shown are the full `u64`s the host sent, and the
//!   device derives from every bit of them (the high and low words are separate BIP32 path elements), so two requests
//!   the screen tells apart are requests for different keys. When the device derived from the index modulo `2^32`, a
//!   host could dress one key and nonce up as two requests - `i` and `2^63 | i`, or nonces `j` and `j + 2^32`. The
//!   review is *not* a reliable control against nonce reuse, though; see the residual below.
//!
//! What remains extractable is pre-mine script keys and pre-mine sender offset keys. A pre-mine output's script
//! offset is its script key minus its sender offset key, so recovering either gives up the other.
//!
//! # Scope
//!
//! This applies only to keys signed through the legacy instruction, which is only the pre-mine spend flow. Normal
//! spend flows are unaffected: they sign through `GetRawSchnorrSignature` with a device-issued handle that the host
//! can neither choose nor redeem twice, so there is no second signature to difference against.
//!
//! The residual: the pre-mine flow reserves its nonces in step 2 and signs in step 3 with a file in between, so a
//! compromised host can still sign twice under one nonce index - in the same device session or across a device
//! restart. The device deliberately keeps no record of which nonce indexes it has signed with (it stores nothing for
//! this instruction, in RAM or in NVM).
//!
//! It does not need the *same pair* to do it. Reusing one nonce index across **any two `PreMine` keys** leaks both,
//! because `GetScriptOffset` hands the host linear relations between pre-mine keys with no prompt at all: step 2 gives
//! it `R = k_A - k_S` for script key `A` and sender offset key `S`. One legacy signature under `(PreMine A, Random j)`
//! and one under `(PreMine S, Random j)` over different challenges give `s1 - s2 = (e1 - e2)·k_A + e2·R`, so `k_A`, and
//! `k_S` with it. Two `GetScriptOffset([A])` replies likewise give `k_S2 - k_S1`, which relates two sender offset keys.
//! The two reviews read "Pre-mine script signature" and "Pre-mine metadata signature" - exactly what a legitimate step
//! 3 shows - and the only tell is the same nonce index on both screens. A careful user can notice that; it is not a
//! control to rely on.
//!
//! What bounds it: `alpha` stays out of reach (see above), the exposure is contained to pre-mine outputs, and those are
//! multisig, so one leaked key share is not theft by itself.
//!
//! Folding the signing key's branch and index into the legacy nonce derivation would close this without any device
//! state - a nonce index would then name a different nonce under every key - but it changes what step 2 reserves and
//! step 3 signs with, so it belongs with the TODO below rather than in front of it.
//!
//! # The fix, and what gets deleted with it
//!
//! TODO: Teach the pre-mine step 2 / step 3 session file to carry device-issued nonce handles instead of nonce
//! branches and indexes. Handles survive a file fine - what they cannot survive is the device restarting between
//! the two steps, so this also needs the pre-mine flow to reserve its nonces in the same device session that signs
//! with them, or the device store to be made persistent.
//!
//! When that lands, these get deleted together:
//!
//! - `Instruction::GetRawSchnorrSignatureLegacyNonce` (this crate's `common_types`)
//! - `handler_get_raw_schnorr_signature_legacy_nonce` (the Ledger application)
//! - `ledger_get_raw_schnorr_signature_legacy_nonce` (`minotari_ledger_wallet_comms::accessor_methods`)
//! - `KeyManager::ledger_get_raw_schnorr_signature_legacy_nonce_wrapper` and the `(LedgerKey, LedgerKey)` arm of
//!   `sign_with_nonce_and_challenge`
//! - this module

use crate::{common_types::LedgerKeyBranch, script_offset::is_pre_mine_sender_offset_index};

/// Why a `GetRawSchnorrSignatureLegacyNonce` request was refused.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum LegacyNonceBranchError {
    /// The private key branch is not one the pre-mine spend flow signs with.
    KeyBranchNotAllowed,
    /// The nonce branch is not the one the pre-mine spend flow reserves from.
    NonceBranchNotAllowed,
}

/// Check that a legacy nonce request is one the pre-mine spend flow could actually have made.
///
/// The whitelist is the containment: signing with a deterministic nonce is equivalent to handing the private key
/// over (see the module docs), so the exposure is held to the one branch pre-mine signs with. The pre-mine script
/// signature and the pre-mine metadata signature both sign a `PreMine` key: the script key at its genesis output
/// index, and the sender offset key `GetScriptOffset` derived for it on `PreMine` in pre-mine mode.
///
/// `OneSidedSenderOffset` is refused because `GetScriptOffset` hands out its indexes next to replies that contain
/// `alpha`; `Random` and `Spend` are refused because pre-mine never signs with them.
///
/// This lives in the shared crate, and is checked on both sides, so the two whitelists cannot drift apart. The
/// device's check is the one that counts; the host mirrors it so a caller gets a legible error instead of a status
/// word, and so a request the device would refuse never reaches the wire.
pub fn check_legacy_nonce_branches(
    private_key_branch: LedgerKeyBranch,
    nonce_branch: LedgerKeyBranch,
) -> Result<(), LegacyNonceBranchError> {
    if private_key_branch != LedgerKeyBranch::PreMine {
        return Err(LegacyNonceBranchError::KeyBranchNotAllowed);
    }
    // Pinned so the instruction cannot be turned into a way to extract a key on some other branch by pointing its
    // nonce there. Pre-mine reserves both of its nonces from `Random`.
    if nonce_branch != LedgerKeyBranch::Random {
        return Err(LegacyNonceBranchError::NonceBranchNotAllowed);
    }
    Ok(())
}

/// The purpose line the device shows when asked for a legacy signature by the `PreMine` key at `key_index`.
///
/// Pre-mine sender offset keys are derived at indexes with
/// [`PRE_MINE_SENDER_OFFSET_INDEX_BIT`](crate::script_offset::PRE_MINE_SENDER_OFFSET_INDEX_BIT) set, and pre-mine
/// script keys at their genesis output index, which never has it. So the index says which of the two signatures this
/// is: the script signature signs with the script key, and the metadata signature with the sender offset key. The
/// marker is bit 63, and the device derives from all 64 bits, so the label is a property of the key being signed.
pub fn legacy_signature_purpose(key_index: u64) -> &'static str {
    if is_pre_mine_sender_offset_index(key_index) {
        "Pre-mine metadata signature"
    } else {
        "Pre-mine script signature"
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::script_offset::PRE_MINE_SENDER_OFFSET_INDEX_BIT;

    const ALL_BRANCHES: [LedgerKeyBranch; 4] = [
        LedgerKeyBranch::OneSidedSenderOffset,
        LedgerKeyBranch::Random,
        LedgerKeyBranch::PreMine,
        LedgerKeyBranch::Spend,
    ];

    /// The pre-mine spend flow signs both its script signature and its metadata signature with a `PreMine` key,
    /// against a `Random` nonce. That pair, and only that pair, is allowed.
    #[test]
    fn only_a_pre_mine_key_with_a_random_nonce_is_allowed() {
        let mut allowed = 0u32;
        for key_branch in ALL_BRANCHES {
            for nonce_branch in ALL_BRANCHES {
                if check_legacy_nonce_branches(key_branch, nonce_branch).is_ok() {
                    allowed = allowed.saturating_add(1);
                    assert_eq!(
                        (key_branch, nonce_branch),
                        (LedgerKeyBranch::PreMine, LedgerKeyBranch::Random)
                    );
                }
            }
        }
        assert_eq!(allowed, 1);
    }

    /// `OneSidedSenderOffset` is the branch `GetScriptOffset` blinds `alpha` with. Signing it here is what recovered
    /// `alpha`; see the module docs.
    #[test]
    fn the_sender_offset_branch_is_refused() {
        assert_eq!(
            check_legacy_nonce_branches(LedgerKeyBranch::OneSidedSenderOffset, LedgerKeyBranch::Random),
            Err(LegacyNonceBranchError::KeyBranchNotAllowed)
        );
    }

    /// Every key branch but `PreMine` is refused, `alpha` and the old pre-mine `Random` sender offsets included.
    #[test]
    fn every_other_key_branch_is_refused() {
        for key_branch in [
            LedgerKeyBranch::OneSidedSenderOffset,
            LedgerKeyBranch::Random,
            LedgerKeyBranch::Spend,
        ] {
            assert_eq!(
                check_legacy_nonce_branches(key_branch, LedgerKeyBranch::Random),
                Err(LegacyNonceBranchError::KeyBranchNotAllowed),
                "{key_branch}"
            );
        }
    }

    /// Every branch other than `Random` is refused as a nonce.
    #[test]
    fn only_the_random_branch_may_supply_a_nonce() {
        for nonce_branch in [
            LedgerKeyBranch::PreMine,
            LedgerKeyBranch::OneSidedSenderOffset,
            LedgerKeyBranch::Spend,
        ] {
            assert_eq!(
                check_legacy_nonce_branches(LedgerKeyBranch::PreMine, nonce_branch),
                Err(LegacyNonceBranchError::NonceBranchNotAllowed)
            );
        }
    }

    /// A bad key branch is reported even when the nonce branch is bad too, so the caller fixes the one that is
    /// actually the containment boundary first.
    #[test]
    fn the_key_branch_is_checked_before_the_nonce_branch() {
        assert_eq!(
            check_legacy_nonce_branches(LedgerKeyBranch::Spend, LedgerKeyBranch::PreMine),
            Err(LegacyNonceBranchError::KeyBranchNotAllowed)
        );
    }

    /// Script keys sit at genesis output indexes; sender offsets have bit 63 set.
    #[test]
    fn the_purpose_follows_the_index() {
        assert_eq!(legacy_signature_purpose(0), "Pre-mine script signature");
        assert_eq!(legacy_signature_purpose(12_345), "Pre-mine script signature");
        assert_eq!(legacy_signature_purpose(1 << 32), "Pre-mine script signature");
        assert_eq!(
            legacy_signature_purpose(PRE_MINE_SENDER_OFFSET_INDEX_BIT - 1),
            "Pre-mine script signature"
        );
        assert_eq!(
            legacy_signature_purpose(PRE_MINE_SENDER_OFFSET_INDEX_BIT),
            "Pre-mine metadata signature"
        );
        assert_eq!(
            legacy_signature_purpose(PRE_MINE_SENDER_OFFSET_INDEX_BIT | 12_345),
            "Pre-mine metadata signature"
        );
        assert_eq!(legacy_signature_purpose(u64::MAX), "Pre-mine metadata signature");
    }

    /// `i` and `2^63 | i` used to derive one key and get two labels. They are different keys now, so the two labels
    /// describe two different signatures.
    #[test]
    fn indexes_that_differ_above_bit_31_are_labelled_by_their_own_marker() {
        let i = 12_345u64;
        assert_eq!(legacy_signature_purpose(i), "Pre-mine script signature");
        assert_eq!(
            legacy_signature_purpose(PRE_MINE_SENDER_OFFSET_INDEX_BIT | i),
            "Pre-mine metadata signature"
        );
    }
}
