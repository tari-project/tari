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
//!   signature it is (see [`legacy_signature_purpose`]).
//! - The device remembers which nonce indexes it has signed with ([`LegacyNonceUse`]) and refuses any second use but
//!   the identical request, and refuses nonce indexes below 2^32 ([`check_legacy_nonce_index`]).
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
//! # Residual
//!
//! The pre-mine flow reserves its nonces in step 2 and signs with them in step 4, with a session file in between, so
//! the nonce is named by the host. What that still allows, and what holds each case:
//!
//! 1. **Same nonce, two keys.** A compromised host asks for the script signature as `(PreMine x, Random j, e1)` and the
//!    metadata signature as `(PreMine 2^63|b, Random j, e2)`. With the step 2 script offset `off = P_x - K_b` that is
//!    three equations in three unknowns, and both keys fall out - behind exactly the two reviews an honest step 4
//!    shows; the only tell is the same nonce number on both. The device's used-nonce record refuses the second request
//!    (`LegacyNonceReused`, before any screen). It is RAM, so the attack now needs the user to restart the application
//!    between the two approvals.
//! 2. **Malicious leader re-run.** A leader sends a changed step 3 file ("please redo step 4") and the party re-signs
//!    under the same step 2 nonces, with identical screens - or the leader makes step 4 stop after the script signature
//!    (a bad shared secret or encrypted data), then sends a "fixed" file with a different script challenge. Step 4 now
//!    checks all leader data and builds both challenges before the first signature, and keeps a write-ahead progress
//!    file in the session directory: before each legacy signature it records the output, which signature, the nonce and
//!    a hash of the challenge, and after it the signature. A later run reuses recorded signatures, may retry an
//!    identical request (same challenge, which reproduces the signature), and refuses the whole step if any recorded
//!    request's challenge or nonce differs from the current one. Step 4 also refuses to run once its final output file
//!    exists, and the device record refuses a second challenge within one application run. Not covered: anyone with
//!    write access to the session directory can delete or edit the progress file (the file-tamper residual below), and
//!    then only the device record - one application run - stands in the way.
//! 3. **Session file tamper.** The step 2 self file is plain JSON and unauthenticated: an attacker with write access
//!    can point two signatures at one nonce, at an earlier session's nonce, or at another output's script key. Step 4
//!    validates the file's shape before signing (distinct nonce ids, the expected key shapes), which refuses the first
//!    and last. It also compares each signature's public nonce with the public nonce stored in the same file - but
//!    whoever can edit the nonce ids can edit those too, and the public key of any `Random` index can be read from the
//!    device without a prompt, so that comparison only catches an edit that left the stored public nonces alone (and
//!    host/device drift). Within one application run the device's used-nonce record is what blocks a redirect to a
//!    nonce already used; a redirect to a nonce an *earlier* run used is blocked only by the follow-up - an NVM-backed
//!    record, or device-issued handles. A MAC over the self file is a further follow-up.
//! 4. **Pre-upgrade alias.** An application before 6.1.1-pre.0 derived nonce index `j` as `j mod 2^32`; this one
//!    derives an index below 2^32 along that same path. One signature from an aborted pre-upgrade session plus one new
//!    one at `j mod 2^32` gives up the key, and the record knows nothing of the old one. Nonce indexes below 2^32 are
//!    refused ([`check_legacy_nonce_index`]), and the host never draws one.
//! 5. **Old devices.** A device still on 6.1.0 or earlier signs `OneSidedSenderOffset` keys through this instruction
//!    and so remains exposed to the `alpha` extraction above, whatever the host does. The host refuses such a device
//!    (`MIN_LEDGER_APP_VERSION`), but a compromised host would not.
//! 6. **Consent.** `GetScriptSchnorrSignature` and `GetRawSchnorrSignature` still sign `PreMine` keys with no prompt,
//!    over messages the host chooses (with device drawn nonces, so nothing is extractable). That is a separate issue.
//!    The legacy review is therefore an extraction control - it makes a second signature under one nonce visible and,
//!    with the record, refused - not a consent control for pre-mine spending.
//!
//! # The fix, and what gets deleted with it
//!
//! TODO: Teach the pre-mine step 2 / step 3 session file to carry device-issued nonce handles instead of nonce
//! branches and indexes. Handles survive a file fine - what they cannot survive is the device restarting between
//! the two steps, so this also needs the pre-mine flow to reserve its nonces in the same device session that signs
//! with them, or the device store to be made persistent.
//!
//! An NVM-backed used-nonce record is the intermediate step: it would close the restart gap in residuals 1 and 2
//! without changing the session file.
//!
//! When that lands, these get deleted together:
//!
//! - `Instruction::GetRawSchnorrSignatureLegacyNonce` (this crate's `common_types`)
//! - `handler_get_raw_schnorr_signature_legacy_nonce` (the Ledger application)
//! - `ledger_get_raw_schnorr_signature_legacy_nonce` (`minotari_ledger_wallet_comms::accessor_methods`)
//! - `KeyManager::ledger_get_raw_schnorr_signature_legacy_nonce_wrapper` and the `(LedgerKey, LedgerKey)` arm of
//!   `sign_with_nonce_and_challenge`
//! - this module

