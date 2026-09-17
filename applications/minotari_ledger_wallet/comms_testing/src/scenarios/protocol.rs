// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Malformed APDUs: what the device refuses, and with which status word.
//!
//! Every probe here goes out over [`crate::raw`], because that is the only way to send them. The accessor methods
//! are the host side *mirror* of these rules and refuse most of them before opening the transport, so a probe
//! driven through an accessor would test the mirror and report green against a device with no checks at all.
//!
//! # The status word is asserted exactly, never "some failure"
//!
//! "The device refused it" and "the device refused it for the reason the rule is written in terms of" are different
//! statements, and only the second survives a refactor that moves a check somewhere it no longer covers the case.
//! A `Spend` branch refused as `WrongApduLength` would mean the branch mapping was never reached.
//!
//! # Several rejection paths cannot be probed unattended on BAGL models, and that is a device bug
//!
//! Most of the device application's error paths look like this on BAGL models (`nanosplus`, `nanox`) - 25 call
//! sites across nine files, by `grep -rn 'show_and_wait()\|\.event_loop()' wallet/src`:
//!
//! ```text
//! SingleMessage::new("Invalid data length").show_and_wait();
//! return Err(AppSW::WrongApduLength);
//! ```
//!
//! `show_and_wait` is a **blocking loop on a button press**, and `MessageScroller::event_loop` is the same. The
//! status word is correct and does arrive - after a human touches the device. So a single malformed APDU leaves the
//! device showing a modal that only a person can dismiss, with the host's exchange outstanding the whole time and
//! no way to cancel it from that side. A host does not have to be malicious to send one; a truncated write does
//! it, and `ledger` is a default feature of the console wallet.
//!
//! The paths a *host* can reach are the ones that matter here:
//!
//! | Path | Status word | Reachable from the host |
//! |---|---|---|
//! | the nine handlers' payload length checks | `WrongApduLength` | yes, any wrong length |
//! | `get_key_from_canonical_bytes` | `KeyDeriveFromCanonical` | yes, any non-canonical key or scalar |
//! | `get_one_sided_metadata_signature`'s address parse | `MetadataSignatureFail` | yes, a bad address checksum |
//! | `get_key_from_uniform_bytes`, the signing failures | various | no, device internal |
//!
//! The NBGL path (`stax`, `flex`) uses `NbglStatus::show`, which draws and returns. It is what the BAGL path should
//! look like, and it is why the whole suite passes unattended on `stax` today.
//!
//! **This is a finding of this suite, not a design choice of it.** It is left unfixed here on purpose: changing
//! device behaviour and adding the tests that describe device behaviour in one change makes both impossible to
//! review. See `tests/speculos_scenarios.rs`, which carries the probe that demonstrates it as the one `#[ignore]`d
//! test in this crate that is not the "needs a simulator" gate.
//!
//! Two consequences for this module, both of them narrowings rather than gaps in what is asserted:
//!
//! * the length probes aim at the handlers that answer `WrongApduLength` with **no UI at all** - the `GetScriptOffset`
//!   chunks and `GetOneSidedMetadataSignature`'s minimum size check. Those are genuine fixed length payloads and
//!   genuine length rules; what they are not is the whole set.
//! * `KeyDeriveFromCanonical` and `MetadataSignatureFail` have no scenario at all, despite being the two status words
//!   in `AppSW` that a host can otherwise provoke directly. When the blocking goes, they are the first two scenarios to
//!   add here.

use minotari_ledger_wallet_common::{
    common_types::{AppSW, Instruction, LedgerKeyBranch},
    script_offset::{MAX_SENDER_OFFSET_KEYS, SCRIPT_OFFSET_HEADER_SIZE},
};

use crate::{
    fixtures,
    raw::{self, MAX_PAYLOADS, SW_BAD_CLA, payload},
    scenarios::{
        Approval,
        Scenario,
        ScenarioContext,
        ScenarioModule,
        ScenarioResult,
        WithContext,
        expect_ok,
        expect_raw_status,
        expect_status,
        require,
    },
};

