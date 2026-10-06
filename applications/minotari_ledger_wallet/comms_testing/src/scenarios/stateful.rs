// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! The state that survives between exchanges, which is the part no unit test can reach.
//!
//! Two pieces of device state outlive a single APDU, and they have opposite requirements:
//!
//! * the **script offset context** must be destroyed by anything that is not the next chunk of its accumulation;
//! * the **ephemeral nonce store** must survive exactly that.
//!
//! Both are owned by the main loop in `wallet/src/main.rs` and live for as long as the application does. Neither can
//! be observed without sending several instructions to one device and watching what the later ones see - which is
//! why [`crate::approver::SpeculosApprover::expect_home`] asserts the device came home instead of restarting the
//! simulator between scenarios. A suite that restarted would be unable to see either of these at all.
//!
//! # Script offset: the reset is a spend key protection, not tidiness
//!
//! From `wallet/src/main.rs`, on the line that resets the context for any instruction that is not the next chunk:
//! without it, *"a host whose chunk was rejected could resume with a differently numbered follow-up chunk and read
//! back a value the rejection withheld - for an unblinded script offset, that is the wallet's spend key."*
//!
//! Every rejection scenario below therefore has a **positive control** next to it: the same sequence without the
//! interleaving, or without the rejection, must succeed. A scenario that only ever checks for a refusal passes
//! against a device that refuses everything, which is a different broken device and not one anybody would spot from
//! a green run.
//!
//! # Nonce store: it evicts, and `Full` does not mean full
//!
//! Read [`minotari_ledger_wallet_common::ephemeral_nonce`] before changing anything here. The store is eight slots
//! and it **evicts** rather than refusing, deliberately, because nothing releases a reserved nonce except signing
//! with it and a refusing store would wedge until the application restarted.
//!
//! So `EphemeralNonceStoreError::Full` - `AppSW::NonceStoreFull` on the wire - means *the handle counter is
//! exhausted*, which takes 2^64 reservations. Nine reservations produce `NonceHandleInvalid` on the oldest handle,
//! not `NonceStoreFull`, and expecting the latter is the obvious mistake to make here.

use minotari_ledger_wallet_common::{
    common_types::{AppSW, Instruction, LedgerKeyBranch},
    ephemeral_nonce::{EPHEMERAL_NONCE_STORE_SIZE, INVALID_NONCE_HANDLE},
    script_offset::sender_offset_index,
};
use minotari_ledger_wallet_comms::accessor_methods::{ledger_get_public_key, ledger_get_public_spend_key};
use tari_crypto::{keys::PublicKey, ristretto::RistrettoPublicKey};
use tari_utilities::hex::Hex;

use crate::{
    fixtures,
    raw::{self, EphemeralNonceReply, RawReply, SchnorrReply, ScriptOffsetReply, payload},
    scenarios::{
        Approval,
        Scenario,
        ScenarioContext,
        ScenarioError,
        ScenarioModule,
        ScenarioResult,
        WithContext,
        expect_ok,
        expect_refused,
        expect_status,
        fail,
        require,
    },
};

pub const MODULE: ScenarioModule = ScenarioModule {
    name: "stateful",
    scenarios: SCENARIOS,
};

const SCENARIOS: &[Scenario] = &[
    Scenario {
        name: "an interleaved instruction invalidates an in-progress script offset accumulation",
        covers: &[Instruction::GetScriptOffset, Instruction::GetViewKey],
        approval: Approval::NotNeeded,
        run: an_interleaved_instruction_invalidates_the_accumulation,
    },
    Scenario {
        name: "a chunk numbered outside every declared section folds nothing, and the reply is withheld",
        covers: &[Instruction::GetScriptOffset],
        approval: Approval::NotNeeded,
        run: a_chunk_outside_every_section_folds_nothing,
    },
    Scenario {
        name: "a rejected script offset chunk cannot be resumed from",
        covers: &[Instruction::GetScriptOffset],
        approval: Approval::NotNeeded,
        run: a_rejected_chunk_cannot_be_resumed_from,
    },
    Scenario {
        name: "the host's partial sum can arrive after a folded key without displacing it",
        covers: &[
            Instruction::GetScriptOffset,
            Instruction::GetPublicKey,
            Instruction::GetPublicSpendKey,
        ],
        approval: Approval::NotNeeded,
        run: the_partial_sum_does_not_displace_a_folded_key,
    },
    Scenario {
        name: "nine reservations leave the first handle invalid, not the store full",
        covers: &[Instruction::GenerateEphemeralNonce, Instruction::GetRawSchnorrSignature],
        approval: Approval::NotNeeded,
        run: nine_reservations_evict_the_oldest_handle,
    },
    Scenario {
        name: "a nonce handle is good for exactly one signature, and handle zero for none",
        covers: &[Instruction::GenerateEphemeralNonce, Instruction::GetRawSchnorrSignature],
        approval: Approval::NotNeeded,
        run: a_handle_is_good_for_one_signature,
    },
    Scenario {
        name: "a reserved nonce survives unrelated instructions, including ones the device refused",
        covers: &[
            Instruction::GenerateEphemeralNonce,
            Instruction::GetRawSchnorrSignature,
            Instruction::GetViewKey,
            Instruction::GetScriptOffset,
            Instruction::GetPublicKey,
        ],
        approval: Approval::NotNeeded,
        run: a_reserved_nonce_survives_unrelated_instructions,
    },
];