use crate::{common_types::LedgerKeyBranch, script_offset::PRE_MINE_SENDER_OFFSET_INDEX_BIT};

/// Why a `GetRawSchnorrSignatureLegacyNonce` request was refused.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum LegacyNonceBranchError {
    /// The private key branch is not one the pre-mine spend flow signs with.
    KeyBranchNotAllowed,
    /// The nonce branch is not the one the pre-mine spend flow reserves from.
    NonceBranchNotAllowed,
    /// The nonce index is below 2^32, where it names the same nonce an application before 6.1.1-pre.0 derived for
    /// some index at or above 2^32. See [`check_legacy_nonce_index`].
    NonceIndexAliasesOldApp,
    /// The account is 2^32 or more, where it names the same keys as `account mod 2^32`. See [`check_legacy_account`].
    AccountWraps,
}

/// The account word that actually reaches the derivation path: its low 32 bits.
///
/// The device writes the `u64` account into a hardened path element that `make_bip32_path` parses into a wrapping
/// `u32` - unlike the key index, the account was not split across two elements - so accounts `a` and `a + 2^32` derive
/// the same keys and the same nonces. The used-nonce record keys on this word, not on the `u64` the host sent, so that
/// two requests that sign with one nonce cannot pass as two different accounts.
pub fn legacy_account_word(account: u64) -> u32 {
    u32::try_from(account & u64::from(u32::MAX)).unwrap_or(u32::MAX)
}

/// Refuse a legacy request on an account of 2^32 or more.
///
/// Such an account names the same keys as its low word (see [`legacy_account_word`]), and nothing on the review shows
/// the account, so a host could otherwise sign one nonce under two different-looking accounts. The record keys on the
/// low word anyway; this refuses the ambiguous form outright. A ledger wallet's account is the small number the user
/// chose at setup (the prompt suggests 1-9), so an honest request is never refused.
pub fn check_legacy_account(account: u64) -> Result<(), LegacyNonceBranchError> {
    if account > u64::from(u32::MAX) {
        return Err(LegacyNonceBranchError::AccountWraps);
    }
    Ok(())
}

/// [`check_legacy_nonce_branches`] then [`check_legacy_nonce_index`]: every stateless rule a legacy request has to pass.
/// The host mirrors call this; the device calls the two in the same order.
pub fn check_legacy_nonce_request(
    private_key_branch: LedgerKeyBranch,
    nonce_branch: LedgerKeyBranch,
    nonce_index: u64,
) -> Result<(), LegacyNonceBranchError> {
    check_legacy_nonce_branches(private_key_branch, nonce_branch)?;
    check_legacy_nonce_index(nonce_index)
}

/// The smallest nonce index the legacy instruction will sign with.
pub const MIN_LEGACY_NONCE_INDEX: u64 = 1 << 32;