pub const MODULE: ScenarioModule = ScenarioModule {
    name: "protocol",
    scenarios: SCENARIOS,
};

const SCENARIOS: &[Scenario] = &[
    Scenario {
        name: "a wrong class byte is refused by the SDK before the application sees it",
        covers: &[Instruction::GetPublicKey],
        approval: Approval::NotNeeded,
        run: a_wrong_class_byte_is_refused,
    },
    Scenario {
        name: "an instruction byte the application does not serve is InsNotSupported",
        covers: &[Instruction::GetPublicKey],
        approval: Approval::NotNeeded,
        run: an_unknown_instruction_is_refused,
    },
    Scenario {
        name: "non-zero P1 or P2 is refused, with WrongP1P2 where the application distinguishes it",
        covers: &[
            Instruction::GetPublicKey,
            Instruction::GetScriptSchnorrSignature,
            Instruction::GetViewKey,
        ],
        approval: Approval::NotNeeded,
        run: bad_p1_p2_is_refused,
    },
    Scenario {
        name: "a payload one byte short or one byte long is WrongApduLength",
        covers: &[Instruction::GetScriptOffset, Instruction::GetOneSidedMetadataSignature],
        approval: Approval::NotNeeded,
        run: a_wrong_length_payload_is_refused,
    },
    Scenario {
        name: "the spend branch is not addressable from the host, on any instruction that takes a branch",
        covers: &[
            Instruction::GetPublicKey,
            Instruction::GetDHSharedSecret,
            Instruction::GetScriptSchnorrSignature,
            Instruction::GetScriptOffset,
        ],
        approval: Approval::NotNeeded,
        run: the_spend_branch_is_refused,
    },
    Scenario {
        name: "a chunk number above MAX_PAYLOADS is refused",
        covers: &[Instruction::GetScriptOffset],
        approval: Approval::NotNeeded,
        run: a_chunk_number_above_the_maximum_is_refused,
    },
    Scenario {
        name: "a script offset header the device would not blind is refused, on both sides of the sum",
        covers: &[Instruction::GetScriptOffset],
        approval: Approval::NotNeeded,
        run: an_unblindable_header_is_refused,
    },
];

/// A well formed `GetPublicKey`, which the probes below bend one field of at a time.
///
/// Starting from a request the device *would* accept is what makes each rejection attributable: a hand written bad
/// APDU can be wrong in two ways at once and then proves nothing about either.
fn valid_public_key_request(account: u64) -> raw::RawRequest {
    raw::command(
        account,
        Instruction::GetPublicKey,
        payload::public_key(fixtures::random_u64(), LedgerKeyBranch::Random),
    )
}

/// Acceptance: a class byte other than `0x80` is refused, and the device carries on afterwards.
///
/// `Comm::new().set_expected_cla(CLA)` in `wallet/src/main.rs` makes the SDK answer this before the application's
/// `TryFrom<ApduHeader>` is reached, so the status word is the SDK's [`SW_BAD_CLA`] rather than anything in
/// [`AppSW`] - which is exactly why [`crate::raw::RawReply`] keeps the status as a `u16`.
///
/// The "and carries on" half is not padding. This is the one probe that does not reach the application at all, so
/// it is the one most likely to leave the SDK's command buffer in a state the next instruction inherits.
fn a_wrong_class_byte_is_refused(_context: &ScenarioContext<'_>) -> ScenarioResult {
    let account = fixtures::random_u64();
    let valid = valid_public_key_request(account);

    for cla in [0x00u8, 0x81, 0xff] {
        let reply = valid
            .clone()
            .with_cla(cla)
            .send()
            .context(|| format!("GetPublicKey with CLA {cla:#04x}"))?;
        expect_raw_status(&format!("a command with CLA {cla:#04x}"), &reply, SW_BAD_CLA)?;
    }

    let reply = valid.send().context(|| "GetPublicKey after a wrong CLA".to_string())?;
    expect_ok("the instruction after a wrong class byte", &reply)
}

