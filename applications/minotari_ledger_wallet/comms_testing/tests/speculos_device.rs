// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! The assertions that need a running device.
//!
//! # Running these
//!
//! Every test in this file is `#[ignore]`d, so a plain `cargo test` reports them as *ignored* rather than passing
//! them without a device. See `simulator`'s module docs for why that is the gate and not an environment variable.
//! To run them, bring a simulator up and pass `--ignored`; `scripts/ledger_speculos.sh` does the whole thing:
//!
//! ```text
//! ./scripts/ledger_speculos.sh test                 # build, run every model and seed, tear down
//! ./scripts/ledger_speculos.sh up                   # or leave one running and iterate against it
//! cargo test --manifest-path applications/minotari_ledger_wallet/comms_testing/Cargo.toml -- --ignored
//! ```
//!
//! # What is asserted where
//!
//! These tests bind the device to the table. They do **not** check that the table is internally sensible or that
//! the oracle agrees with it - those are unit tests in `vectors` and `oracle`, they need no device, and they run on
//! every `cargo test`. Splitting it that way means a table typo is caught by a fast local run, and only "does this
//! device agree" needs Docker.

use std::sync::OnceLock;

use minotari_ledger_wallet_comms::accessor_methods::{
    ledger_get_app_name,
    ledger_get_public_key,
    ledger_get_public_spend_key,
    ledger_get_version,
    ledger_get_view_key,
};
use minotari_ledger_wallet_comms_testing::{
    seeds::SeedId,
    simulator,
    vectors::{ACCOUNT_WRAP_VECTOR, DERIVATION_VECTORS, DerivationVector, DeviceCall, EXPECTED_VECTOR_COUNT},
};
use tari_utilities::{ByteArray, hex::Hex};

/// Fail if the table is not the size it is supposed to be.
///
/// Every device test below is a `for` loop over `DERIVATION_VECTORS`, and a `for` loop over an empty slice
/// succeeds. Without this, a truncated table would let the whole device suite report green against a live
/// simulator having asserted nothing whatsoever - the exact silence the table exists to prevent, arriving in the
/// one place nobody would think to look for it. `vectors::test::the_table_has_the_expected_number_of_rows` makes
/// the same check without a device; this one guarantees that the *device* assertions are never vacuous, even if
/// somebody runs this binary on its own.
fn assert_table_is_populated() {
    assert_eq!(
        DERIVATION_VECTORS.len(),
        EXPECTED_VECTOR_COUNT,
        "the vector table has {} rows, expected {}; the device assertions below would be vacuous",
        DERIVATION_VECTORS.len(),
        EXPECTED_VECTOR_COUNT
    );
}

