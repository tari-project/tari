// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! The simulator frontend: every scenario in [`minotari_ledger_wallet_comms_testing::scenarios`], run unattended.
//!
//! This file contains **no assertions of its own**. Every one of them is in the scenario library, which
//! `examples/ledger_demo.rs` runs against real hardware with exactly the same expectations. A check that lived here
//! would be one the hardware path silently did not make.
//!
//! # Running these
//!
//! Every test here is `#[ignore]`d, and for all but one of them that is the "needs a device" gate and nothing else -
//! see `simulator`'s module docs for why an environment variable would be the wrong one. A plain `cargo test`
//! reports them as *ignored*, which `libtest` prints distinctly from "passed", so a machine with no simulator is
//! quiet without ever claiming the device was checked. `scripts/ledger_speculos.sh test` runs
//! `--run-ignored all`, so the whole suite runs on every pull request.
//!
//! The one exception is [`a_wrong_length_payload_does_not_block_on_a_button_press`], which documents a device bug
//! this suite found and deliberately did not fix. It is excluded by name in `ledger_speculos.sh` - by name, so
//! that the exclusion cannot quietly grow into a list - and is the only thing in this crate that does not run on a
//! merge. Read its doc comment before touching either place.
//!
//! ```text
//! ./scripts/ledger_speculos.sh test                          # build, run every model and seed, tear down
//!
//! ./scripts/ledger_speculos.sh up stax default               # or iterate against one
//! SPECULOS_APDU_ADDRESS=$(./scripts/ledger_speculos.sh address stax default) \
//! SPECULOS_API_ADDRESS=$(./scripts/ledger_speculos.sh api-address stax default) \
//! SPECULOS_MODEL=stax SPECULOS_SEED_ID=default \
//!   cargo test --manifest-path applications/minotari_ledger_wallet/comms_testing/Cargo.toml \
//!     --test speculos_scenarios -- --ignored --test-threads=1 \
//!       --skip a_wrong_length_payload_does_not_block_on_a_button_press
//! ```
//!
//! # One test per scenario *module*, not per scenario
//!
//! `cargo nextest` runs every `#[test]` in its own process, and the device state these scenarios exist to exercise -
//! the ephemeral nonce store and the script offset context - lives for as long as the application does. Scenarios
//! that have to observe one another's leftovers therefore have to share a process, which means sharing a test.
//!
//! The grouping is not free-form: `the_test_functions_cover_every_module` fails if a module is added without a test
//! function here.
//!
//! # One at a time, and by two different mechanisms
//!
//! These tests share a device, and two of them driving it at once is not a clean failure - it is gibberish that
//! neither can recover from. Serialisation comes from two places, and they do not overlap:
//!
//! * **Under `cargo test`**, libtest runs this binary's tests on parallel threads of **one process**, and the
//!   [`DEVICE`] mutex is what stops them.
//! * **Under `cargo nextest`**, which is what `scripts/ledger_speculos.sh test` invokes, every test runs in its **own
//!   process**, so a process-local mutex protects nothing. What serialises that path is `test-threads = 1` in
//!   `[profile.ci]` of `.config/nextest.toml`, and nothing else.

use std::sync::{Mutex, MutexGuard};

use minotari_ledger_wallet_common::common_types::{AppSW, Instruction, LedgerKeyBranch};
use minotari_ledger_wallet_comms_testing::{
    approver::SpeculosApprover,
    raw::{self, payload},
    review::UiToolkit,
    scenarios::{
        MODULES,
        ScenarioContext,
        ScenarioModule,
        crypto,
        handshake,
        legacy_nonce,
        protocol,
        stateful,
        vectors,
    },
    simulator,
    speculos_api::Button,
};

/// Only one scenario may drive the device at a time - within **this process**. See the module docs for what that
/// does and does not cover.
static DEVICE: Mutex<()> = Mutex::new(());

/// A panicking scenario poisons this, and refusing to hand it out afterwards would turn one real failure into
/// "mutex poisoned" for every test after it, burying the failure that mattered.
fn device() -> MutexGuard<'static, ()> {
    DEVICE.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Run every scenario in `module`, reporting all of their failures rather than only the first.
