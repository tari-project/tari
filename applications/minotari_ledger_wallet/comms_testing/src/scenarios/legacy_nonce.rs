// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! The branch whitelist on `GetRawSchnorrSignatureLegacyNonce`, proved **on the device**.
//!
//! Read [`minotari_ledger_wallet_common::legacy_nonce`] before this file. Its module docs are the canonical
//! description of the exposure and are not restated here; what follows is only why it is worth a device test.
//!
//! # This instruction is live, not dead code
//!
//! `Instruction::GetRawSchnorrSignatureLegacyNonce` (`0x14`) is on the pre-mine spend path today - three call sites
//! in `applications/minotari_console_wallet/src/automation/commands.rs`, each with a comment pointing at the shared
//! module. It is the instruction `GetRawSchnorrSignature` replaced, and it carries the flaw that replacement fixed:
//! the host names the nonce by branch and index, and the device re-derives the same scalar every time it is asked.
//! Two signatures over different challenges with the same `(key index, nonce index)` therefore give
//! `k = (s1 - s2) / (e1 - e2)` and then `x = (s1 - k) / e1`.
//!
//! Because `OneSidedSenderOffset` is on the whitelist, that reaches **sender offset private keys**; and because a
//! pre-mine output's script offset is a single script key minus a single sender offset key, a host that recovers
//! the sender offset key can subtract it back out of an offset it already holds and recover the *script* private
//! key too.
//!
//! # The whitelist is the entire containment, and only the device's copy counts
//!
//! `check_legacy_nonce_branches` lives in the shared crate and is called on both sides. Its own documentation says
//! which of the two matters: **"The device's check is the one that counts."** The host's copy exists so a caller
//! gets a legible error instead of a status word.
//!
//! The host side unit tests for that function already exist and pass. They test the mirror. Nothing automated has
//! ever exercised the copy that is actually load bearing - a device application that dropped the call entirely
//! would pass every test in this repository. **That is the gap this module closes.**
//!
//! Which is why every request here goes out over [`crate::raw`] rather than through
//! `ledger_get_raw_schnorr_signature_legacy_nonce`. That accessor refuses a disallowed pair *before it opens the
//! transport*, so a scenario driven through it would only ever re-test the mirror, from a second angle, and would
//! report green against a device with no check in it at all.
//!
//! # Testing this does not entrench it
//!
//! [`minotari_ledger_wallet_common::legacy_nonce`] carries a TODO to delete this instruction - along with the
//! handler, the accessor and the key manager wrapper - once the pre-mine step 2 / step 3 session file carries
//! device-issued nonce handles instead of branches and indexes. These scenarios make that deletion *safer*, not
//! harder: they are the thing that says out loud what the current behaviour is, so whoever removes it can see
//! exactly which guarantees go with it. When the instruction goes, this module goes in the same commit.

use minotari_ledger_wallet_common::{
    common_types::{AppSW, Instruction, LedgerKeyBranch},
    legacy_nonce::check_legacy_nonce_branches,
};
use minotari_ledger_wallet_comms::accessor_methods::{
    ledger_get_public_key,
    ledger_get_raw_schnorr_signature_legacy_nonce,
};

use crate::{
    fixtures,
    raw::{self, SchnorrReply, payload},
    scenarios::{
        Approval,
        Scenario,
        ScenarioContext,
        ScenarioModule,
        ScenarioResult,
        WithContext,
        expect_ok,
        expect_status,
        fail,
        require,
    },
};

pub const MODULE: ScenarioModule = ScenarioModule {
    name: "legacy_nonce",
    scenarios: SCENARIOS,
};

const SCENARIOS: &[Scenario] = &[
    Scenario {
        name: "the device accepts exactly the branch pairs the pre-mine spend flow uses",
        covers: &[
            Instruction::GetRawSchnorrSignatureLegacyNonce,
            Instruction::GetPublicKey,
        ],
        approval: Approval::NotNeeded,
        run: allowed_pairs_are_accepted,
    },
    Scenario {
        name: "the device refuses every disallowed legacy nonce branch pair with BadBranchKey",
        covers: &[Instruction::GetRawSchnorrSignatureLegacyNonce],
        approval: Approval::NotNeeded,
        run: disallowed_pairs_are_refused,
    },
    Scenario {
        name: "the legacy nonce really is deterministic, which is why the whitelist exists",
        covers: &[Instruction::GetRawSchnorrSignatureLegacyNonce],
        approval: Approval::NotNeeded,
        run: the_nonce_is_deterministic,
    },
];