/// Refuse to transcribe a device's answers anywhere until it has proved it is a published test device.
///
/// Every assertion below prints the device's raw answer on failure - `assert_eq!` and `assert_ne!` both dump their
/// operands - and `scripts/ledger_speculos.sh` copies the resulting JUnit XML into an artifact directory that CI is
/// told to upload. One of the table's instructions, `GetViewKey`, returns a **secret** scalar.
///
/// Under the two published seeds that is harmless: both mnemonics are in this repository, so nothing about their
/// keys is secret. The problem is that nothing constrains the harness to those seeds. `SPECULOS_APDU_ADDRESS` is
/// free-form and will happily point at anything speaking APDU over TCP - a Speculos somebody loaded with a real
/// recovery phrase to reproduce a bug, or a real device behind a TCP-to-HID bridge. Aimed there, a single
/// mismatching row would write live key material into a file built to be uploaded.
///
/// So this is a precondition, not redaction: ask the device one canonical question whose answer is already in the
/// table, and refuse to go on unless it answers with one of the two published seeds' values. A device that fails
/// this check has its answers withheld entirely - the failure message deliberately contains no device output.
/// Reusing the table for this costs one extra APDU per process and no new trusted data.
fn assert_device_is_a_published_test_device() {
    static CHECKED: OnceLock<()> = OnceLock::new();
    CHECKED.get_or_init(|| {
        // Register the Speculos transport first. Without it `Command::execute` falls through to the HID path and
        // the probe fails with "Ledger device not found" rather than telling you anything about the device.
        // `connect()` is idempotent, so the tests below may - and do - call it again.
        simulator::connect();
        assert_table_is_populated();
        // A public-key row on purpose: the check itself must not be the thing that extracts a secret.
        let canonical = DERIVATION_VECTORS
            .iter()
            .find(|v| matches!(v.call, DeviceCall::PublicKey { .. }))
            .expect("the table must contain at least one GetPublicKey vector to probe with");

        let actual = ask_device(canonical);
        assert!(
            SeedId::ALL.iter().any(|seed| canonical.expected(*seed) == actual),
            "The device at {} did not answer '{}' with either published test seed's value, so this harness will not \
             print or record anything it returns.\n\nThis suite is only for a Speculos simulator loaded with one of \
             the two seeds in `seeds.rs`. It transcribes device answers - including the GetViewKey secret scalar - \
             into assertion messages and into JUnit XML that CI uploads, so it refuses to run against a device it \
             cannot identify as a test device. If that is a real device or a simulator holding a real recovery \
             phrase, nothing above was printed and nothing was written.\n\nPoint SPECULOS_APDU_ADDRESS at a simulator \
             started by ./scripts/ledger_speculos.sh.",
            simulator::apdu_address().unwrap_or_else(|_| "<unset>".to_string()),
            canonical.name
        );
    });
}

/// Ask the device for whatever `vector` describes, and return the 32 bytes it answered with.
fn ask_device(vector: &DerivationVector) -> String {
    let bytes = match vector.call {
        DeviceCall::PublicKey { index, branch } => ledger_get_public_key(vector.account, index, branch)
            .unwrap_or_else(|e| panic!("GetPublicKey failed for '{}': {e}", vector.name))
            .as_bytes()
            .to_vec(),
        DeviceCall::PublicSpendKey => ledger_get_public_spend_key(vector.account)
            .unwrap_or_else(|e| panic!("GetPublicSpendKey failed for '{}': {e}", vector.name))
            .as_bytes()
            .to_vec(),
        DeviceCall::ViewKey => ledger_get_view_key(vector.account)
            .unwrap_or_else(|e| panic!("GetViewKey failed for '{}': {e}", vector.name))
            .as_bytes()
            .to_vec(),
    };
    bytes.to_hex()
}

/// Acceptance: the Spec 1 transport reaches the simulator and completes a `GetVersion` exchange.
///
/// `ledger_get_version` is not a bare APDU - every accessor method runs `verify_ledger_application` first, which
/// checks the application name, enforces `MIN_LEDGER_APP_VERSION`, and makes the device sign a random challenge and
/// verifies the signature against a public key it also asks for. Reaching a version string therefore means the
/// whole host-to-device path works, not just that something answered.
#[test]
#[ignore = "needs a running Speculos simulator; see the module docs"]
fn the_transport_completes_a_get_version_exchange() {
    simulator::connect();

    let name = ledger_get_app_name().expect("GetAppName");
    assert_eq!(name, "minotari_ledger_wallet");

    let version = ledger_get_version().expect("GetVersion");
    // Not pinned to an exact string: the application version moves every release, and a test that had to be edited
    // on each version bump would be edited without being thought about. What matters is that a version came back
    // and that `verify_ledger_application` was willing to accept it.
    assert!(
        semver::Version::parse(&version).is_ok(),
        "'{version}' is not a semantic version"
    );
}