/// Acceptance: an instruction byte the application does not serve is `InsNotSupported`.
///
/// The probe bytes are chosen to bracket the real set rather than to be far away from it: `0x00` is below it,
/// `0x0a` sits in the gap between `GetRawSchnorrSignature` (`0x09`) and `GetScriptSchnorrSignature` (`0x10`) - the
/// instruction numbering has a hole there, and a hole is where an off-by-one in a dispatch table lands - and `0x15`
/// is one past the highest. Each is checked against `Instruction::from_byte` first, so that a future instruction
/// taking one of these numbers turns into a legible failure here rather than a confusing one.
fn an_unknown_instruction_is_refused(_context: &ScenarioContext<'_>) -> ScenarioResult {
    let account = fixtures::random_u64();
    let valid = valid_public_key_request(account);

    for ins in [0x00u8, 0x0a, 0x15, 0xff] {
        require(Instruction::from_byte(ins).is_none(), || {
            format!("{ins:#04x} is now a real instruction; pick a byte that is not")
        })?;
        let reply = valid
            .clone()
            .with_ins(ins)
            .send()
            .context(|| format!("an APDU with instruction {ins:#04x}"))?;
        expect_status(
            &format!("an APDU with the unknown instruction {ins:#04x}"),
            &reply,
            AppSW::InsNotSupported,
        )?;
    }
    Ok(())
}

/// Acceptance: P1 and P2 must be zero on every instruction but `GetScriptOffset`, which uses them for chunking.
///
/// The status word differs by instruction, and the difference is in the device's own `TryFrom<ApduHeader>` rather
/// than in this scenario: `GetScriptSchnorrSignature` has an explicit `(_, _, _) => Err(AppSW::WrongP1P2)` arm, and
/// everything else falls through to the table's `InsNotSupported` catch-all. Both are asserted as they are, because
/// asserting "some refusal" would hide the day one of them stops matching its arm.
fn bad_p1_p2_is_refused(_context: &ScenarioContext<'_>) -> ScenarioResult {
    let account = fixtures::random_u64();

    // The one instruction with a dedicated wrong-parameters arm.
    let schnorr = raw::command(
        account,
        Instruction::GetScriptSchnorrSignature,
        payload::script_schnorr_signature(
            fixtures::random_u64(),
            LedgerKeyBranch::Random,
            &fixtures::random_bytes_32(),
        ),
    );
    for (p1, p2) in [(1u8, 0u8), (0, 1), (0xff, 0xff)] {
        let reply = schnorr
            .clone()
            .with_p1(p1)
            .with_p2(p2)
            .send()
            .context(|| format!("GetScriptSchnorrSignature with P1={p1:#04x} P2={p2:#04x}"))?;
        expect_status(
            &format!("GetScriptSchnorrSignature with P1={p1:#04x} P2={p2:#04x}"),
            &reply,
            AppSW::WrongP1P2,
        )?;
    }

    // Everything else falls through to the catch-all.
    let public_key = valid_public_key_request(account);
    let view_key = raw::command(account, Instruction::GetViewKey, payload::account_only());
    for (name, request) in [("GetPublicKey", public_key), ("GetViewKey", view_key)] {
        for (p1, p2) in [(1u8, 0u8), (0, 1)] {
            let reply = request
                .clone()
                .with_p1(p1)
                .with_p2(p2)
                .send()
                .context(|| format!("{name} with P1={p1:#04x} P2={p2:#04x}"))?;
            expect_status(
                &format!("{name} with P1={p1:#04x} P2={p2:#04x}"),
                &reply,
                AppSW::InsNotSupported,
            )?;
        }
    }
    Ok(())
}

/// The smallest `GetOneSidedMetadataSignature` payload the device will look at, from the handler's own comment:
/// `account(8) + network(8) + txo_version(8) + sender_offset_key_index(8) + value(8) + commitment_mask(32) +
/// address_size(2) + min_address(67) + message(32)`.
const METADATA_SIGNATURE_MINIMUM: usize = 171;