/// Every branch the shared enum names. The whitelist is stated over all four, so the scenarios enumerate all four.
const ALL_BRANCHES: [LedgerKeyBranch; 4] = [
    LedgerKeyBranch::OneSidedSenderOffset,
    LedgerKeyBranch::Spend,
    LedgerKeyBranch::Random,
    LedgerKeyBranch::PreMine,
];

/// Ask the device for a legacy signature, over raw APDUs so that the host's mirror of the whitelist is bypassed.
///
/// The branches go as raw bytes rather than as [`LedgerKeyBranch`] values, so that a scenario can also send a
/// branch identifier the shared enum has no name for.
fn legacy_signature(
    account: u64,
    key_index: u64,
    key_branch: u8,
    nonce_index: u64,
    nonce_branch: u8,
    challenge: &[u8; 64],
) -> Result<crate::raw::RawReply, crate::scenarios::ScenarioError> {
    raw::command(
        account,
        Instruction::GetRawSchnorrSignatureLegacyNonce,
        payload::raw_schnorr_signature_legacy_nonce(key_index, key_branch, nonce_index, nonce_branch, challenge),
    )
    .send()
    .context(|| {
        format!("GetRawSchnorrSignatureLegacyNonce with key branch {key_branch:#04x}, nonce branch {nonce_branch:#04x}")
    })
}

/// Acceptance: the three key branches the pre-mine spend flow signs with, against a `Random` nonce, are accepted -
/// and the signature they produce verifies.
///
/// The accepting half is as necessary as the refusing half. A device that refused *everything* would satisfy every
/// other scenario in this module while breaking the pre-mine spend flow outright, and nothing else in this
/// repository exercises that flow end to end.
///
/// `PreMine` signs the script signature, `OneSidedSenderOffset` signs the metadata signature, and `Random` is where
/// pre-mine sender offset keys lived before they moved on to the device.
fn allowed_pairs_are_accepted(_context: &ScenarioContext<'_>) -> ScenarioResult {
    let account = fixtures::random_u64();

    for key_branch in [
        LedgerKeyBranch::PreMine,
        LedgerKeyBranch::OneSidedSenderOffset,
        LedgerKeyBranch::Random,
    ] {
        // The shared whitelist has to agree that this pair is allowed, or the scenario is asserting the wrong
        // thing: a pair the host also refuses would make a device rejection look like success.
        require(
            check_legacy_nonce_branches(key_branch, LedgerKeyBranch::Random).is_ok(),
            || {
                format!(
                    "the shared whitelist no longer allows {key_branch}, so this scenario is testing the wrong pair"
                )
            },
        )?;

        let key_index = fixtures::random_u64();
        let challenge = fixtures::random_challenge();

        // Through the accessor, not over raw APDUs. This is the pre-mine spend flow's own call, and it is the only
        // place in the repository that exercises it: the accessor lays out the 104 byte payload, applies the host
        // mirror of the whitelist, and parses the reply into a `CompressedSignature`. The refusal scenarios below
        // have to bypass it - that is the whole point of them - but the accepting path must not, or a regression in
        // shipped code would be invisible while this suite stayed green.
        let signature = ledger_get_raw_schnorr_signature_legacy_nonce(
            account,
            key_index,
            key_branch,
            fixtures::random_u64(),
            LedgerKeyBranch::Random,
            &challenge,
        )
        .context(|| format!("GetRawSchnorrSignatureLegacyNonce on the {key_branch} branch"))?;

        let public_key = ledger_get_public_key(account, key_index, key_branch)
            .context(|| format!("GetPublicKey for the {key_branch} key"))?;
        let signature = signature
            .to_schnorr_signature()
            .context(|| "the device's compressed signature would not decompress".to_string())?;

        require(signature.verify_raw_uniform(&public_key, &challenge), || {
            format!("the legacy signature on the {key_branch} branch does not verify against that branch's key")
        })?;
    }
    Ok(())
}