/// A `GetScriptOffset` header declaring one sender offset key, no indexed script keys and one derived script key.
///
/// The smallest shape that can produce a reply, which makes the chunk numbering easy to reason about: chunk 0 is
/// the header, chunk 1 is the partial sum, chunk 2 is the one derived script key section, and chunk 3 onwards is
/// outside every section.
fn one_derived_key_header() -> Vec<u8> {
    payload::script_offset_header(1, 0, 1)
}

fn script_offset_chunk(account: u64, number: u8, more: bool, data: Vec<u8>) -> Result<RawReply, ScenarioError> {
    raw::chunk(account, Instruction::GetScriptOffset, number, more, data)
        .send()
        .context(|| format!("GetScriptOffset chunk {number}"))
}

/// Acceptance: an unrelated instruction between two chunks destroys the accumulation.
///
/// The sequence is: header, one derived script key, **something else**, then the terminating chunk. Without the
/// reset in the main loop, the terminating chunk would find a context still carrying the header's counts and the
/// folded key, and would emit a script offset. With it, the context is empty, no sender offset keys are declared,
/// and the reply is withheld with `ScriptOffsetNoSenderOffsets`.
///
/// The positive control runs first: the identical sequence *without* the interleaved instruction succeeds. Without
/// it this scenario would pass against a device that had simply stopped serving `GetScriptOffset`.
fn an_interleaved_instruction_invalidates_the_accumulation(_context: &ScenarioContext<'_>) -> ScenarioResult {
    let account = fixtures::random_u64();
    let blinding_factor = fixtures::random_scalar_bytes();

    // Positive control: the same four exchanges, minus the interleaving.
    expect_ok(
        "the control header",
        &script_offset_chunk(account, 0, true, one_derived_key_header())?,
    )?;
    expect_ok(
        "the control script key",
        &script_offset_chunk(
            account,
            2,
            true,
            payload::script_offset_derived_script_key(&blinding_factor),
        )?,
    )?;
    let control = script_offset_chunk(account, 3, false, Vec::new())?;
    expect_ok(
        "a script offset accumulation with nothing interleaved, which is the control for the scenario below",
        &control,
    )?;
    ScriptOffsetReply::parse(&control.data).map_err(fail)?;

    // And now with an unrelated instruction in the middle.
    expect_ok(
        "the header",
        &script_offset_chunk(account, 0, true, one_derived_key_header())?,
    )?;
    expect_ok(
        "the script key",
        &script_offset_chunk(
            account,
            2,
            true,
            payload::script_offset_derived_script_key(&blinding_factor),
        )?,
    )?;

    let interleaved = raw::command(account, Instruction::GetViewKey, payload::account_only())
        .send()
        .context(|| "the interleaved GetViewKey".to_string())?;
    expect_ok("the interleaved GetViewKey", &interleaved)?;

    let resumed = script_offset_chunk(account, 3, false, Vec::new())?;
    expect_status(
        "a script offset accumulation resumed after an unrelated instruction",
        &resumed,
        AppSW::ScriptOffsetNoSenderOffsets,
    )
}