/// Acceptance: a payload one byte short, or one byte long, is `WrongApduLength`.
///
/// Both directions, because they fail differently in the handler: one byte short is caught by the length check, and
/// one byte long would - without the check - leave a handler reading its fields from the right offsets and quietly
/// ignoring a trailing byte the host chose. A length rule that only rejected short payloads would be a length rule
/// that lets a host append.
///
/// Aimed at `GetScriptOffset` and at `GetOneSidedMetadataSignature`'s minimum, which are the handlers that answer
/// `WrongApduLength` without first blocking on a button press. See the module docs: that restriction is a device
/// bug, not a gap in this scenario.
///
/// The `GetOneSidedMetadataSignature` probe is one byte **below** the minimum rather than an exact size, and that
/// is the point of aiming it there: it is refused before the review is built, so this scenario stays unattended on
/// a device whose only review screen belongs to that same instruction.
fn a_wrong_length_payload_is_refused(_context: &ScenarioContext<'_>) -> ScenarioResult {
    let account = fixtures::random_u64();

    // The header chunk: exactly `SCRIPT_OFFSET_HEADER_SIZE` bytes with the account.
    let header = raw::chunk(
        account,
        Instruction::GetScriptOffset,
        0,
        true,
        payload::script_offset_header(1, 0, 1),
    );
    require(header.data.len() == SCRIPT_OFFSET_HEADER_SIZE, || {
        format!(
            "the script offset header is {} bytes, but SCRIPT_OFFSET_HEADER_SIZE says {SCRIPT_OFFSET_HEADER_SIZE}; \
             the probes below would be bracketing the wrong length",
            header.data.len()
        )
    })?;
    for length in [
        SCRIPT_OFFSET_HEADER_SIZE.saturating_sub(1),
        SCRIPT_OFFSET_HEADER_SIZE.saturating_add(1),
    ] {
        let reply = header
            .clone()
            .with_data_length(length)
            .send()
            .context(|| format!("a {length} byte GetScriptOffset header"))?;
        expect_status(
            &format!("a {length} byte GetScriptOffset header, which must be {SCRIPT_OFFSET_HEADER_SIZE}"),
            &reply,
            AppSW::WrongApduLength,
        )?;
    }

    // The partial sum chunk: exactly 32 bytes, and not account prefixed. Probed after a *valid* header, so that
    // the handler has really reached the chunk 1 branch rather than bailing out on an empty context.
    let valid_header = raw::chunk(
        account,
        Instruction::GetScriptOffset,
        0,
        true,
        payload::script_offset_header(1, 0, 1),
    );
    for length in [31usize, 33] {
        expect_ok(
            "the header before a wrong length partial sum",
            &valid_header
                .clone()
                .send()
                .context(|| "GetScriptOffset header".to_string())?,
        )?;
        let reply = raw::chunk(account, Instruction::GetScriptOffset, 1, true, vec![0u8; length])
            .send()
            .context(|| format!("a {length} byte GetScriptOffset partial sum"))?;
        expect_status(
            &format!("a {length} byte GetScriptOffset partial sum, which must be 32"),
            &reply,
            AppSW::WrongApduLength,
        )?;
    }

    // `GetOneSidedMetadataSignature`, one byte below the size at which the handler will even look at the fields.
    let reply = raw::command(
        account,
        Instruction::GetOneSidedMetadataSignature,
        vec![0u8; METADATA_SIGNATURE_MINIMUM.saturating_sub(9)],
    )
    .send()
    .context(|| "a short GetOneSidedMetadataSignature".to_string())?;
    expect_status(
        &format!("a GetOneSidedMetadataSignature one byte below its {METADATA_SIGNATURE_MINIMUM} byte minimum"),
        &reply,
        AppSW::WrongApduLength,
    )
}

