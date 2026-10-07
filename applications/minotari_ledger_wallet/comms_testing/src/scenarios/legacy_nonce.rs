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
//! The whitelist used to admit `OneSidedSenderOffset` keys, which together with the base index `GetScriptOffset`
//! returns reached `alpha`. It now admits `PreMine` keys only - pre-mine script keys and, since `GetScriptOffset`
//! puts pre-mine sender offsets on `PreMine`, pre-mine sender offset keys - and the device asks the user to approve
//! every signature it makes. So every *accepting* scenario here answers a review, and every *refusal* must be
//! refused before one is drawn.
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
//! Which is why every **rejection** here goes out over [`crate::raw`] rather than through
//! `ledger_get_raw_schnorr_signature_legacy_nonce`. That accessor refuses a disallowed pair *before it opens the
//! transport*, so a scenario driven through it would only ever re-test the mirror, from a second angle, and would
//! report green against a device with no check in it at all.
//!
//! The **accepting** scenarios do the opposite and drive the accessor, which is the rule stated in
//! [`crate::raw`] with no exception in this module: the pair is on the whitelist, so the mirror passes it through,
//! and the accessor is the shipped code the pre-mine spend flow calls. It is also the only caller of that accessor
//! anywhere in the repository.
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
use tari_common_types::types::CompressedSignature;

use crate::{
    approver::{Outcome, while_reviewing},
    fixtures,
    raw::{self, payload},
    review::ExpectedReview,
    scenarios::{
        Approval,
        Scenario,
        ScenarioContext,
        ScenarioModule,
        ScenarioResult,
        WithContext,
        expect_status,
        require,
    },
};

pub const MODULE: ScenarioModule = ScenarioModule {
    name: "legacy_nonce",
    scenarios: SCENARIOS,
};

const SCENARIOS: &[Scenario] = &[
    Scenario {
        name: "the device signs a PreMine key against a Random nonce once the user approves the review",
        covers: &[
            Instruction::GetRawSchnorrSignatureLegacyNonce,
            Instruction::GetPublicKey,
        ],
        approval: Approval::Required,
        run: the_allowed_pair_is_reviewed_and_signed,
    },
    Scenario {
        name: "a legacy request for a OneSidedSenderOffset key is refused with BadBranchKey, before any review",
        covers: &[Instruction::GetRawSchnorrSignatureLegacyNonce],
        approval: Approval::NotNeeded,
        run: a_sender_offset_key_is_refused,
    },
    Scenario {
        name: "the device refuses every disallowed legacy nonce branch pair with BadBranchKey",
        covers: &[Instruction::GetRawSchnorrSignatureLegacyNonce],
        approval: Approval::NotNeeded,
        run: disallowed_pairs_are_refused,
    },
    Scenario {
        name: "a legacy retry over the same challenge is identical, and a different challenge is refused as reused",
        covers: &[Instruction::GetRawSchnorrSignatureLegacyNonce],
        approval: Approval::Required,
        run: the_nonce_is_deterministic,
    },
    Scenario {
        name: "PreMine x and PreMine 2^63 | x are different keys, and one nonce index will not sign for both",
        covers: &[
            Instruction::GetRawSchnorrSignatureLegacyNonce,
            Instruction::GetPublicKey,
        ],
        approval: Approval::Required,
        run: the_reviewed_index_is_the_signing_index,
    },
    Scenario {
        name: "a legacy nonce index below 2^32, or an account of 2^32 or more, is refused with BadBranchKey",
        covers: &[Instruction::GetRawSchnorrSignatureLegacyNonce],
        approval: Approval::NotNeeded,
        run: a_nonce_index_below_two_to_the_thirty_two_is_refused,
    },
];

/// Every branch the shared enum names. The whitelist is stated over all four, so the scenarios enumerate all four.
const ALL_BRANCHES: [LedgerKeyBranch; 4] = [
    LedgerKeyBranch::OneSidedSenderOffset,
    LedgerKeyBranch::Spend,
    LedgerKeyBranch::Random,
    LedgerKeyBranch::PreMine,
];

/// The top bit of a pre-mine sender offset key index, restated rather than imported so that a device which put the
/// metadata signature's purpose on the wrong side of it would be caught. See `ExpectedReview::legacy_signature`.
const SENDER_OFFSET_INDEX_BIT: u64 = 1 << 63;

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