/// Acceptance: declaring a script key section and then terminating on a chunk outside it emits nothing.
///
/// This is the gap between "what the host declared" and "what the device folded", and it is exploitable in two
/// APDUs if the emission check looks at the header. Declare `derived_script_key_count = 1`, then terminate on chunk
/// 3 - which is past the end of that section, since it occupies chunk 2 alone. Nothing is folded, the script side
/// of the sum is still zero, and a check on the declared count would still pass. The reply would be `-k_sender` for
/// a key the device had just generated, with the base index that names it in the same reply.
///
/// The host's partial sum is sent in the second half, and must not rescue it: it is one opaque scalar the host
/// computed itself, so it blinds nothing against the host, and a host with no script keys sends the zero scalar
/// which the device cannot tell apart from a legitimate sum of zero.
fn a_chunk_outside_every_section_folds_nothing(_context: &ScenarioContext<'_>) -> ScenarioResult {
    let account = fixtures::random_u64();

    expect_ok(
        "the header",
        &script_offset_chunk(account, 0, true, one_derived_key_header())?,
    )?;
    let reply = script_offset_chunk(account, 3, false, fixtures::random_scalar_bytes().to_vec())?;
    expect_status(
        "a script offset that declared a script key section and terminated outside it",
        &reply,
        AppSW::ScriptOffsetNoDeviceScriptKeys,
    )?;

    // The same, with only the host's partial sum contributing.
    expect_ok(
        "the header",
        &script_offset_chunk(account, 0, true, one_derived_key_header())?,
    )?;
    expect_ok(
        "the partial sum",
        &script_offset_chunk(
            account,
            1,
            true,
            payload::script_offset_partial_sum(&fixtures::random_scalar_bytes()),
        )?,
    )?;
    let reply = script_offset_chunk(account, 3, false, fixtures::random_scalar_bytes().to_vec())?;
    expect_status(
        "a script offset blinded only by the host's own partial sum",
        &reply,
        AppSW::ScriptOffsetNoDeviceScriptKeys,
    )
}

/// Acceptance: a rejected chunk leaves nothing behind for a follow-up chunk to resume from.
///
/// Both refusable headers are tried, because they are rejected at different points and a reset that covered only
/// one of them would be a live hole:
///
/// * a header with **no sender offset keys** - if it left its counts behind, a host could follow it with a chunk that
///   folds an alpha derived script key and then read the sum back unblinded, which is full spend key recovery in two
///   calls;
/// * a header with **no device derived script keys** - the mirror case, which leaks a sender offset private key.
///
/// The resume attempt is a *terminating* chunk, because a continuation would be accepted whatever the context held
/// and would prove nothing. What it must not do is produce a reply.
fn a_rejected_chunk_cannot_be_resumed_from(_context: &ScenarioContext<'_>) -> ScenarioResult {
    let account = fixtures::random_u64();

    let rejected_headers = [
        ("no sender offset keys", payload::script_offset_header(0, 0, 1)),
        ("no device derived script keys", payload::script_offset_header(1, 0, 0)),
    ];
    for (label, header) in rejected_headers {
        expect_refused(
            &format!("a script offset header with {label}"),
            &script_offset_chunk(account, 0, true, header)?,
        )?;

        // Which status word the resume comes back with is deliberately not asserted. The reset leaves an empty
        // context, so the device has a genuine choice of refusals here - either side of the sum is now missing -
        // and pinning one of them would pin an implementation detail rather than the property. The property is
        // that no reply carrying a script offset comes out.
        for resume_at in [2u8, 3] {
            expect_refused(
                &format!("chunk {resume_at}, resuming a script offset rejected for {label}"),
                &script_offset_chunk(account, resume_at, false, fixtures::random_scalar_bytes().to_vec())?,
            )?;
        }
    }

    // A rejected *body* chunk, rather than a rejected header: the header is accepted, the script key chunk names a
    // branch the device will not address by index, and the follow-up must not be able to finish the exchange.
    expect_ok(
        "the header",
        &script_offset_chunk(account, 0, true, payload::script_offset_header(1, 1, 0))?,
    )?;
    let rejection = script_offset_chunk(
        account,
        2,
        true,
        payload::script_offset_script_index(LedgerKeyBranch::Random, fixtures::random_u64()),
    )?;
    expect_status(
        "a script key addressed by index outside the pre-mine branch",
        &rejection,
        AppSW::ScriptOffsetInvalidScriptBranch,
    )?;
    expect_refused(
        "a chunk resuming an accumulation whose script key chunk the device had just rejected",
        &script_offset_chunk(account, 3, false, Vec::new())?,
    )
}