/// Refuse a legacy nonce index below 2^32.
///
/// Applications before 6.1.1-pre.0 derived every key from `index mod 2^32`, so the `Random` nonce a pre-upgrade
/// session reserved at index `j` (a random `u64`, almost always at or above 2^32) was the key this application derives
/// at `j mod 2^32` - an index below 2^32 keeps its old derivation path. One signature from an aborted pre-upgrade
/// session plus one new legacy signature at `j mod 2^32` over a different challenge would give up the signing key, and
/// this application's used-nonce record cannot know about the old one. Indexes at or above 2^32 derive differently
/// from anything an old application signed with, so they are safe.
///
/// An honest step 2 draws a random `u64` nonce index, which is below 2^32 with probability 2^-32; the host redraws in
/// that case (`get_random_key` on a ledger wallet), so an honest request is never refused. Checked on both sides,
/// before the review.
pub fn check_legacy_nonce_index(nonce_index: u64) -> Result<(), LegacyNonceBranchError> {
    if nonce_index < MIN_LEGACY_NONCE_INDEX {
        return Err(LegacyNonceBranchError::NonceIndexAliasesOldApp);
    }
    Ok(())
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

/// How many legacy signatures the device remembers per application run. See [`LegacyNonceUse`].
///
/// Pre-mine step 4 makes two legacy signatures per output, so this bounds one application run to 32 pre-mine
/// outputs; step 4 saves its progress and asks for an application restart to continue past that. Kept at 64 because
/// the device's RAM budget has not been measured for more; an NVM-backed record is the follow-up.
pub const LEGACY_NONCE_RECORD_SIZE: usize = 64;

/// One legacy signature the device has made (or approved and is about to make): which nonce it used, and what for.
///
/// The device keeps these in a fixed array of [`LEGACY_NONCE_RECORD_SIZE`] slots and checks every new legacy request
/// against it with [`check_legacy_nonce_use`] / [`record_legacy_nonce_use`]. A nonce index may be asked for again only
/// with the same key and the same challenge - an honest retry, which reproduces the same signature and so discloses
/// nothing. Any other second use of a nonce index is refused: that is the one thing that turns two signatures into a
/// private key, including the same-nonce, two-keys variant where the script signature and the metadata signature of
/// one pre-mine output are both asked for under one nonce index and the step 2 script offset closes the system.
///
/// **The record is RAM-backed and is cleared by an application restart.** Persisting it in NVM is a follow-up
/// decision (flash wear, and what a record that outlives the app means for an honest re-run); until then an attacker
/// who wants a second signature under a used nonce index has to get the user to restart the application between two
/// approvals. The record never evicts: when it is full it refuses, until the application is restarted.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct LegacyNonceUse {
    /// The account word that reaches the derivation path - [`legacy_account_word`] of the requested account - so that
    /// accounts 2^32 apart, which sign with the same nonce, share one entry.
    pub account: u32,
    pub nonce_index: u64,
    pub key_branch: u8,
    pub key_index: u64,
    /// A domain separated hash of the 64 byte challenge, so a slot costs 32 bytes rather than 64.
    pub challenge_hash: [u8; 32],
}

/// Why a legacy signature was refused by the used-nonce record.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum LegacyNonceRecordError {
    /// The nonce index was already used on this account, for a different key or a different challenge.
    Reused,
    /// Every slot is taken. The record fails closed rather than forgetting an earlier use.
    Full,
}

/// Check a legacy request against the record without changing it.
///
/// `Ok(true)` means this exact use is already recorded (an honest retry), `Ok(false)` that the nonce index is unused
/// and there is room to record it. The device calls this before its review, so a refused request never reaches the
/// screen, and calls [`record_legacy_nonce_use`] after the user approves.
pub fn check_legacy_nonce_use(
    record: &[Option<LegacyNonceUse>],
    used: &LegacyNonceUse,
) -> Result<bool, LegacyNonceRecordError> {
    for entry in record.iter().flatten() {
        if entry.account == used.account && entry.nonce_index == used.nonce_index {
            if entry == used {
                return Ok(true);
            }
            return Err(LegacyNonceRecordError::Reused);
        }
    }
    if record.iter().any(|slot| slot.is_none()) {
        Ok(false)
    } else {
        Err(LegacyNonceRecordError::Full)
    }
}