///
/// # Every scenario starts and ends at the home screen
///
/// `expect_home` before each one and `wait_for_home` after it. The first is what replaces restarting the simulator
/// between scenarios - a restart would be the easy way to get a known starting state and would also delete the only
/// state worth testing - and it clears the diagnostic event log so that a failure dumps what the device drew during
/// *that* scenario. The second catches a scenario that left the device somewhere unexpected, and blames the
/// scenario that did it rather than the next one along.
///
/// # A failing scenario does not stop the module
///
/// Each one is independent, and a run that reports "the nonce store scenario failed" and stops leaves you
/// wondering about the four after it. The mutex is held for the whole module either way, so nothing else is
/// touching the device in between.
fn run_module(module: &ScenarioModule) {
    let _device = device();
    // The APDU transport and the control API are two different sockets on the same simulator, and both are needed:
    // the instructions go down one and the buttons down the other.
    simulator::connect();
    let approver = SpeculosApprover::from_env().expect("SPECULOS_API_ADDRESS / SPECULOS_MODEL");
    println!(
        "Running the '{}' scenarios against a {} at '{}'",
        module.name,
        approver.model(),
        approver.api().address()
    );
    let context = ScenarioContext::new(&approver);

    let mut failures = Vec::new();
    for scenario in module.scenarios {
        if let Err(e) = approver.expect_home() {
            failures.push(format!("  before '{}': {e}", scenario.name));
            continue;
        }
        if let Err(e) = (scenario.run)(&context) {
            failures.push(format!("  '{}': {e}", scenario.name));
        }
        if let Err(e) = approver.wait_for_home() {
            failures.push(format!(
                "  after '{}', the device was not left at home: {e}",
                scenario.name
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} '{}' scenarios failed:\n{}",
        failures.len(),
        module.scenarios.len(),
        module.name,
        failures.join("\n")
    );
}

/// `verify_ledger_application` end to end, and the version floor the host enforces.
///
/// Runs first in a `cargo test` run by virtue of its name, and in its own process under nextest either way. Both
/// matter: it is what verifies the application for the scenarios that deliberately do not, and its concurrency
/// assertion is only meaningful while `verify_ledger_application`'s process wide cache is cold.
#[test]
#[ignore = "needs a running Speculos simulator; see the module docs"]
fn a_handshake_scenarios() {
    run_module(&handshake::MODULE);
}

/// The key derivation vector table, against this model and this seed.
#[test]
#[ignore = "needs a running Speculos simulator; see the module docs"]
fn b_vector_scenarios() {
    run_module(&vectors::MODULE);
}

/// Signatures, script offsets and shared secrets, verified as mathematics. Includes the one scenario in the suite
/// that raises a review, which `SpeculosApprover` answers.
#[test]
#[ignore = "needs a running Speculos simulator; see the module docs"]
fn c_crypto_scenarios() {
    run_module(&crypto::MODULE);
}

/// Malformed APDUs.
#[test]
#[ignore = "needs a running Speculos simulator; see the module docs"]
fn d_protocol_scenarios() {
    run_module(&protocol::MODULE);
}

/// The script offset context and the ephemeral nonce store, which only exist between exchanges.
#[test]
#[ignore = "needs a running Speculos simulator; see the module docs"]
fn e_stateful_scenarios() {
    run_module(&stateful::MODULE);
}

/// The legacy nonce branch whitelist, enforced by the device rather than mirrored by the host.
#[test]
#[ignore = "needs a running Speculos simulator; see the module docs"]
fn f_legacy_nonce_scenarios() {
    run_module(&legacy_nonce::MODULE);
}

/// Every scenario module has a test function here.
///
/// Host only, so it runs on every `cargo test`. Without it, adding a module to `scenarios::MODULES` and forgetting
/// to add a function above would mean the whole module simply never ran - green, silent, and invisible in a report
/// that lists six passing tests exactly as it did before.
#[test]
fn the_test_functions_cover_every_module() {
    // Kept in step by hand, which is what the assertion below is for.
    let covered = [
        handshake::MODULE.name,
        vectors::MODULE.name,
        crypto::MODULE.name,
        protocol::MODULE.name,
        stateful::MODULE.name,
        legacy_nonce::MODULE.name,
    ];
    let declared: Vec<&str> = MODULES.iter().map(|module| module.name).collect();

    for module in &declared {
        assert!(
            covered.contains(module),
            "the '{module}' scenario module has no test function in tests/speculos_scenarios.rs, so none of its \
             scenarios run against the simulator. Add one, and add it to `covered` above."
        );
    }
    assert_eq!(
        covered.len(),
        declared.len(),
        "tests/speculos_scenarios.rs lists {covered:?} but scenarios::MODULES is {declared:?}"
    );
}

/// **A device bug this suite found, deliberately not fixed here.**
///
/// On BAGL models (`nanosplus`, `nanox`) the device application reports errors through blocking UI. Nine handlers
/// answer a wrong payload length like this:
///
/// ```text
/// SingleMessage::new("Invalid data length").show_and_wait();
/// return Err(AppSW::WrongApduLength);
/// ```
///
/// `ledger_device_sdk::ui::gadgets::SingleMessage::show_and_wait` is a loop on `get_event` that returns only on a
/// button release, and `MessageScroller::event_loop` is the same. The status word is correct and does arrive -
/// **after somebody presses a button on the device**. So a single malformed APDU leaves the device showing a modal
/// that only a person can dismiss, with the host's exchange outstanding the whole time and no way to cancel it from
/// that side. A host does not have to be malicious to send one; a truncated write does it, and `ledger` is a
/// default feature of the console wallet.
///
/// There are 26 such call sites across nine files - `grep -rn 'show_and_wait()\|\.event_loop()' wallet/src`. Three
/// groups of them are reachable from the host, and this probe demonstrates the first:
///
/// * the nine handlers' payload length checks - `get_public_key`, `get_public_spend_key`, `get_view_key`,
///   `get_dh_shared_secret`, `get_ephemeral_nonce`, both `get_script_signature` entry points and all three in
///   `get_schnorr_signature`;
/// * `utils::get_key_from_canonical_bytes`, reached by any non-canonical key or scalar in a payload, which is why
///   `AppSW::KeyDeriveFromCanonical` has no scenario;
/// * `get_one_sided_metadata_signature`'s address parse, reached by a bad address checksum, which is why
///   `AppSW::MetadataSignatureFail` has no scenario.
///
/// `get_script_offset` and `get_one_sided_metadata_signature`'s length checks return `WrongApduLength` with no UI
/// at all. That is the behaviour the rest should have, and it is why
/// `scenarios::protocol::a_wrong_length_payload_is_refused` can run unattended at all.
///
/// # Measured, not inferred - and **both** toolkits are affected
///
/// Against the Speculos image pinned in `scripts/ledger_speculos.sh`. The two halves fail differently, and the
/// NBGL half is the one that is easy to get wrong by reading the source:
///
/// * **`nanosplus` (BAGL): the test fails.** The device never answers. The exchange sits for the transport's full 120
///   second `DEFAULT_READ_TIMEOUT` and then reports "The device did not answer within the read timeout". The device is
///   left on the modal, which a button press from the host can at least clear.
/// * **`stax` (NBGL): the test passes in about three seconds, and wedges the run anyway.** `NbglStatus::show` draws and
///   returns, so the status word comes straight back and the next instruction succeeds - both of this test's assertions
///   hold. But the "Invalid data length" status screen stays up, and **nothing puts the device back at its home
///   screen**: `show_status_and_home_if_needed` in `wallet/src/main.rs` calls `home.show_and_return()` only for
///   `GetOneSidedMetadataSignature`. Measured consequence: the next module's scenarios all fail in `expect_home` - 3 of
///   3 vector scenarios, with "The device is not at its home screen, it is showing \"Invalid data length\"".
///
/// So "it passes on stax" is true of the test and false of the run. That is why `ledger_speculos.sh` excludes it
/// on **every** model rather than only on BAGL, and why [`dismiss_any_modal`] does not try to tidy up on NBGL:
/// there is no host side action that restores that home screen.
///
/// It also widens what the fix has to cover. Making the BAGL handlers non-blocking is necessary and is not
/// sufficient; the NBGL side additionally has to return home after a failed instruction, which is the same gap
/// that `UserCancelled` was already added to `show_status_and_home_if_needed` to close.
///
/// # Why this is `#[ignore]`d rather than in the scenario library
///
/// Three reasons, and the last is the important one.
///
/// 1. It cannot run unattended on either toolkit - on BAGL it never returns, on NBGL it leaves the device somewhere
///    every following scenario refuses to start from.
/// 2. It is toolkit dependent, and a scenario is not: the suite runs one scenario library against every model, on
///    purpose, because "the models agree" is itself one of the things being asserted. A scenario that had to know which
///    toolkit it was talking to would be the first crack in that.
/// 3. Changing device behaviour is out of scope for the change that introduces this suite. A pull request that both
///    altered the device and added the tests that describe the device is one where neither half can be reviewed: every
///    failing assertion has two candidate explanations. The fix is a separate change, and this test is the thing that
///    says exactly what the fix has to make true.
///
/// **This is the only `#[ignore]` in this crate that is not the "needs a simulator" gate**, and the only thing in
/// it that does not run on a merge. When the device stops wedging itself on both toolkits, delete this test and
/// move the probe into `scenarios::protocol`, where it belongs, aimed at all eleven handlers.
#[test]
#[ignore = "device bug: on BAGL models a wrong payload length blocks on a button press; see this test's doc comment"]
fn a_wrong_length_payload_does_not_block_on_a_button_press() {
    let _device = device();
    simulator::connect();

    // A `GetPublicKey` that is one byte short. Everything else about it is valid.
    let request = raw::command(
        1,
        Instruction::GetPublicKey,
        payload::public_key(2, LedgerKeyBranch::Random),
    );
    let short = request.clone().with_data_length(request.data.len().saturating_sub(1));

    let reply = short.send();

    // Clean up before asserting, not after.
    //
    // On BAGL this call has just timed out with the device sitting on a modal that only a button press dismisses,
    // and nothing else in the suite can dismiss it. Left there, the failure this test is *supposed* to report
    // becomes a wall of unrelated ones: every later module fails in `approver.expect_home()` against a device
    // showing "Invalid data length". Pressing the button here contains the damage to the one test that found it.
    //
    // Deliberately best-effort and deliberately before the assertions, because an `assert!` unwinds and would skip
    // it. On NBGL there is nothing to dismiss and the press lands on the home screen, which is harmless.
    dismiss_any_modal();

    let reply = reply.expect("the device must answer a wrong length APDU without waiting for a human");
    assert_eq!(
        reply.status,
        AppSW::WrongApduLength as u16,
        "expected WrongApduLength, got {}",
        reply.describe_status()
    );

    // And the device is immediately usable, rather than stuck behind a modal.
    let next = request.send().expect("the instruction after a wrong length one");
    assert!(next.is_ok(), "the device did not recover: {}", next.describe_status());
}

/// Press whatever dismisses a modal, and wait for the home screen. Best effort; every failure is swallowed.
///
/// Only used by the test above, which is the only thing in this crate that can leave a device wedged. It is a
/// Speculos-only action by nature - a real device needs a thumb - which is another reason that test is not a
/// scenario.
///
/// **BAGL only, and that is a limitation rather than an optimisation.** A button press clears the BAGL modal and
/// the device comes home. NBGL has no equivalent: the status screen is cleared by the next NBGL draw, and the only
/// thing in the application that draws one is the `GetOneSidedMetadataSignature` review. So on NBGL this does
/// nothing, the device stays off its home screen, and the test's exclusion from `ledger_speculos.sh` is what
/// prevents the cascade instead. See the test's doc comment.
fn dismiss_any_modal() {
    let Ok(approver) = SpeculosApprover::from_env() else {
        return;
    };
    match approver.model().toolkit() {
        UiToolkit::Bagl => {
            let _pressed = approver.api().press_button(Button::Both);
            let _home = approver.wait_for_home();
        },
        // NBGL cannot be cleaned up from the host at all - see the test's doc comment. Waiting for home here would
        // just burn the approver's full deadline before the assertions ran.
        UiToolkit::Nbgl => {},
    }
}
