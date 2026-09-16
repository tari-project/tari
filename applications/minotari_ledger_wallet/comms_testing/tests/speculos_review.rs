// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! The review screen, asserted against a running device.
//!
//! # Running these
//!
//! Every test here is `#[ignore]`d, and that is the only gate - see `simulator`'s module docs for why an
//! environment variable would be the wrong one. A plain `cargo test` reports them as *ignored*, which `libtest`
//! prints distinctly from "passed", so a machine with no simulator is quiet without ever claiming the review
//! screen was checked.
//!
//! ```text
//! ./scripts/ledger_speculos.sh test                          # build, run every model and seed, tear down
//!
//! ./scripts/ledger_speculos.sh up stax default               # or iterate against one
//! SPECULOS_APDU_ADDRESS=$(./scripts/ledger_speculos.sh address stax default) \
//! SPECULOS_API_ADDRESS=$(./scripts/ledger_speculos.sh api-address stax default) \
//! SPECULOS_MODEL=stax SPECULOS_SEED_ID=default \
//!   cargo test --manifest-path applications/minotari_ledger_wallet/comms_testing/Cargo.toml \
//!     --test speculos_review -- --ignored --test-threads=1
//! ```
//!
//! # Why these need a simulator and cannot touch real hardware
//!
//! `SpeculosApprover` drives the device over Speculos' HTTP control API, which a real Ledger does not have. There
//! is therefore no way to point this file at hardware even by accident, which is why it carries no equivalent of
//! `speculos_device.rs`'s "is this a published test device" guard: that guard exists because the vector tests
//! transcribe a device's *answers*, including a secret view key, into JUnit XML. Nothing here does. The values
//! asserted on screen - an amount and an address - are the ones this file chose and put into the request, and the
//! diagnostic dumps are of a screen that Speculos, and only Speculos, can be asked for.
//!
//! The hardware path is [`HumanApprover`], which is exercised by hand rather than by this file.
//!
//! # One at a time, and by two different mechanisms
//!
//! These tests share a device, and two scenarios pressing its buttons at once is not a clean failure - it is
//! gibberish that neither of them can recover from. Serialisation comes from two places, and it is worth being
//! precise about which covers what, because they do not overlap:
//!
//! * **Under `cargo test`**, libtest runs this binary's tests on parallel threads of **one process**, and the
//!   [`DEVICE`] mutex is what stops them. `--test-threads=1` would do it too; the mutex means the documented fallback
//!   command is safe whether or not anybody remembers the flag.
//! * **Under `cargo nextest`**, which is what `scripts/ledger_speculos.sh test` actually invokes, every test runs in
//!   its **own process**, so a process-local mutex protects nothing at all. What serialises that path is `test-threads
//!   = 1` in `[profile.ci]` of `.config/nextest.toml`, and nothing else.
//!
//! So the mutex is not a belt-and-braces backstop for the nextest path. Relaxing `test-threads = 1` would remove
//! the only thing holding that path together, and the symptom would be scenarios failing in ways that have nothing
//! to do with what they assert.

use std::{
    sync::{Mutex, MutexGuard},
    time::Duration,
};

use minotari_ledger_wallet_comms::{
    accessor_methods::{ledger_get_one_sided_metadata_signature, ledger_get_public_spend_key},
    error::LedgerDeviceError,
};
use minotari_ledger_wallet_comms_testing::{
    approver::{Approver, Outcome, SpeculosApprover, while_reviewing},
    review::{ExpectedReview, RECEIVER_FIELD},
    simulator,
    speculos_api::SpeculosApi,
};
use tari_common_types::{
    tari_address::{TariAddress, TariAddressFeatures},
    types::{ComAndPubSignature, PrivateKey},
};

/// Only one scenario may drive the device at a time - within **this process**.
///
/// That qualifier is the whole content of this comment. Under `cargo nextest` each test is its own process and
/// this mutex is uncontended and useless; the `ci` profile's `test-threads = 1` is what serialises there. See the
/// module docs.
static DEVICE: Mutex<()> = Mutex::new(());