/// Acceptance: the host chooses the chunk order, and a partial sum arriving after a folded key must not displace
/// it.
///
/// The host picks which chunk numbers it sends and in which order, so the device cannot assume the format was
/// followed. `ScriptOffsetCtx` keeps `host_partial_script_key_sum` in its own field for exactly this reason: the
/// partial sum arrives as a whole *sum* rather than as a term, so assigning it into the running total would let a
/// host fold a device derived key, wipe it with this chunk, and still satisfy a check that counted the folded key.
/// The reply would then be `partial - k_sender`, and the host - who chose `partial` - recovers a sender offset
/// private key by subtraction.
///
/// This sequence is legitimate, so it must **succeed**; what is asserted is the value. The full sum is checked,
/// which subsumes the leak: if the folded key had been overwritten the reply would be exactly `partial - k_sender`,
/// and the equation below would not hold.
fn the_partial_sum_does_not_displace_a_folded_key(_context: &ScenarioContext<'_>) -> ScenarioResult {
    let account = fixtures::random_u64();
    let partial_sum = fixtures::random_secret_key();
    let blinding_factor = fixtures::random_secret_key();

    // Deliberately out of order: the folded key first, the partial sum afterwards.
    expect_ok(
        "the header",
        &script_offset_chunk(account, 0, true, one_derived_key_header())?,
    )?;
    expect_ok(
        "the derived script key",
        &script_offset_chunk(
            account,
            2,
            true,
            payload::script_offset_derived_script_key(&fixtures::secret_key_bytes(&blinding_factor)),
        )?,
    )?;
    expect_ok(
        "the partial sum, sent after the key it must not displace",
        &script_offset_chunk(
            account,
            1,
            true,
            payload::script_offset_partial_sum(&fixtures::secret_key_bytes(&partial_sum)),
        )?,
    )?;
    let reply = script_offset_chunk(account, 3, false, Vec::new())?;
    expect_ok("a legitimate script offset sent out of order", &reply)?;

    let parsed = ScriptOffsetReply::parse(&reply.data).map_err(fail)?;
    let script_offset = fixtures::secret_key_from_bytes("the script offset", &parsed.script_offset).map_err(fail)?;

    let public_spend_key = ledger_get_public_spend_key(account)
        .context(|| "GetPublicSpendKey".to_string())?
        .to_public_key()
        .context(|| "the device's public spend key would not decompress".to_string())?;
    let sender_offset = ledger_get_public_key(
        account,
        sender_offset_index(parsed.base_index, 0),
        LedgerKeyBranch::OneSidedSenderOffset,
    )
    .context(|| "GetPublicKey for the sender offset key".to_string())?;

    let expected = fixtures::alpha_derived_script_public_key(&blinding_factor, &public_spend_key) +
        RistrettoPublicKey::from_secret_key(&partial_sum) -
        sender_offset;

    require(RistrettoPublicKey::from_secret_key(&script_offset) == expected, || {
        format!(
            "the out of order script offset is {} but should be {}. If the partial sum had overwritten the folded \
             script key the reply would be `partial - k_sender`, which hands the host a sender offset private key.",
            RistrettoPublicKey::from_secret_key(&script_offset).to_hex(),
            expected.to_hex()
        )
    })
}

/// Reserve one ephemeral nonce.
fn reserve_nonce(account: u64) -> Result<EphemeralNonceReply, ScenarioError> {
    let reply = raw::command(
        account,
        Instruction::GenerateEphemeralNonce,
        payload::generate_ephemeral_nonce(),
    )
    .send()
    .context(|| "GenerateEphemeralNonce".to_string())?;
    expect_ok("GenerateEphemeralNonce", &reply)?;
    EphemeralNonceReply::parse(&reply.data).map_err(fail)
}

/// Ask for a raw Schnorr signature with `handle`, without asserting anything about the answer.
fn sign_with_handle(
    account: u64,
    index: u64,
    branch: LedgerKeyBranch,
    handle: u64,
    challenge: &[u8; 64],
) -> Result<RawReply, ScenarioError> {
    raw::command(
        account,
        Instruction::GetRawSchnorrSignature,
        payload::raw_schnorr_signature(index, branch, handle, challenge),
    )
    .send()
    .context(|| format!("GetRawSchnorrSignature with nonce handle {handle}"))
}

