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
//! [`LedgerKeyBranch::OneSidedSenderOffset`] is on the whitelist below, so this reaches sender offset private keys.
//! That matters more than it looks: a pre-mine output's script offset is a single script key minus a single sender
//! offset key, so a host that recovers the sender offset private key can subtract it back out of the script offset
//! it already holds and recover that output's *script* private key too. For pre-mine outputs specifically, this
//! re-opens what device-generated sender offset keys closed.
//!
//! # Scope
//!
//! This applies only to keys signed through the legacy instruction, which is only the pre-mine spend flow. Normal
//! spend flows are unaffected: they sign through `GetRawSchnorrSignature` with a device-issued handle that the host
//! can neither choose nor redeem twice, so there is no second signature to difference against.
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

use crate::common_types::LedgerKeyBranch;

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
/// over (see the module docs), so the exposure is held to the branches pre-mine genuinely uses and no others.
/// `Spend` is excluded here as well as by the device's branch mapping - `alpha` is never signable by index.
///
/// This lives in the shared crate, and is checked on both sides, so the two whitelists cannot drift apart. The
/// device's check is the one that counts; the host mirrors it so a caller gets a legible error instead of a status
/// word, and so a request the device would refuse never reaches the wire.
pub fn check_legacy_nonce_branches(
    private_key_branch: LedgerKeyBranch,
    nonce_branch: LedgerKeyBranch,
) -> Result<(), LegacyNonceBranchError> {
    match private_key_branch {
        // `PreMine` signs the script signature, `OneSidedSenderOffset` signs the metadata signature, and `Random`
        // is what pre-mine sender offset keys were on before they moved onto the device.
        LedgerKeyBranch::PreMine | LedgerKeyBranch::OneSidedSenderOffset | LedgerKeyBranch::Random => {},
        LedgerKeyBranch::Spend => return Err(LegacyNonceBranchError::KeyBranchNotAllowed),
    }
    // Pinned so the instruction cannot be turned into a way to extract a key on some other branch by pointing its
    // nonce there. Pre-mine reserves both of its nonces from `Random`.
    if nonce_branch != LedgerKeyBranch::Random {
        return Err(LegacyNonceBranchError::NonceBranchNotAllowed);
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    /// The exact set the pre-mine spend flow signs with, as of sender offset keys being device generated:
    /// `PreMine` for the script signature and `OneSidedSenderOffset` for the metadata signature.
    #[test]
    fn the_branches_pre_mine_signs_with_are_allowed() {
        for branch in [
            LedgerKeyBranch::PreMine,
            LedgerKeyBranch::OneSidedSenderOffset,
            LedgerKeyBranch::Random,
        ] {
            assert_eq!(check_legacy_nonce_branches(branch, LedgerKeyBranch::Random), Ok(()));
        }
    }

    /// `alpha` is never signable by index, on this instruction least of all.
    #[test]
    fn the_spend_branch_is_refused() {
        assert_eq!(
            check_legacy_nonce_branches(LedgerKeyBranch::Spend, LedgerKeyBranch::Random),
            Err(LegacyNonceBranchError::KeyBranchNotAllowed)
        );
    }

    /// Every branch other than `Random` is refused as a nonce, including the ones that are fine as a key.
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
}