/// Take the device for the rest of this test, in this process; see [`DEVICE`] for what that does and does not
/// cover.
///
/// A panicking scenario poisons this, and refusing to hand it out afterwards would turn one real failure into
/// "mutex poisoned" for every scenario after it - burying the failure that mattered. The device itself is left in
/// whatever state the panic left it, which is exactly what `expect_home` is for.
fn device() -> MutexGuard<'static, ()> {
    DEVICE.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The account to derive the sender offset key from. Any account will do; a constant keeps runs comparable.
const ACCOUNT: u64 = 0;
/// The sender offset key index. Same reasoning.
const SENDER_OFFSET_KEY_INDEX: u64 = 7;
/// The amount the review will show. Under a million, so the device renders it in microTari - see
/// `review::minotari_amount`, which has to make the same choice.
const VALUE: u64 = 12_345;
/// A payment ID length that is long enough to change the address's base58 completely and short enough to keep the
/// review to a handful of pages.
const PAYMENT_ID_LENGTH: usize = 32;

/// A published Tari dual address, taken from `comms/examples/ledger_demo/main.rs`.
///
/// It only has to be a well formed address whose public spend key is a real Ristretto point: the device parses it
/// after the review is approved and fails the instruction if it is not, which would turn every assertion here into
/// a confusing signing error rather than a screen mismatch.
const RECEIVER_BASE58: &str =
    "f48ScXDKxTU3nCQsQrXHs4tnkAyLViSUpi21t7YuBNsJE1VpqFcNSeEzQWgNeCqnpRaCA9xRZ3VuV11F8pHyciegbCt";

/// An approver for whatever simulator the environment names, with the ledger client pointed at it.
fn approver() -> SpeculosApprover {
    // The APDU transport and the control API are two different sockets on the same simulator, and both are needed:
    // the instruction goes down one and the buttons go down the other.
    simulator::connect();
    let approver = SpeculosApprover::from_env().expect("SPECULOS_API_ADDRESS / SPECULOS_MODEL");
    println!(
        "Speculos control API at '{}', driving a {}",
        approver.api().address(),
        approver.model()
    );
    approver
}

/// The receiver, with or without an embedded payment ID.
///
/// The payment ID variant is built from the published address's own keys rather than from fresh random ones, so
/// that the two scenarios differ in exactly the thing under test - whether a `Payment ID` row appears - and not
/// also in which keys are involved.
fn receiver(payment_id_length: usize) -> TariAddress {
    let published = TariAddress::from_base58(RECEIVER_BASE58).expect("the published address must parse");
    if payment_id_length == 0 {
        return published;
    }
    let view_key = published
        .public_view_key()
        .expect("a dual address has a view key")
        .clone();
    let spend_key = published.public_spend_key().clone();
    TariAddress::new_dual_address(
        view_key,
        spend_key,
        published.network(),
        // `new_dual_address` sets `PAYMENT_ID` itself when it is given payment ID bytes, but saying so here keeps
        // the intent of the scenario in the scenario.
        TariAddressFeatures::default() | TariAddressFeatures::PAYMENT_ID,
        Some(vec![0xAB; payment_id_length]),
    )
    .expect("a 32 byte payment ID is well within the limit")
}

/// Ask the device for a one sided metadata signature over `receiver`. This is the call that blocks on the review.
fn sign(receiver: &TariAddress) -> Result<ComAndPubSignature, LedgerDeviceError> {
    ledger_get_one_sided_metadata_signature(
        ACCOUNT,
        receiver.network(),
        0,
        VALUE,
        SENDER_OFFSET_KEY_INDEX,
        // A fixed commitment mask rather than a random one: nothing here depends on it being secret, and a
        // constant means two runs of the same scenario send the same bytes.
        &PrivateKey::from(42u64),
        receiver,
        &[7u8; 32],
    )
}