/// Acceptance: a ninth reservation evicts the first, and the evicted handle is refused as
/// **`NonceHandleInvalid`** - not `NonceStoreFull`.
///
/// Getting that backwards is the obvious mistake, and it is worth stating why it is a mistake rather than a
/// preference. The store evicts because nothing releases a reserved nonce except signing with it: a caller that
/// reserves and then fails - a transport error, a user rejecting the next prompt - abandons the slot for the life
/// of the application, and a store that refused when full would turn a handful of ordinary failures into a wallet
/// that cannot sign at all until the application restarts. `NonceStoreFull` is reserved for handle *counter*
/// exhaustion, which takes 2^64 reservations and must refuse because the alternative is re-issuing a handle an
/// earlier nonce still answers to.
///
/// The eviction target is deterministic whatever else is in the store when this runs. Handles are issued from a
/// strictly increasing counter, so every entry left by an earlier scenario has a smaller handle than any of these
/// nine; the first eight reservations therefore push all of them out, and the ninth evicts the oldest of *this*
/// batch. That is what lets this scenario run in a suite that deliberately never restarts the device.
fn nine_reservations_evict_the_oldest_handle(_context: &ScenarioContext<'_>) -> ScenarioResult {
    let account = fixtures::random_u64();
    let reservations = EPHEMERAL_NONCE_STORE_SIZE.saturating_add(1);

    let mut handles = Vec::with_capacity(reservations);
    for _ in 0..reservations {
        handles.push(reserve_nonce(account)?.handle);
    }

    // Handles are issued from a counter, so they must be strictly increasing and never `INVALID_NONCE_HANDLE`.
    for window in handles.windows(2) {
        let (first, second) = (window.first().copied(), window.get(1).copied());
        require(matches!((first, second), (Some(a), Some(b)) if b > a), || {
            format!("the device re-issued or lowered a nonce handle: {handles:?}")
        })?;
    }
    require(handles.iter().all(|handle| *handle != INVALID_NONCE_HANDLE), || {
        format!("the device issued the reserved 'no handle' value: {handles:?}")
    })?;

    let evicted = handles
        .first()
        .copied()
        .ok_or_else(|| fail("no nonces were reserved"))?;
    let survivor = handles.last().copied().ok_or_else(|| fail("no nonces were reserved"))?;

    let reply = sign_with_handle(
        account,
        fixtures::random_u64(),
        LedgerKeyBranch::Random,
        evicted,
        &fixtures::random_challenge(),
    )?;
    expect_status(
        &format!(
            "the oldest of {reservations} reserved nonces, which the {reservations}th reservation evicted from a \
             {EPHEMERAL_NONCE_STORE_SIZE} slot store"
        ),
        &reply,
        AppSW::NonceHandleInvalid,
    )?;

    // The newest reservation - the one that did the evicting - is still good. A store that had refused the ninth
    // reservation outright, or that had wiped itself, would fail here rather than passing the assertion above for
    // the wrong reason.
    let reply = sign_with_handle(
        account,
        fixtures::random_u64(),
        LedgerKeyBranch::Random,
        survivor,
        &fixtures::random_challenge(),
    )?;
    expect_ok(
        "the newest reservation, which is the one that evicted the oldest",
        &reply,
    )
}

/// Acceptance: a handle names a nonce exactly once, and handle zero names none.
///
/// Taking a nonce out of the store is the same operation as reading it, and the handler takes it *before* signing,
/// so every path - success, a signing failure, anything a later change adds - leaves the slot empty. A slot that
/// survived would be a nonce the host could spend a second time on a different challenge, which is the whole
/// disclosure this instruction was introduced to close.
///
/// [`INVALID_NONCE_HANDLE`] is checked alongside because zero is reserved rather than merely unused: an empty slot
/// holds it, so a host sending a zeroed handle must be refused rather than served whatever sits in slot zero.
fn a_handle_is_good_for_one_signature(_context: &ScenarioContext<'_>) -> ScenarioResult {
    let account = fixtures::random_u64();
    let index = fixtures::random_u64();
    let branch = LedgerKeyBranch::Random;

    let reserved = reserve_nonce(account)?;
    let first = sign_with_handle(account, index, branch, reserved.handle, &fixtures::random_challenge())?;
    expect_ok("the first signature with a freshly reserved nonce", &first)?;

    let second = sign_with_handle(account, index, branch, reserved.handle, &fixtures::random_challenge())?;
    expect_status(
        "a second signature with a nonce handle that has already been spent",
        &second,
        AppSW::NonceHandleInvalid,
    )?;

    let zero = sign_with_handle(
        account,
        index,
        branch,
        INVALID_NONCE_HANDLE,
        &fixtures::random_challenge(),
    )?;
    expect_status(
        "a signature with handle 0, which is the reserved 'no handle' value",
        &zero,
        AppSW::NonceHandleInvalid,
    )?;

    // A handle that was never issued. Above the counter rather than below it, because a handle below the counter
    // might legitimately have been issued and spent by an earlier scenario, and this is asking about one that was
    // never issued at all.
    let never_issued = reserved.handle.saturating_add(1_000_000);
    let unknown = sign_with_handle(account, index, branch, never_issued, &fixtures::random_challenge())?;
    expect_status(
        "a signature with a handle the device never issued",
        &unknown,
        AppSW::NonceHandleInvalid,
    )
}