/// A random legacy nonce index at or above 2^32, the range the device signs with (see
/// `minotari_ledger_wallet_common::legacy_nonce::check_legacy_nonce_index`).
fn legacy_nonce_index() -> u64 {
    fixtures::random_u64() | (1 << 32)
}

/// Ask for a `PreMine` legacy signature through the shipped accessor, approving the review the device puts up -
/// after checking it names the purpose, the key and the nonce that were asked for.
fn reviewed_pre_mine_signature(
    context: &ScenarioContext<'_>,
    account: u64,
    key_index: u64,
    nonce_index: u64,
    challenge: &[u8; 64],
) -> Result<CompressedSignature, crate::scenarios::ScenarioError> {
    let expected = ExpectedReview::legacy_signature(
        &LedgerKeyBranch::PreMine.to_string(),
        key_index,
        &LedgerKeyBranch::Random.to_string(),
        nonce_index,
    );
    let (signature, review) = while_reviewing(context.approver(), &expected, Outcome::Approve, || {
        ledger_get_raw_schnorr_signature_legacy_nonce(
            account,
            key_index,
            LedgerKeyBranch::PreMine,
            nonce_index,
            LedgerKeyBranch::Random,
            challenge,
        )
    });
    review.context(|| "the device's legacy signature review".to_string())?;
    signature.context(|| "GetRawSchnorrSignatureLegacyNonce on the PreMine branch".to_string())
}

/// Acceptance: the one pair the pre-mine spend flow signs with - a `PreMine` key against a `Random` nonce - is shown
/// to the user, and once approved it is signed, and the signature verifies.
///
/// The accepting half is as necessary as the refusing half. A device that refused *everything* would satisfy every
/// other scenario in this module while breaking the pre-mine spend flow outright, and nothing else in this
/// repository exercises that flow end to end.
///
/// The key index is a small one, the shape of a pre-mine *script* key (its genesis output index), so the review must
/// say "Pre-mine script signature". [`the_nonce_is_deterministic`] covers the sender offset shape.
fn the_allowed_pair_is_reviewed_and_signed(context: &ScenarioContext<'_>) -> ScenarioResult {
    let account = fixtures::random_u64();

    // The shared whitelist has to agree that this pair is allowed, or the scenario is asserting the wrong thing: a
    // pair the host also refuses would make a device rejection look like success.
    require(
        check_legacy_nonce_branches(LedgerKeyBranch::PreMine, LedgerKeyBranch::Random).is_ok(),
        || "the shared whitelist no longer allows PreMine, so this scenario is testing the wrong pair".to_string(),
    )?;

    let key_index = fixtures::random_u64() % 100_000;
    let challenge = fixtures::random_challenge();

    // Through the accessor, not over raw APDUs. This is the pre-mine spend flow's own call: the accessor lays out
    // the 104 byte payload, applies the host mirror of the whitelist, and parses the reply. The refusal scenarios
    // have to bypass it - that is the whole point of them - but the accepting path must not, or a regression in
    // shipped code would be invisible while this suite stayed green.
    let signature = reviewed_pre_mine_signature(context, account, key_index, legacy_nonce_index(), &challenge)?;

    let public_key = ledger_get_public_key(account, key_index, LedgerKeyBranch::PreMine)
        .context(|| "GetPublicKey for the PreMine key".to_string())?;
    let signature = signature
        .to_schnorr_signature()
        .context(|| "the device's compressed signature would not decompress".to_string())?;

    require(signature.verify_raw_uniform(&public_key, &challenge), || {
        "the legacy signature on the PreMine branch does not verify against that branch's key".to_string()
    })
}

