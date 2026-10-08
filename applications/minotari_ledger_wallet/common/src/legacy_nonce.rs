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
//! The residual: the pre-mine flow reserves its nonces in step 2 and signs in step 3 with a file in between, so one
//! nonce index can still be signed with twice - in the same device session or across a device restart. The device
//! deliberately keeps no record of which nonce indexes it has signed with (it stores nothing for this instruction, in
//! RAM or in NVM), and neither does the host.
//!
//! Three attackers get there, and only the first needs a compromised host:
//!
//! - **A compromised host**, which can ask for any legacy signature it likes.
//! - **A malicious pre-mine leader, against an honest host.** Both step 4 challenges are built entirely from fields the
//!   leader's step 3 file supplies (the script and metadata signature ephemerals, `total_script_key`, the sender offset
//!   public key), and the nonce ids come from the party's own step 2 self file, which nothing marks as spent. A leader
//!   that sends a second step 3 file differing in one ephemeral value and asks the party to re-run step 4 gets two
//!   signatures under each nonce over different challenges. The two device reviews are identical - same keys, same
//!   nonce indexes - which is exactly what a legitimate retry looks like.
//! - **Anyone who can write the session directory**, against an honest host: the step 2 self file names the keys and
//!   nonces step 4 signs with. Step 4 holds the file to the shape step 2 writes - every nonce and every sender offset
//!   key distinct across the whole file, every nonce a `Random` key, each script key the unmarked `PreMine` key at its
//!   output index, each sender offset key a marked `PreMine` key - which stops a file that names one nonce twice, in
//!   one output or across two. That check is per *file*. Every session lives under one directory
//!   (`~/Documents/tari_pre_mine/spend/`, often cloud-synced), so a writer can copy a nonce id that an earlier session
//!   already signed with into a new session's self file: one ordinary step 4 run of the new session - normal-looking
//!   screens, no re-run - is then a second signature under that nonce. It also does nothing against a re-run of the
//!   same file, a leader-induced re-run, or a compromised host.
//!
//! Under the rule that the device stores nothing, the only closure on the host side is a record of legacy nonces,
//! kept by the wallet and checked before every legacy signature. That is a tracked follow-up, not part of this change.
//! To close the attacks above it has to be:
//!
//! 1. **global across sessions**, not per session file;
//! 2. **kept in the wallet database**, outside the session directory the files live in;
//! 3. an **allowlist** of the nonce indexes step 2 issued, each consumed *before* it is signed with - a denylist of
//!    indexes already signed with fails open on a restored wallet or on another machine, which have never seen them;
//! 4. **keyed on the full `u64` nonce index** - the account path element still wraps modulo `2^32`, and the review does
//!    not show the account - with a log of `(nonce index, H(challenge))` for audit.
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
//! What bounds it: `alpha` stays out of reach (see above), and the exposure is contained to pre-mine outputs. Within
//! them it is not bounded by the multisig. Every pre-mine output's script is `CheckHeight(fail_safe_height) LeZero
//! IfThen CheckMultiSigVerifyAggregatePubKey(..) Else PushPubKey(backup_key) EndIf`, and the backup key is a `PreMine`
//! key at a small, host-known index (the backup spend names it from the output's payment id). It is one of the keys
//! this instruction signs - labelled "Pre-mine script signature" - so whoever recovers it can spend that output
//! *alone* once `fail_safe_height` has passed. The multisig protects only the pre-fail-safe path, and some schedule
//! entries set `fail_safe_height` to the payout period, which may already be behind the tip.
//!
//! What extraction adds depends on who does it:
//!
//! - **A compromised host gains only permanence.** It can already spend any pre-mine output this device holds keys for,
//!   with no prompt at all: `GetScriptSignatureManaged`, the handle based `GetRawSchnorrSignature` and pre-mine mode
//!   `GetScriptOffset` all use `PreMine` keys without a review. That is a pre-existing gap outside this change, tracked
//!   separately. Extracting the key only means it keeps working after the compromise ends.
//! - **For the malicious leader and the session directory writer, extraction is the whole capability.** Neither has
//!   device access, so the key is the only way they get to spend.
//!
//! Folding the signing key's branch and index into the legacy nonce derivation would close the cross-key case without
//! any device state - a nonce index would then name a different nonce under every key - but it changes what step 2
//! reserves and step 3 signs with, so it belongs with the TODO below rather than in front of it. It would not close a
//! re-run of step 4 under one key.
//!
//! One more boundary: an application from before the 64-bit index split derived nonces from the index modulo `2^32`, so
//! its nonce at host index `j` is this application's nonce at `j mod 2^32`. The legacy instruction therefore refuses a
//! nonce index below `2^32` ([`LEGACY_NONCE_INDEX_FLOOR`]), and the host draws its nonces above it. A legacy signature
//! from an older application can never be combined with one from this one - and an old step 2 session cannot be
//! finished on this application, which it could not have been anyway.
//!
//! # The fix, and what gets deleted with it
//!
//! TODO: Close the reuse, without any persistent data on the device - that option is ruled out. Either:
//!
//! - **same-session reservation**: teach the pre-mine flow to carry device-issued nonce handles instead of nonce
//!   branches and indexes, and to reserve its nonces in the same device session that signs with them (handles survive a
//!   file fine; what they cannot survive is the device restarting between the two steps). Its feasibility is open: the
//!   device nonce store is 8 RAM slots, so 4 outputs per device session at two nonces each, and the application would
//!   have to stay open across the leader's step 3 round trip. Neither constraint has been examined; or
//! - **a host-side nonce ledger**: the wallet keeps the record described above - global, in the wallet database, an
//!   allowlist consumed before signing, keyed on the full nonce index - which keeps the file-based flow but trusts the
//!   host to keep the record.
//!
//! Only the first lets everything below be deleted.
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