/// Acceptance: the review shows the amount and the receiver, and no `Payment ID` row, and the device signs.
#[test]
#[ignore = "needs a running Speculos simulator; see the module docs"]
fn the_review_shows_the_amount_and_receiver_and_no_payment_id() {
    let _device = device();
    let approver = approver();
    approver.expect_home().expect("the device should start at home");

    let receiver = receiver(0);
    let expected = ExpectedReview::one_sided_metadata_signature(VALUE, &receiver.to_base58(), 0);

    let (signature, review) = while_reviewing(&approver, &expected, Outcome::Approve, || sign(&receiver));
    review.unwrap_or_else(|e| panic!("{e}"));
    signature.expect("an approved review must produce a signature");

    approver
        .wait_for_home()
        .expect("the device should be back at home after signing");
}

/// Acceptance: with a payment ID, the `Payment ID` row appears and says how many bytes it carries.
///
/// Run against the same device, immediately after the scenario above, with no restart in between - which is the
/// whole point of the home assertion.
#[test]
#[ignore = "needs a running Speculos simulator; see the module docs"]
fn the_review_shows_a_payment_id_when_the_address_carries_one() {
    let _device = device();
    let approver = approver();
    approver.expect_home().expect("the device should start at home");

    let receiver = receiver(PAYMENT_ID_LENGTH);
    let expected = ExpectedReview::one_sided_metadata_signature(VALUE, &receiver.to_base58(), PAYMENT_ID_LENGTH);

    let (signature, review) = while_reviewing(&approver, &expected, Outcome::Approve, || sign(&receiver));
    review.unwrap_or_else(|e| panic!("{e}"));
    signature.expect("an approved review must produce a signature");

    approver.wait_for_home().expect("the device should be back at home");
}

/// Acceptance: an assertion that cannot fail is not an assertion.
///
/// Corrupt one character of the expected receiver and the scenario must fail - and fail *naming the receiver*,
/// rather than timing out or passing. The device is showing the right address throughout; it is the expectation
/// that is wrong, which is the same shape as the bug this whole spec exists to catch, with the two sides swapped.
///
/// The review is rejected on a mismatch rather than approved, so the instruction comes back `UserCancelled` and
/// the device is left at home for the next scenario. A failing assertion must not also break everything after it.
#[test]
#[ignore = "needs a running Speculos simulator; see the module docs"]
fn a_wrong_expected_receiver_fails_the_scenario_and_leaves_the_device_usable() {
    let _device = device();
    let approver = approver();
    approver.expect_home().expect("the device should start at home");

    let receiver = receiver(0);
    let correct = receiver.to_base58();
    let mut wrong = correct.clone();
    // Base58 has no `1` ambiguity to worry about and every address contains many characters, so changing the
    // eleventh to something else is guaranteed to produce a different, still plausible looking, address.
    wrong.replace_range(10..11, if correct.get(10..11) == Some("Z") { "Y" } else { "Z" });
    assert_ne!(wrong, correct, "the corrupted address must actually differ");

    let expected =
        ExpectedReview::one_sided_metadata_signature(VALUE, &correct, 0).with_field_value(RECEIVER_FIELD, &wrong);

    let (signature, review) = while_reviewing(&approver, &expected, Outcome::Approve, || sign(&receiver));

    let problem = review
        .expect_err("a wrong expected receiver must fail the scenario")
        .to_string();
    assert!(
        problem.contains("Receiver"),
        "the failure must name the field that disagreed, got:\n{problem}"
    );
    assert!(
        problem.contains(&wrong),
        "the failure must quote what was expected, got:\n{problem}"
    );

    match signature {
        Err(LedgerDeviceError::UserCancelled) => {},
        other => panic!("a mismatched review must be rejected, not approved; the instruction returned {other:?}"),
    }

    // And the device carries on. A harness that failed an assertion and wedged the device would make the second
    // failure in a run impossible to read.
    approver
        .wait_for_home()
        .expect("a rejected review must leave the device at home");
    ledger_get_public_spend_key(ACCOUNT).expect("the next instruction must still succeed");
}