/// Acceptance: `LedgerKeyBranch::Spend` is refused wherever the host can name a branch.
///
/// `alpha` is the wallet's root spend key, and no handler may be pointed at it on the host's say-so.
/// `KeyType::from_branch_key` has no mapping for the spend branch and answers `BadBranchKey`, which is what keeps
/// it reachable only internally through `derive_from_bip32_key(account, STATIC_SPEND_INDEX, KeyType::Spend)`.
///
/// Every instruction that takes a branch is probed, not a representative one. The mapping is called separately in
/// each handler, so "one of them still checks" is not the property that matters.
///
/// `GetScriptOffset`'s indexed script key chunk is here too, with a twist: it refuses a `Random` branch as well,
/// with its own `ScriptOffsetInvalidScriptBranch`, because only pre-mine script keys may be addressed by index.
/// Both refusals are asserted, because they are two different rules and only one of them is the spend branch rule.
fn the_spend_branch_is_refused(_context: &ScenarioContext<'_>) -> ScenarioResult {
    let account = fixtures::random_u64();
    let index = fixtures::random_u64();
    let spend = LedgerKeyBranch::Spend;

    let probes: [(&str, raw::RawRequest); 3] = [
        (
            "GetPublicKey",
            raw::command(account, Instruction::GetPublicKey, payload::public_key(index, spend)),
        ),
        (
            "GetDHSharedSecret",
            raw::command(
                account,
                Instruction::GetDHSharedSecret,
                payload::dh_shared_secret(index, spend, &fixtures::random_scalar_bytes()),
            ),
        ),
        (
            "GetScriptSchnorrSignature",
            raw::command(
                account,
                Instruction::GetScriptSchnorrSignature,
                payload::script_schnorr_signature(index, spend, &fixtures::random_bytes_32()),
            ),
        ),
    ];
    for (name, request) in probes {
        let reply = request.send().context(|| format!("{name} on the spend branch"))?;
        expect_status(&format!("{name} on the spend branch"), &reply, AppSW::BadBranchKey)?;
    }

    // The script offset's indexed script key chunk. A valid header first, so the chunk lands in the indexed script
    // key section rather than in no section at all - in which case it would carry nothing and be accepted.
    for (branch, expected) in [
        (LedgerKeyBranch::Spend, AppSW::BadBranchKey),
        (LedgerKeyBranch::Random, AppSW::ScriptOffsetInvalidScriptBranch),
    ] {
        let header = raw::chunk(
            account,
            Instruction::GetScriptOffset,
            0,
            true,
            payload::script_offset_header(1, 1, 0),
        )
        .send()
        .context(|| "GetScriptOffset header".to_string())?;
        expect_ok("the header before a bad script key branch", &header)?;

        let reply = raw::chunk(
            account,
            Instruction::GetScriptOffset,
            2,
            false,
            payload::script_offset_script_index(branch, index),
        )
        .send()
        .context(|| format!("a GetScriptOffset script key on the {branch} branch"))?;
        expect_status(
            &format!("a GetScriptOffset script key addressed by index on the {branch} branch"),
            &reply,
            expected,
        )?;
    }
    Ok(())
}

/// Acceptance: a chunk number above `MAX_PAYLOADS` is refused.
///
/// `GetScriptOffset` is the only instruction with a non-zero P1, and its accepted range is `0..=MAX_PAYLOADS` in
/// the device's `TryFrom<ApduHeader>`. Above that the match falls through to the table's catch-all, so the status
/// word is `InsNotSupported` rather than `WrongP1P2` - which reads oddly and is exactly why it is worth pinning:
/// a reader guessing from the name would guess wrong, and a future rearrangement of that match would change it
/// without anybody noticing.
///
/// `MAX_PAYLOADS` itself is probed as well, on the accepting side. A bound tested only from above can be off by one
/// in the direction that refuses legitimate work, and 251 chunks is a legitimate request.
fn a_chunk_number_above_the_maximum_is_refused(_context: &ScenarioContext<'_>) -> ScenarioResult {
    let account = fixtures::random_u64();

    for chunk_number in [MAX_PAYLOADS.saturating_add(1), u8::MAX] {
        let reply = raw::chunk(
            account,
            Instruction::GetScriptOffset,
            chunk_number,
            true,
            fixtures::random_scalar_bytes().to_vec(),
        )
        .send()
        .context(|| format!("GetScriptOffset chunk {chunk_number}"))?;
        expect_status(
            &format!("GetScriptOffset chunk {chunk_number}, above MAX_PAYLOADS ({MAX_PAYLOADS})"),
            &reply,
            AppSW::InsNotSupported,
        )?;
    }

    // And P2, whose only accepted values are 0 and 1.
    let reply = raw::chunk(
        account,
        Instruction::GetScriptOffset,
        0,
        false,
        payload::script_offset_header(1, 0, 1),
    )
    .with_p2(2)
    .send()
    .context(|| "GetScriptOffset with P2=2".to_string())?;
    expect_status("GetScriptOffset with a P2 of 2", &reply, AppSW::InsNotSupported)?;

    // The boundary itself is accepted. It carries no script key - chunk 250 is outside every section for these
    // counts - so it is accepted as a continuation and nothing is emitted, which is the behaviour being pinned.
    let header = raw::chunk(
        account,
        Instruction::GetScriptOffset,
        0,
        true,
        payload::script_offset_header(1, 0, 1),
    )
    .send()
    .context(|| "GetScriptOffset header".to_string())?;
    expect_ok("the header before the boundary chunk", &header)?;
    let reply = raw::chunk(
        account,
        Instruction::GetScriptOffset,
        MAX_PAYLOADS,
        true,
        fixtures::random_scalar_bytes().to_vec(),
    )
    .send()
    .context(|| format!("GetScriptOffset chunk {MAX_PAYLOADS}"))?;
    expect_ok(
        &format!("GetScriptOffset chunk {MAX_PAYLOADS}, which is exactly MAX_PAYLOADS"),
        &reply,
    )
}