/// The smallest nonce index the legacy instruction will sign with: `2^32`.
///
/// Before the 64-bit index split the device derived from an index modulo `2^32`, so an old application's nonce at index
/// `j` sat at exactly the path the current application uses for the small index `j mod 2^32` (the high word element was
/// a constant `0`). Host nonce indexes are random `u64`s, so almost every old legacy nonce is reachable that way: a
/// host holding one old legacy signature - old step 4 output carries them - could ask for a single new signature under
/// `(PreMine i, Random j mod 2^32)`, with an index on screen the user has never seen, and solve the two for the key.
///
/// An index at or above `2^32` has a non-zero high word element, which no old application ever derived from, so a
/// new-application nonce can never be an old-application one. The cost is that a signature from an old application
/// cannot be combined with one from this application - which is the point.
pub const LEGACY_NONCE_INDEX_FLOOR: u64 = 1 << 32;

/// A legacy nonce index below [`LEGACY_NONCE_INDEX_FLOOR`].
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct LegacyNonceIndexBelowFloor;

/// Check that a legacy nonce index cannot name a nonce an application from before the 64-bit index split derived. See
/// [`LEGACY_NONCE_INDEX_FLOOR`]. The device refuses with `BadBranchKey` - the same status word as the branch
/// whitelist, because it is the same kind of refusal: a nonce the legacy instruction will not sign with - before
/// its review; the host mirrors it.
pub fn check_legacy_nonce_index(nonce_index: u64) -> Result<(), LegacyNonceIndexBelowFloor> {
    if nonce_index < LEGACY_NONCE_INDEX_FLOOR {
        return Err(LegacyNonceIndexBelowFloor);
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

    /// Nonce indexes below `2^32` alias an old application's nonces and are refused; everything from `2^32` up is
    /// allowed.
    #[test]
    fn nonce_indexes_below_2_32_are_refused() {
        for index in [0, 1, 0x8765_4321, u64::from(u32::MAX)] {
            assert_eq!(
                check_legacy_nonce_index(index),
                Err(LegacyNonceIndexBelowFloor),
                "{index}"
            );
        }
        for index in [1 << 32, (1 << 32) | 0x8765_4321, 1 << 63, u64::MAX] {
            assert_eq!(check_legacy_nonce_index(index), Ok(()), "{index}");
        }
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