/// Acceptance: **every** disallowed pair is refused by the device, with `BadBranchKey`.
///
/// The whole cross product of the four named branches is enumerated, and each pair is classified by the shared
/// `check_legacy_nonce_branches` rather than by a second list written here. That is deliberate: a hand written
/// table of "these should fail" drifts the moment the whitelist changes, and drifts *silently* in the direction of
/// testing less. Asking the shared function means the scenario always tests exactly the whitelist that is in
/// force - and a change that widened the whitelist would show up as a device scenario that started sending a pair
/// it used to refuse.
///
/// # Every refusal is the same status word, so *which rule* refused is not host-observable
///
/// `handler_get_raw_schnorr_signature_legacy_nonce` does
/// `check_legacy_nonce_branches(..).map_err(|_| AppSW::BadBranchKey)`, and `KeyType::from_branch_key` independently
/// answers `BadBranchKey` for the spend branch. Every refusal path for this instruction therefore yields
/// `BadBranchKey`, and no scenario can tell the whitelist's refusal apart from the branch mapping's, or a key
/// branch refusal from a nonce branch one. Nothing here claims otherwise.
///
/// The status word is still asserted *exactly* rather than as "some failure", because a refusal for a different
/// reason - a length check, say - would mean the whitelist was never reached at all.
///
/// # Which pairs only the whitelist can be refusing
///
/// That distinction cannot be observed pair by pair, but it can be observed in aggregate, and it is the half that
/// matters. Split the 13 disallowed pairs by what would happen if `check_legacy_nonce_branches` were deleted from
/// the handler tomorrow:
///
/// * the 7 pairs with `Spend` on either side would still be refused, by `KeyType::from_branch_key`, which has no
///   mapping for that branch;
/// * the remaining **6** - key branch in `{PreMine, OneSidedSenderOffset, Random}` against a nonce branch of `PreMine`
///   or `OneSidedSenderOffset` - would be **accepted and signed**. `from_branch_key` maps all of those happily, so the
///   whitelist is the only thing standing between a host and a deterministic-nonce signature over a pre-mine or sender
///   offset key, which is the disclosure this module's docs describe.
///
/// Those 6 are counted separately below. They are the load bearing ones; a change that left the other 7 passing and
/// silently dropped these would be the change this scenario exists to catch.
///
/// A branch byte the shared enum does not name is checked too. It is refused earlier, by `branch_key_from_u64`, but
/// it is refused with the same status word and it is the shape an attacker would try first.
fn disallowed_pairs_are_refused(_context: &ScenarioContext<'_>) -> ScenarioResult {
    let account = fixtures::random_u64();
    let mut checked = 0usize;
    let mut whitelist_only = 0usize;

    for key_branch in ALL_BRANCHES {
        for nonce_branch in ALL_BRANCHES {
            if check_legacy_nonce_branches(key_branch, nonce_branch).is_ok() {
                continue;
            }
            let reply = legacy_signature(
                account,
                fixtures::random_u64(),
                key_branch.as_byte(),
                fixtures::random_u64(),
                nonce_branch.as_byte(),
                &fixtures::random_challenge(),
            )?;
            expect_status(
                &format!("a legacy signature with key branch {key_branch} and nonce branch {nonce_branch}"),
                &reply,
                AppSW::BadBranchKey,
            )?;
            checked = checked.saturating_add(1);
            // A pair the device's *other* check would wave through. See the doc comment: these are the ones the
            // whitelist alone refuses, and the only ones whose refusal says anything about the whitelist.
            if key_branch != LedgerKeyBranch::Spend && nonce_branch != LedgerKeyBranch::Spend {
                whitelist_only = whitelist_only.saturating_add(1);
            }
        }
    }

    // 16 pairs, of which 3 are allowed, so 13 must have been refused. Pinned because every assertion above is
    // inside a `continue`-guarded loop, and a whitelist that accidentally allowed everything would make this
    // scenario pass having sent nothing at all.
    require(checked == 13, || {
        format!(
            "expected 13 disallowed branch pairs out of {}, checked {checked}. The whitelist in \
             `minotari_ledger_wallet_common::legacy_nonce` has changed shape; make sure the new shape is the one you \
             meant before updating this number.",
            ALL_BRANCHES.len() * ALL_BRANCHES.len()
        )
    })?;

    // And of those 13, six must have been pairs that only the whitelist refuses. Without this the scenario could
    // stay green on a device that had lost `check_legacy_nonce_branches` entirely, because the seven pairs
    // involving `Spend` would still be refused by the branch mapping.
    require(whitelist_only == 6, || {
        format!(
            "expected 6 of the disallowed pairs to be ones only `check_legacy_nonce_branches` refuses, found \
             {whitelist_only}. Those are the pairs `KeyType::from_branch_key` would otherwise sign; if that number \
             has fallen, the whitelist is covering less than it did."
        )
    })?;

    // A branch identifier that is not one of the four at all. Refused by `branch_key_from_u64` before the whitelist
    // is reached, with the same status word - so a host cannot get past the whitelist by naming a fifth branch.
    let unnamed = 0x42u8;
    require(LedgerKeyBranch::from_byte(unnamed).is_none(), || {
        format!("{unnamed:#04x} is now a named branch; pick a byte that is not")
    })?;
    let reply = legacy_signature(
        account,
        fixtures::random_u64(),
        unnamed,
        fixtures::random_u64(),
        LedgerKeyBranch::Random.as_byte(),
        &fixtures::random_challenge(),
    )?;
    expect_status(
        "a legacy signature with a branch identifier the shared enum does not name",
        &reply,
        AppSW::BadBranchKey,
    )
}