/// Acceptance: a header that would leave the reply unblinded on either side of the sum is refused, and so is one
/// that asks for more sender offset keys than the device will derive.
///
/// The three rules are in `minotari_ledger_wallet_common::script_offset` and are unit tested there; what is
/// asserted here is that the **device** applies them, and which status word each maps to.
///
/// * **No sender offset keys** -> `ScriptOffsetNoSenderOffsets`. The reply would be the plain sum of the input script
///   private keys, and since the host chose the blinding factors those were derived from, it could strip them off and
///   be left with `alpha`.
/// * **No script keys the device derived** -> `ScriptOffsetNoDeviceScriptKeys`. The reply would be `-k_sender` for a
///   key the device had just generated, with the index that names it in the same reply.
/// * **More than `MAX_SENDER_OFFSET_KEYS`** -> `WrongApduLength`, which is `header_error_to_app_sw`'s choice and reads
///   oddly enough to be worth pinning rather than rediscovering.
fn an_unblindable_header_is_refused(_context: &ScenarioContext<'_>) -> ScenarioResult {
    let account = fixtures::random_u64();

    let cases = [
        (
            "no sender offset keys",
            payload::script_offset_header(0, 0, 1),
            AppSW::ScriptOffsetNoSenderOffsets,
        ),
        (
            "no script keys the device derived",
            payload::script_offset_header(1, 0, 0),
            AppSW::ScriptOffsetNoDeviceScriptKeys,
        ),
        (
            "more sender offset keys than the device will derive",
            payload::script_offset_header(MAX_SENDER_OFFSET_KEYS.saturating_add(1), 0, 1),
            AppSW::WrongApduLength,
        ),
    ];
    for (label, header, expected) in cases {
        let reply = raw::chunk(account, Instruction::GetScriptOffset, 0, true, header)
            .send()
            .context(|| format!("a GetScriptOffset header with {label}"))?;
        expect_status(&format!("a GetScriptOffset header with {label}"), &reply, expected)?;
    }

    // The bound itself is accepted, so that a bound tested only from above cannot drift downwards and start
    // refusing legitimate requests.
    let reply = raw::chunk(
        account,
        Instruction::GetScriptOffset,
        0,
        true,
        payload::script_offset_header(MAX_SENDER_OFFSET_KEYS, 0, 1),
    )
    .send()
    .context(|| "a GetScriptOffset header at MAX_SENDER_OFFSET_KEYS".to_string())?;
    expect_ok(
        &format!("a GetScriptOffset header asking for exactly MAX_SENDER_OFFSET_KEYS ({MAX_SENDER_OFFSET_KEYS})"),
        &reply,
    )
}