/// Record a legacy use, refusing it as [`check_legacy_nonce_use`] would. Recording an already recorded use is a no-op.
///
/// Called after the review and before the signature is computed, so a failure after the screen still leaves the
/// nonce index spent.
pub fn record_legacy_nonce_use(
    record: &mut [Option<LegacyNonceUse>],
    used: LegacyNonceUse,
) -> Result<(), LegacyNonceRecordError> {
    if check_legacy_nonce_use(record, &used)? {
        return Ok(());
    }
    for slot in record.iter_mut() {
        if slot.is_none() {
            *slot = Some(used);
            return Ok(());
        }
    }
    Err(LegacyNonceRecordError::Full)
}

/// The purpose line the device shows when asked for a legacy signature by the `PreMine` key at `key_index`.
///
/// Pre-mine sender offset keys are derived at indexes with
/// [`PRE_MINE_SENDER_OFFSET_INDEX_BIT`](crate::script_offset::PRE_MINE_SENDER_OFFSET_INDEX_BIT) set, and pre-mine
/// script keys at their genesis output index, which never has it. So the index says which of the two signatures this
/// is: the script signature signs with the script key, and the metadata signature with the sender offset key.
///
/// The displayed index is the derived index only because `derive_from_bip32_key` on the device uses all 64 bits of
/// it. When the index wrapped at 2^32, `PreMine 2^63 | x` *was* the script key at `x`, and a host could show two
/// different-looking reviews for one key and one nonce.
pub fn legacy_signature_purpose(key_index: u64) -> &'static str {
    if key_index & PRE_MINE_SENDER_OFFSET_INDEX_BIT == 0 {
        "Pre-mine script signature"
    } else {
        "Pre-mine metadata signature"
    }
}

#[cfg(test)]
mod test {
    use super::*;

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

    /// Script keys sit at genesis output indexes; sender offsets in the top half of `u64`.
    #[test]
    fn the_purpose_follows_the_index() {
        assert_eq!(legacy_signature_purpose(0), "Pre-mine script signature");
        assert_eq!(legacy_signature_purpose(12_345), "Pre-mine script signature");
        assert_eq!(legacy_signature_purpose(u64::MAX >> 1), "Pre-mine script signature");
        assert_eq!(
            legacy_signature_purpose(PRE_MINE_SENDER_OFFSET_INDEX_BIT),
            "Pre-mine metadata signature"
        );
        assert_eq!(legacy_signature_purpose(u64::MAX), "Pre-mine metadata signature");
    }

    fn used(account: u64, nonce_index: u64, key_index: u64, challenge: u8) -> LegacyNonceUse {
        LegacyNonceUse {
            account: legacy_account_word(account),
            nonce_index,
            key_branch: LedgerKeyBranch::PreMine.as_byte(),
            key_index,
            challenge_hash: [challenge; 32],
        }
    }

    #[test]
    fn a_new_nonce_index_is_recorded() {
        let mut record = [None; 4];
        assert_eq!(check_legacy_nonce_use(&record, &used(1, 2, 3, 4)), Ok(false));
        assert_eq!(record_legacy_nonce_use(&mut record, used(1, 2, 3, 4)), Ok(()));
        assert_eq!(record[0], Some(used(1, 2, 3, 4)));
        assert_eq!(check_legacy_nonce_use(&record, &used(1, 2, 3, 4)), Ok(true));
    }

    /// An honest retry - same key, same challenge - reproduces the same signature, so it is allowed and takes no
    /// second slot.
    #[test]
    fn the_same_use_again_is_allowed_without_a_second_slot() {
        let mut record = [None; 2];
        record_legacy_nonce_use(&mut record, used(1, 2, 3, 4)).unwrap();
        assert_eq!(record_legacy_nonce_use(&mut record, used(1, 2, 3, 4)), Ok(()));
        assert_eq!(record[1], None);
    }

    #[test]
    fn a_different_challenge_under_a_used_nonce_is_refused() {
        let mut record = [None; 4];
        record_legacy_nonce_use(&mut record, used(1, 2, 3, 4)).unwrap();
        assert_eq!(
            check_legacy_nonce_use(&record, &used(1, 2, 3, 5)),
            Err(LegacyNonceRecordError::Reused)
        );
        assert_eq!(
            record_legacy_nonce_use(&mut record, used(1, 2, 3, 5)),
            Err(LegacyNonceRecordError::Reused)
        );
    }