/// Acceptance: the nonce really is re-derived rather than drawn, which is the property the whitelist contains.
///
/// Two signatures over *different* challenges with the same `(key index, nonce index)` come back with the **same**
/// public nonce. That is not a bug being reported - it is the documented behaviour of this instruction, and it is
/// the reason the whitelist exists at all. Asserting it here means the containment and the thing being contained
/// are in the same file: if the device ever starts drawing a fresh nonce, this scenario fails, and whoever sees it
/// fail is looking at the module that explains why that would be a *good* change and what else should be deleted
/// alongside it.
///
/// Deliberately **not** carried further. The next two lines of arithmetic recover the private key, and this suite
/// has no business writing a key recovery it does not need: the equality above already proves the nonce is reused,
/// and a recovered key would be a secret in a failure message in a file CI uploads.
fn the_nonce_is_deterministic(_context: &ScenarioContext<'_>) -> ScenarioResult {
    let account = fixtures::random_u64();
    let key_index = fixtures::random_u64();
    let nonce_index = fixtures::random_u64();

    let first = legacy_signature(
        account,
        key_index,
        LedgerKeyBranch::PreMine.as_byte(),
        nonce_index,
        LedgerKeyBranch::Random.as_byte(),
        &fixtures::random_challenge(),
    )?;
    expect_ok("the first legacy signature", &first)?;
    let second = legacy_signature(
        account,
        key_index,
        LedgerKeyBranch::PreMine.as_byte(),
        nonce_index,
        LedgerKeyBranch::Random.as_byte(),
        &fixtures::random_challenge(),
    )?;
    expect_ok("the second legacy signature", &second)?;

    let first = SchnorrReply::parse(&first.data).map_err(fail)?;
    let second = SchnorrReply::parse(&second.data).map_err(fail)?;

    require(first.public_nonce == second.public_nonce, || {
        "the legacy instruction returned a different public nonce for the same nonce index. That is a *safer* device \
         than the one documented, but it means this instruction is no longer what \
         `minotari_ledger_wallet_common::legacy_nonce` describes - read that module, and check whether the whitelist \
         and this whole instruction can now be deleted rather than just updating this assertion."
            .to_string()
    })?;
    require(first.signature != second.signature, || {
        "two legacy signatures over different challenges produced the same `s`, which cannot happen for a correct \
         signature over a reused nonce"
            .to_string()
    })
}