/// Acceptance: the shared vector table passes against whatever model is running, unmodified.
///
/// The same table, the same assertions, for `nanosplus` and for `stax`. A `stax` failure here means the device
/// application derives different keys on different models - a user who moves their recovery phrase between Ledger
/// models would find a different wallet - and it is fixed in the device, never by forking the table.
#[test]
#[ignore = "needs a running Speculos simulator; see the module docs"]
fn the_vector_table_matches_the_device() {
    assert_device_is_a_published_test_device();
    simulator::connect();
    let seed = simulator::seed_id().expect("SPECULOS_SEED_ID");

    let mut mismatches = Vec::new();
    for vector in DERIVATION_VECTORS {
        let actual = ask_device(vector);
        if actual != vector.expected(seed) {
            mismatches.push(format!(
                "  {}\n    expected {}\n    device   {}",
                vector.name,
                vector.expected(seed),
                actual
            ));
        }
    }

    // Every row is checked before failing. A vector table that stops at the first mismatch tells you one row moved;
    // one that reports all of them tells you whether the derivation changed or a single row was mistyped, which is
    // the difference between a five minute and a five hour diagnosis.
    assert!(
        mismatches.is_empty(),
        "{} of {} vectors disagree with the device under the {} seed:\n{}",
        mismatches.len(),
        DERIVATION_VECTORS.len(),
        seed.name(),
        mismatches.join("\n")
    );
}

/// Acceptance: swapping the seed changes **every** vector, as observed on the device.
///
/// `vectors::test::the_second_seed_changes_every_vector` already asserts the two columns of the table differ. That
/// is a statement about the table. This is the statement about the *device*: whichever seed it was started with, it
/// must not be answering with the other seed's values. Run under both seeds - which `scripts/ledger_speculos.sh`
/// does - the pair rules out a device that ignores its seed and returns constants.
#[test]
#[ignore = "needs a running Speculos simulator; see the module docs"]
fn the_device_does_not_answer_with_the_other_seeds_values() {
    assert_device_is_a_published_test_device();
    simulator::connect();
    let seed = simulator::seed_id().expect("SPECULOS_SEED_ID");
    let other = match seed {
        SeedId::SpeculosDefault => SeedId::Alternate,
        SeedId::Alternate => SeedId::SpeculosDefault,
    };

    for vector in DERIVATION_VECTORS {
        let actual = ask_device(vector);
        assert_ne!(
            actual,
            vector.expected(other),
            "the simulator is loaded with the {} seed but answered '{}' with the {} seed's value - either the harness \
             started it with the wrong seed, or the device is not deriving from the seed at all",
            seed.name(),
            vector.name,
            other.name()
        );
    }
}

/// The account path element wraps at 2^32, because `make_bip32_path` accumulates it into a `u32`.
///
/// Kept separate from the table because it asserts a *relationship* rather than a value: two accounts 2^32 apart
/// address the same key. See `vectors::ACCOUNT_WRAP_VECTOR` for why this sharp edge is worth pinning rather than
/// quietly tolerating.
#[test]
#[ignore = "needs a running Speculos simulator; see the module docs"]
fn the_account_wraps_at_u32() {
    assert_device_is_a_published_test_device();
    simulator::connect();
    let (low, wrapping, branch) = ACCOUNT_WRAP_VECTOR;

    let from_low = ledger_get_public_key(low, 0, branch).expect("GetPublicKey for the low account");
    let from_wrapping = ledger_get_public_key(wrapping, 0, branch).expect("GetPublicKey for the wrapping account");
    assert_eq!(
        from_low, from_wrapping,
        "account {low} and account {wrapping} no longer collide - `make_bip32_path` has stopped wrapping, which \
         changes which key every large account addresses"
    );
}

/// The device answers the same value twice for the same inputs.
///
/// Derivation is meant to be a pure function of (seed, account, index, key type). If it were not - if some device
/// state or the RNG leaked into it - a wallet would not be able to find its own outputs after a restart. Cheap to
/// assert, and not implied by any single-shot vector comparison.
#[test]
#[ignore = "needs a running Speculos simulator; see the module docs"]
fn derivation_is_deterministic_across_calls() {
    assert_device_is_a_published_test_device();
    simulator::connect();

    for vector in DERIVATION_VECTORS {
        let first = ask_device(vector);
        let second = ask_device(vector);
        assert_eq!(first, second, "'{}' is not deterministic on the device", vector.name);
    }
}