/// Acceptance: a reserved nonce survives whatever happens between reserving it and signing with it.
///
/// **This is the whole point of the store**, and the one assertion that Spec 3's no-restart-between-scenarios rule
/// exists to make possible. The reserve-do-other-work-then-sign shape is not a corner case; it is how a multi party
/// exchange uses this instruction, and every round trip in between is one the user can reject.
///
/// The interleaved instructions are chosen to include the things most likely to disturb it: a plain read, a
/// *rejected* APDU, and a `GetScriptOffset` accumulation - which is the one thing in the main loop that has its own
/// reset, so an over-broad reset there would take the nonce store with it.
///
/// The signature is verified rather than merely accepted, because "the handle was still valid" and "the handle
/// still named the nonce it was reserved for" are different claims, and only the second one is any use.
fn a_reserved_nonce_survives_unrelated_instructions(_context: &ScenarioContext<'_>) -> ScenarioResult {
    let account = fixtures::random_u64();
    let index = fixtures::random_u64();
    let branch = LedgerKeyBranch::PreMine;
    let challenge = fixtures::random_challenge();

    let reserved = reserve_nonce(account)?;

    // A plain read.
    expect_ok(
        "the interleaved GetViewKey",
        &raw::command(account, Instruction::GetViewKey, payload::account_only())
            .send()
            .context(|| "the interleaved GetViewKey".to_string())?,
    )?;

    // An APDU the device refuses. A rejection unwinds through the main loop's error arm, which resets the script
    // offset context - it must not reset anything else.
    let refused = raw::command(
        account,
        Instruction::GetPublicKey,
        payload::public_key(index, LedgerKeyBranch::Spend),
    )
    .send()
    .context(|| "the interleaved rejected GetPublicKey".to_string())?;
    expect_status(
        "the interleaved GetPublicKey on the spend branch",
        &refused,
        AppSW::BadBranchKey,
    )?;

    // A whole `GetScriptOffset` accumulation, start to finish.
    expect_ok(
        "the interleaved script offset header",
        &script_offset_chunk(account, 0, true, one_derived_key_header())?,
    )?;
    expect_ok(
        "the interleaved script offset key",
        &script_offset_chunk(
            account,
            2,
            true,
            payload::script_offset_derived_script_key(&fixtures::random_scalar_bytes()),
        )?,
    )?;
    expect_ok(
        "the interleaved script offset",
        &script_offset_chunk(account, 3, false, Vec::new())?,
    )?;

    // And only now, sign with the handle reserved before any of that.
    let reply = sign_with_handle(account, index, branch, reserved.handle, &challenge)?;
    expect_ok(
        "a signature with a nonce reserved before several unrelated instructions",
        &reply,
    )?;

    let parsed = SchnorrReply::parse(&reply.data).map_err(fail)?;
    require(parsed.public_nonce == reserved.public_nonce, || {
        format!(
            "the signature's public nonce is {} but the reservation returned {}, so the device signed with some other \
             nonce than the one the handle named",
            parsed.public_nonce.to_vec().to_hex(),
            reserved.public_nonce.to_vec().to_hex()
        )
    })?;

    let public_key = ledger_get_public_key(account, index, branch).context(|| "GetPublicKey".to_string())?;
    let signature = tari_common_types::types::CompressedSignature::new(
        tari_common_types::types::CompressedPublicKey::new(&parsed.public_nonce),
        fixtures::secret_key_from_bytes("the signature", &parsed.signature).map_err(fail)?,
    )
    .to_schnorr_signature()
    .context(|| "the device's compressed signature would not decompress".to_string())?;

    require(signature.verify_raw_uniform(&public_key, &challenge), || {
        "a signature made with a nonce that had survived several unrelated instructions does not verify".to_string()
    })
}