    /// The same-nonce, two-keys attack: the script key and the sender offset key under one nonce index.
    #[test]
    fn a_different_key_under_a_used_nonce_is_refused() {
        let mut record = [None; 4];
        record_legacy_nonce_use(&mut record, used(1, 2, 3, 4)).unwrap();
        assert_eq!(
            record_legacy_nonce_use(&mut record, used(1, 2, PRE_MINE_SENDER_OFFSET_INDEX_BIT | 3, 4)),
            Err(LegacyNonceRecordError::Reused)
        );
        let mut other_branch = used(1, 2, 3, 4);
        other_branch.key_branch = LedgerKeyBranch::Random.as_byte();
        assert_eq!(
            record_legacy_nonce_use(&mut record, other_branch),
            Err(LegacyNonceRecordError::Reused)
        );
    }

    /// A full record refuses rather than evicting, and still allows a retry of something it holds.
    #[test]
    fn a_full_record_refuses_and_never_evicts() {
        let mut record = [None; 2];
        record_legacy_nonce_use(&mut record, used(1, 1, 3, 4)).unwrap();
        record_legacy_nonce_use(&mut record, used(1, 2, 3, 4)).unwrap();
        assert_eq!(
            record_legacy_nonce_use(&mut record, used(1, 3, 3, 4)),
            Err(LegacyNonceRecordError::Full)
        );
        assert_eq!(record_legacy_nonce_use(&mut record, used(1, 1, 3, 4)), Ok(()));
        assert_eq!(
            record_legacy_nonce_use(&mut record, used(1, 1, 3, 5)),
            Err(LegacyNonceRecordError::Reused)
        );
    }

    /// Accounts with different low words derive different nonces from one index, so they are tracked independently.
    #[test]
    fn accounts_with_different_low_words_are_independent() {
        let mut record = [None; 4];
        record_legacy_nonce_use(&mut record, used(1, 2, 3, 4)).unwrap();
        assert_eq!(record_legacy_nonce_use(&mut record, used(9, 2, 7, 8)), Ok(()));
    }

    /// Accounts `a` and `a + 2^32` derive the same nonce, so they are one entry: the second, different use is refused.
    #[test]
    fn accounts_two_to_the_thirty_two_apart_are_one_entry() {
        let mut record = [None; 4];
        record_legacy_nonce_use(&mut record, used(1, 2, 3, 4)).unwrap();
        assert_eq!(
            record_legacy_nonce_use(
                &mut record,
                used(1 + (1 << 32), 2, PRE_MINE_SENDER_OFFSET_INDEX_BIT | 3, 5)
            ),
            Err(LegacyNonceRecordError::Reused)
        );
        // The identical use under the aliased account is the same entry, not a second one.
        assert_eq!(
            record_legacy_nonce_use(&mut record, used(1 + (1 << 32), 2, 3, 4)),
            Ok(())
        );
        assert_eq!(record[1], None);
    }

    #[test]
    fn an_account_of_two_to_the_thirty_two_or_more_is_refused() {
        for account in [0, 1, u64::from(u32::MAX)] {
            assert_eq!(check_legacy_account(account), Ok(()), "{account}");
        }
        for account in [1 << 32, (1 << 32) + 1, u64::MAX] {
            assert_eq!(
                check_legacy_account(account),
                Err(LegacyNonceBranchError::AccountWraps),
                "{account}"
            );
        }
        assert_eq!(legacy_account_word((1 << 32) + 7), 7);
    }

    #[test]
    fn a_nonce_index_below_two_to_the_thirty_two_is_refused() {
        for index in [0, 1, MIN_LEGACY_NONCE_INDEX - 1] {
            assert_eq!(
                check_legacy_nonce_index(index),
                Err(LegacyNonceBranchError::NonceIndexAliasesOldApp),
                "{index}"
            );
        }
        for index in [MIN_LEGACY_NONCE_INDEX, u64::MAX] {
            assert_eq!(check_legacy_nonce_index(index), Ok(()), "{index}");
        }
    }
}