/// Acceptance: rejecting returns `UserCancelled`, and the device is usable afterwards.
///
/// The second half is the half that matters. A device that returns the right error and then wedges is still a
/// broken device, and it was: on `stax` and `flex` a rejected review used to leave the rejection dialog on screen
/// for ever, because `show_status_and_home_if_needed` in `wallet/src/main.rs` only returned home for `Deny` and
/// `Ok`. The `expect_home` below is what fails if that regresses.
#[test]
#[ignore = "needs a running Speculos simulator; see the module docs"]
fn rejecting_the_review_returns_user_cancelled_and_the_device_still_works() {
    let _device = device();
    let approver = approver();
    approver.expect_home().expect("the device should start at home");

    let receiver = receiver(0);
    let expected = ExpectedReview::one_sided_metadata_signature(VALUE, &receiver.to_base58(), 0);

    let (signature, review) = while_reviewing(&approver, &expected, Outcome::Reject, || sign(&receiver));
    review.unwrap_or_else(|e| panic!("{e}"));

    match signature {
        Err(LedgerDeviceError::UserCancelled) => {},
        other => panic!("rejecting must reach the caller as LedgerDeviceError::UserCancelled, got {other:?}"),
    }

    approver
        .wait_for_home()
        .expect("a rejected review must leave the device at its home screen");

    // No nonce was reserved and no script offset context was retained - the review is the first thing the handler
    // does, before it asks for a nonce at all - so the next instruction must behave exactly as if the rejected one
    // had never been sent.
    ledger_get_public_spend_key(ACCOUNT).expect("the instruction after a rejection must succeed");
    let (signature, review) = while_reviewing(&approver, &expected, Outcome::Approve, || sign(&receiver));
    review.unwrap_or_else(|e| panic!("{e}"));
    signature.expect("the same instruction must succeed when it is approved");

    approver.wait_for_home().expect("the device should be back at home");
}

/// Acceptance: a forced timeout prints the full event log and the last screen.
///
/// A timeout with no diagnostics is the one failure mode most likely to make people give up on a suite, so this
/// asserts that the dump is real rather than an empty template: it names what was being waited for, it carries the
/// events the device drew during the scenario, and it carries the screen the device was actually on.
///
/// Forcing it deterministically takes a little care. The scenario runs a real review first, so that the event log
/// has something in it - the log is cleared once per scenario by `expect_home`, and nothing else clears it - and
/// then asks a second approver, with a two second deadline, to answer a review that nobody has started. That wait
/// can only end one way, and it ends there without a `sleep` or a retry anywhere in the harness.
#[test]
#[ignore = "needs a running Speculos simulator; see the module docs"]
fn a_forced_timeout_dumps_the_event_log_and_the_last_screen() {
    let _device = device();
    let approver = approver();
    approver.expect_home().expect("the device should start at home");

    let receiver = receiver(0);
    let expected = ExpectedReview::one_sided_metadata_signature(VALUE, &receiver.to_base58(), 0);
    let (signature, review) = while_reviewing(&approver, &expected, Outcome::Approve, || sign(&receiver));
    review.unwrap_or_else(|e| panic!("{e}"));
    signature.expect("an approved review must produce a signature");
    // `wait_for_home`, not `expect_home`: the latter starts a new scenario and so clears the event log that this
    // test is about to assert is not empty.
    approver.wait_for_home().expect("the device should be back at home");
    let impatient = SpeculosApprover::new(SpeculosApi::from_env().expect("SPECULOS_API_ADDRESS"), approver.model())
        .with_timeout(Duration::from_secs(2));

    let dump = impatient
        .expect_and_approve(&expected)
        .expect_err("there is no review in flight, so this can only time out")
        .to_string();
    println!("{dump}");

    assert!(dump.contains("Timed out"), "{dump}");
    assert!(dump.contains("the review to appear"), "{dump}");
    assert!(dump.contains("Full event log"), "{dump}");
    assert!(
        !dump.contains("Full event log (0 events)"),
        "the dump must carry the events the device drew during this scenario, not an empty log:\n{dump}"
    );
    assert!(dump.contains("Last screen"), "{dump}");
    assert!(
        dump.contains("MinoTari"),
        "the last screen dump must show what was really on the screen:\n{dump}"
    );
    assert!(
        dump.contains(".png"),
        "the dump must say where the last screen's picture was written:\n{dump}"
    );

    approver
        .wait_for_home()
        .expect("a timed out wait must not have disturbed the device");
}