/// Acceptance: a legacy request for a `OneSidedSenderOffset` private key is refused with `BadBranchKey`, and is
/// refused before any review is drawn.
///
/// This is the pair that reached `alpha`: `GetScriptOffset` returns the index of the `OneSidedSenderOffset` key that
/// blinds a reply containing `alpha`, and two legacy signatures under it gave the key up. It is in the enumeration
/// below as well; it is spelled out here because it is the one that matters most. Sent over raw APDUs, so the
/// host's mirror is bypassed and the device's own check is what answers. Had the device drawn a review instead, this
/// would block rather than return, and the scenario would fail on its timeout.
fn a_sender_offset_key_is_refused(_context: &ScenarioContext<'_>) -> ScenarioResult {
    let reply = legacy_signature(
        fixtures::random_u64(),
        fixtures::random_u64(),
        LedgerKeyBranch::OneSidedSenderOffset.as_byte(),
        fixtures::random_u64(),
        LedgerKeyBranch::Random.as_byte(),
        &fixtures::random_challenge(),
    )?;
    expect_status(
        "a legacy signature by a OneSidedSenderOffset key against a Random nonce",
        &reply,
        AppSW::BadBranchKey,
    )
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
/// matters. Split the 15 disallowed pairs by what would happen if `check_legacy_nonce_branches` were deleted from
/// the handler tomorrow:
///
/// * the 7 pairs with `Spend` on either side would still be refused, by `KeyType::from_branch_key`, which has no
///   mapping for that branch;
/// * the remaining **8** - every pair of `{PreMine, OneSidedSenderOffset, Random}` but `(PreMine, Random)` - would be
///   **accepted and signed**. `from_branch_key` maps all of those happily, so the whitelist is the only thing standing
///   between a host and a deterministic-nonce signature over a sender offset key, which is the disclosure this module's
///   docs describe - `(OneSidedSenderOffset, Random)` is the pair that reached `alpha`.
///
/// Those 8 are counted separately below. They are the load bearing ones; a change that left the other 7 passing and
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

    // 16 pairs, of which 1 is allowed, so 15 must have been refused. Pinned because every assertion above is
    // inside a `continue`-guarded loop, and a whitelist that accidentally allowed everything would make this
    // scenario pass having sent nothing at all.
    require(checked == 15, || {
        format!(
            "expected 15 disallowed branch pairs out of {}, checked {checked}. The whitelist in \
             `minotari_ledger_wallet_common::legacy_nonce` has changed shape; make sure the new shape is the one you \
             meant before updating this number.",
            ALL_BRANCHES.len() * ALL_BRANCHES.len()
        )
    })?;

    // And of those 15, eight must have been pairs that only the whitelist refuses. Without this the scenario could
    // stay green on a device that had lost `check_legacy_nonce_branches` entirely, because the seven pairs
    // involving `Spend` would still be refused by the branch mapping.
    require(whitelist_only == 8, || {
        format!(
            "expected 8 of the disallowed pairs to be ones only `check_legacy_nonce_branches` refuses, found \
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

/// Acceptance: the nonce really is re-derived rather than drawn, which is why the device has to remember which
/// nonce indexes it has used - and it does.
///
/// Two approved signatures over the **same** challenge with the same `(key index, nonce index)` are identical - an
/// honest retry, which the used-nonce record allows because it discloses nothing new. A third request under that
/// nonce index over a **different** challenge is refused with `LegacyNonceReused`, before any review: two signatures
/// under one nonce over different challenges give up the key, and the record exists to stop exactly that. The refusal
/// goes over raw APDUs so the device's own check is what answers.
fn the_nonce_is_deterministic(context: &ScenarioContext<'_>) -> ScenarioResult {
    let account = fixtures::random_u64();
    // A key index with the top bit set: the shape of a pre-mine *sender offset* key, so both reviews must say
    // "Pre-mine metadata signature".
    let key_index = fixtures::random_u64() | SENDER_OFFSET_INDEX_BIT;
    let nonce_index = legacy_nonce_index();
    let challenge = fixtures::random_challenge();

    let first = reviewed_pre_mine_signature(context, account, key_index, nonce_index, &challenge)?;
    let second = reviewed_pre_mine_signature(context, account, key_index, nonce_index, &challenge)?;
    require(first == second, || {
        "the same legacy request twice produced two different signatures: either the nonce is no longer re-derived \
         (read `minotari_ledger_wallet_common::legacy_nonce` - the instruction may be deletable) or the retry was not \
         served from the same nonce"
            .to_string()
    })?;

    let reply = legacy_signature(
        account,
        key_index,
        LedgerKeyBranch::PreMine.as_byte(),
        nonce_index,
        LedgerKeyBranch::Random.as_byte(),
        &fixtures::random_challenge(),
    )?;
    expect_status(
        "a legacy signature under a used nonce index over a different challenge",
        &reply,
        AppSW::LegacyNonceReused,
    )
}

/// Acceptance: the index on the review is the index the device signs with - `PreMine x` and `PreMine 2^63 | x` are
/// different keys, so their signatures verify against different public keys - and once a nonce index has signed for
/// one of them, it will not sign for the other.
///
/// When the device reduced the index modulo 2^32 the two were one key. And with the step 2 script offset, signatures
/// by the script key and the sender offset key under *one* nonce index are three equations in three unknowns: the
/// same-nonce, two-keys extraction behind two honest-looking reviews. So each key signs under its own nonce index
/// here, and the cross request - the sender offset key under the script key's nonce index - is refused with
/// `LegacyNonceReused`, before any review.
fn the_reviewed_index_is_the_signing_index(context: &ScenarioContext<'_>) -> ScenarioResult {
    let account = fixtures::random_u64();
    let script_index = fixtures::random_u64() % 100_000;
    let sender_offset_index = SENDER_OFFSET_INDEX_BIT | script_index;

    let script_key = ledger_get_public_key(account, script_index, LedgerKeyBranch::PreMine)
        .context(|| format!("GetPublicKey for PreMine {script_index}"))?;
    let sender_offset_key = ledger_get_public_key(account, sender_offset_index, LedgerKeyBranch::PreMine)
        .context(|| format!("GetPublicKey for PreMine {sender_offset_index}"))?;
    require(script_key != sender_offset_key, || {
        format!("PreMine {script_index} and PreMine {sender_offset_index} are the same key: the index still wraps")
    })?;

    let script_nonce_index = legacy_nonce_index();
    for (index, nonce_index, own_key, other_key) in [
        (script_index, script_nonce_index, &script_key, &sender_offset_key),
        (
            sender_offset_index,
            legacy_nonce_index(),
            &sender_offset_key,
            &script_key,
        ),
    ] {
        let challenge = fixtures::random_challenge();
        let signature = reviewed_pre_mine_signature(context, account, index, nonce_index, &challenge)?
            .to_schnorr_signature()
            .context(|| "the device's compressed signature would not decompress".to_string())?;
        require(signature.verify_raw_uniform(own_key, &challenge), || {
            format!("the legacy signature by PreMine {index} does not verify against that key")
        })?;
        require(!signature.verify_raw_uniform(other_key, &challenge), || {
            format!("the legacy signature by PreMine {index} also verifies against the other index's key")
        })?;
    }

    let reply = legacy_signature(
        account,
        sender_offset_index,
        LedgerKeyBranch::PreMine.as_byte(),
        script_nonce_index,
        LedgerKeyBranch::Random.as_byte(),
        &fixtures::random_challenge(),
    )?;
    expect_status(
        "the sender offset key under the nonce index the script key already signed with",
        &reply,
        AppSW::LegacyNonceReused,
    )
}

/// Acceptance: a legacy nonce index below 2^32 is refused with `BadBranchKey`, before any review.
///
/// An application before 6.1.1-pre.0 derived nonce index `j` as `j mod 2^32`, and an index below 2^32 still derives
/// along that same path - so one old signature plus one new one under `j mod 2^32` would give up a key, and this
/// application's used-nonce record knows nothing of the old one. Over raw APDUs, so the device's check answers.
fn a_nonce_index_below_two_to_the_thirty_two_is_refused(_context: &ScenarioContext<'_>) -> ScenarioResult {
    let account = fixtures::random_u64();
    for nonce_index in [0, 1, (1u64 << 32) - 1, fixtures::random_u64() & 0xFFFF_FFFF] {
        let reply = legacy_signature(
            account,
            fixtures::random_u64() % 100_000,
            LedgerKeyBranch::PreMine.as_byte(),
            nonce_index,
            LedgerKeyBranch::Random.as_byte(),
            &fixtures::random_challenge(),
        )?;
        expect_status(
            &format!("a legacy signature under nonce index {nonce_index}, below 2^32"),
            &reply,
            AppSW::BadBranchKey,
        )?;
    }

    // And an account of 2^32 or more, which derives the same keys and nonces as its low word: the review does not
    // show the account, so the device refuses the ambiguous form outright.
    for account in [1u64 << 32, (1 << 32) | (fixtures::random_u64() % 100), u64::MAX] {
        let reply = legacy_signature(
            account,
            fixtures::random_u64() % 100_000,
            LedgerKeyBranch::PreMine.as_byte(),
            legacy_nonce_index(),
            LedgerKeyBranch::Random.as_byte(),
            &fixtures::random_challenge(),
        )?;
        expect_status(
            &format!("a legacy signature on account {account}, 2^32 or more"),
            &reply,
            AppSW::BadBranchKey,
        )?;
    }
    Ok(())
}
