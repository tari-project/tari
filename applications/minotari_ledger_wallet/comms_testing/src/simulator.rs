// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Finding the running simulator, and deciding which column of the vector table applies to it.
//!
//! # How simulator tests are gated: `#[ignore]`, and nothing else
//!
//! Every test that needs a device is marked `#[ignore]`. Nothing in this module decides whether to run - it only
//! answers "where is it, and which seed is in it".
//!
//! That is a deliberate choice between two options, and the other one is a trap. Gating on "is the environment
//! variable set" means a machine with no simulator *passes*: the assertions never execute, the test reports green,
//! and a vector table that nobody has checked since the day it was written looks exactly like one that was checked
//! five minutes ago. Silence is the one answer a vector table must never be able to give.
//!
//! With `#[ignore]`:
//!
//! * a plain `cargo test` reports these as **ignored**, which `libtest` prints distinctly from "passed", so the local
//!   run is quiet without ever claiming the vectors were verified;
//! * `cargo test -- --ignored` and `cargo nextest run --run-ignored all` - what `scripts/ledger_speculos.sh` and CI use
//!   - run them for real, and there is no skip path left: if the simulator is missing, they **fail**.
//!
//! [`SPECULOS_APDU_ADDRESS`] and [`SPECULOS_SEED_ID`] are configuration, not gates.
//!
//! Unset, [`SPECULOS_APDU_ADDRESS`] is [`SPECULOS_DEFAULT_APDU_ADDRESS`]. That is Speculos' *conventional* port -
//! what you get from Ledger's own `docker run -p 9999:9999 speculos ...` - and it is deliberately **not** where
//! `scripts/ledger_speculos.sh` puts a simulator: that script lets Docker allocate an ephemeral host port, so two
//! runs cannot collide, and prints the address it got. Ask it rather than assuming:
//!
//! ```text
//! SPECULOS_APDU_ADDRESS=$(./scripts/ledger_speculos.sh address nanosplus default)
//! ```
//!
//! # Set but empty is always an error, never a default
//!
//! Both variables distinguish three cases, not two: **unset** means "use the default", a **value** means that
//! value, and **set but empty or whitespace** is a hard error. That third case is not pedantry, it is the shape
//! every realistic failure arrives in:
//!
//! ```text
//! SPECULOS_APDU_ADDRESS=$(./scripts/ledger_speculos.sh address nanosplus default) cargo test -- --ignored
//! ```
//!
//! If that simulator is not up, `address` writes to stderr and exits non-zero, the command substitution yields
//! `""`, and the environment variable is set to the empty string. Falling back to the default would then point the
//! whole suite at [`SPECULOS_DEFAULT_APDU_ADDRESS`] - `127.0.0.1:9999`, which is *Speculos' own documented default
//! `--apdu-port`*, and therefore the single most likely address for some unrelated simulator to be sitting on. If
//! one is, and it happens to run this application under the default seed, every assertion passes against a device
//! that is not the one under test. A green vector table that never contacted the device under test is precisely
//! the silence this crate exists to eliminate.
//!
//! The same argument applies to the seed - a silent fallback there would re-run the default-seed assertions while
//! reporting that the alternate seed was covered - so the two are handled identically rather than each on its own
//! merits.

use std::{
    env,
    sync::{Arc, OnceLock},
};

use minotari_ledger_wallet_comms::error::LedgerDeviceError;

use crate::{SPECULOS_DEFAULT_APDU_ADDRESS, SpeculosTransport, register_transport, seeds::SeedId};

/// Where the simulator's APDU socket is. Defaults to [`SPECULOS_DEFAULT_APDU_ADDRESS`].
pub const SPECULOS_APDU_ADDRESS: &str = "SPECULOS_APDU_ADDRESS";

/// Which seed the running simulator was started with: `default` or `alternate`. Defaults to `default`, which is
/// what Speculos uses when it is given no `--seed`.
pub const SPECULOS_SEED_ID: &str = "SPECULOS_SEED_ID";

/// The address to talk to.
///
/// Errors if [`SPECULOS_APDU_ADDRESS`] is set but empty; see the module docs for why that is not a default.
pub fn apdu_address() -> Result<String, LedgerDeviceError> {
    apdu_address_from(env::var(SPECULOS_APDU_ADDRESS).ok().as_deref())
}

/// Which seed the running simulator holds.
///
/// An unrecognised value is a hard error rather than a fall back to the default. Falling back would mean a typo in
/// the harness quietly re-runs the default-seed assertions twice and reports that the second seed was covered when
/// it was not - which is precisely the thing the second seed exists to rule out.
pub fn seed_id() -> Result<SeedId, LedgerDeviceError> {
    seed_id_from(env::var(SPECULOS_SEED_ID).ok().as_deref())
}

/// Which seed the harness *said* the device holds, if it said anything at all.
///
/// [`seed_id`] folds "unset" into the default, which is right for a simulator the harness started: the default seed
/// is what Speculos loads when it is given no `--seed`. It is wrong for the one caller that needs to tell "the
/// harness declared the default seed" apart from "nobody declared anything", which is
/// `scenarios::vectors::identify_seed`. That function works the seed out by asking the device, so it needs no
/// declaration - but when there *is* one it cross checks against it, because a harness that started the alternate
/// simulator and then declared the default one would otherwise sail through every vector scenario while the
/// model/seed matrix reported a seed it never ran.
///
/// A set-but-unrecognised value is still a hard error, for the reason [`seed_id`] gives.
pub fn seed_id_if_set() -> Result<Option<SeedId>, LedgerDeviceError> {
    seed_id_if_set_from(env::var(SPECULOS_SEED_ID).ok().as_deref())
}

// The three functions above are thin `env::var` wrappers on purpose, with all of the behaviour in the pure
// functions below, because that is what makes the behaviour testable **without touching the process environment**.
//
// `std::env::set_var` is `unsafe` in edition 2024 for a reason that is easy to talk yourself out of: the hazard is
// not "another test reads *this* variable", it is that `setenv` can reallocate `environ` while any thread is in
// `getenv` for *any* variable. `cargo test` runs this crate's tests on parallel threads, and one of them
// (`lib.rs`'s `a_panic_while_holding_the_lock_does_not_break_the_transport`) deliberately panics, which takes the
// panic hook through `std::env::var("RUST_BACKTRACE")`. A test that set environment variables here would be racing
// that on the documented local command. Splitting the parse out means there is no `unsafe` block in this crate at
// all.

/// The address to talk to, given the raw value of [`SPECULOS_APDU_ADDRESS`] if it was set.
///
/// Unset is the default; set but empty is an error. See the module docs - falling back here would aim the suite at
/// Speculos' own default port and could pass against an unrelated simulator.
fn apdu_address_from(value: Option<&str>) -> Result<String, LedgerDeviceError> {
    match value.map(str::trim) {
        None => Ok(SPECULOS_DEFAULT_APDU_ADDRESS.to_string()),
        Some(address) if !address.is_empty() => Ok(address.to_string()),
        Some(_) => Err(LedgerDeviceError::Processing(format!(
            "{SPECULOS_APDU_ADDRESS} is set but empty. It is usually `$(./scripts/ledger_speculos.sh address <model> \
             <seed>)` with no simulator running - start one with `./scripts/ledger_speculos.sh up <model> <seed>`. \
             Refusing to fall back to {SPECULOS_DEFAULT_APDU_ADDRESS}, which is Speculos' default port and may be \
             some other simulator entirely."
        ))),
    }
}

/// Which seed is loaded, given the raw value of [`SPECULOS_SEED_ID`] if it was set.
fn seed_id_from(value: Option<&str>) -> Result<SeedId, LedgerDeviceError> {
    match value {
        None => Ok(SeedId::SpeculosDefault),
        Some(name) => SeedId::from_name(name.trim()).ok_or_else(|| {
            LedgerDeviceError::Processing(format!(
                "{SPECULOS_SEED_ID} is '{name}', which is not 'default' or 'alternate'"
            ))
        }),
    }
}

/// Which seed was declared, given the raw value of [`SPECULOS_SEED_ID`] if it was set.
///
/// The three cases [`seed_id_from`] has, with the first one kept distinct instead of folded into the default.
fn seed_id_if_set_from(value: Option<&str>) -> Result<Option<SeedId>, LedgerDeviceError> {
    match value {
        None => Ok(None),
        Some(name) => seed_id_from(Some(name)).map(Some),
    }
}

/// The one transport this process uses, however many tests ask for it.
///
/// Speculos serves **one** APDU connection at a time. `cargo test` runs the test functions in a binary on parallel
/// threads, so a `connect()` that opened a fresh connection per caller would have the second test's connection
/// refused, or - worse - accepted only once the first test's had closed, turning a deterministic suite into a race.
/// One shared transport instead: its internal mutex serialises the exchanges, which is exactly the serialisation
/// the simulator needs anyway.
static TRANSPORT: OnceLock<Arc<SpeculosTransport>> = OnceLock::new();

/// Connect to the running simulator and point the ledger client at it.
///
/// Idempotent: the first caller connects and registers, every later caller gets the same transport back. Returns it
/// so that a caller wanting to send raw APDUs can, without going through the accessor methods and the application
/// verification they each perform.
///
/// # Panics
///
/// Panics, with the address and the underlying error, if there is no simulator listening. This is a test-only
/// helper and a missing simulator is a failure, not a skip - see the module docs.
pub fn connect() -> Arc<SpeculosTransport> {
    TRANSPORT
        .get_or_init(|| {
            // A set-but-empty address is its own failure, and a different one from "nothing is listening there".
            // Reported on its own so the message names the real problem rather than blaming the default port.
            let address = apdu_address().unwrap_or_else(|e| panic!("{e}"));
            // Logged unconditionally, on the *successful* path too. A run that points at the wrong simulator and
            // passes is the failure mode that leaves no trace anywhere else: the assertions are green, so nothing
            // prints, and the address that produced them is nowhere in the output.
            //
            // `println!` rather than `debug!` because a test binary's logger is usually not initialised.
            //
            // Surviving a *passing* run takes a matching pair of settings in `.config/nextest.toml`, because
            // nextest discards the output of tests that pass by default: `success-output` for the console and
            // `store-success-output` for the JUnit `<system-out>`. Without those this line still exists and still
            // does nothing useful, so do not remove them thinking they are cosmetic.
            println!("Speculos transport connecting to '{address}'");
            let transport = Arc::new(SpeculosTransport::connect(address.clone()).unwrap_or_else(|e| {
                panic!(
                    "No Speculos simulator at '{address}': {e}\nStart one with:  ./scripts/ledger_speculos.sh up\nor \
                     point {SPECULOS_APDU_ADDRESS} at an existing one."
                )
            }));
            register_transport(transport.clone());
            transport
        })
        .clone()
}

#[cfg(test)]
mod test {
    use super::*;

    /// An unset variable means the default the script starts a simulator on.
    #[test]
    fn an_unset_address_is_the_default() {
        assert_eq!(apdu_address_from(None).unwrap(), SPECULOS_DEFAULT_APDU_ADDRESS);
    }

    #[test]
    fn an_address_is_taken_as_given() {
        assert_eq!(apdu_address_from(Some("10.0.0.1:1234")).unwrap(), "10.0.0.1:1234");
        // Docker's `docker port` output can arrive with a trailing newline through a shell.
        assert_eq!(
            apdu_address_from(Some(" 127.0.0.1:55011\n")).unwrap(),
            "127.0.0.1:55011"
        );
    }

    /// Set but empty must be an error, **not** the default.
    ///
    /// This is what `SPECULOS_APDU_ADDRESS=$(... address ...)` leaves behind when the simulator is not up. The
    /// default it would otherwise fall back to is `127.0.0.1:9999` - Speculos' own default `--apdu-port` - so a
    /// fallback could quietly run the whole vector table against an unrelated simulator and pass. See the module
    /// docs; `seed_id_from` applies the same rule for the same reason.
    #[test]
    fn a_set_but_empty_address_is_an_error() {
        for value in ["", " ", "\n", "  \t\n "] {
            let error = apdu_address_from(Some(value))
                .map(|address| address.to_string())
                .expect_err("an empty address must not silently become the default");
            let message = error.to_string();
            assert!(
                message.contains(SPECULOS_APDU_ADDRESS),
                "the error should name the variable, got: {message}"
            );
            assert!(
                message.contains(SPECULOS_DEFAULT_APDU_ADDRESS),
                "the error should say which default it is refusing, got: {message}"
            );
        }
    }

    /// The two variables follow one policy: unset is a default, set-but-empty is an error. Asserted together so
    /// that a change to one of them cannot quietly diverge from the other.
    #[test]
    fn both_variables_treat_set_but_empty_as_an_error() {
        assert!(apdu_address_from(None).is_ok());
        assert!(seed_id_from(None).is_ok());
        assert!(apdu_address_from(Some("")).is_err());
        assert!(seed_id_from(Some("")).is_err());
        assert!(apdu_address_from(Some("   ")).is_err());
        assert!(seed_id_from(Some("   ")).is_err());
    }

    #[test]
    fn an_unset_seed_id_is_the_speculos_default() {
        assert_eq!(seed_id_from(None).unwrap(), SeedId::SpeculosDefault);
    }

    #[test]
    fn both_seed_names_are_recognised() {
        assert_eq!(seed_id_from(Some("default")).unwrap(), SeedId::SpeculosDefault);
        assert_eq!(seed_id_from(Some("alternate")).unwrap(), SeedId::Alternate);
        assert_eq!(seed_id_from(Some(" alternate\n")).unwrap(), SeedId::Alternate);
    }

    /// A typo must be loud, not a silent fall back to the default seed - otherwise the alternate-seed half of the
    /// matrix quietly re-runs the default-seed assertions and reports that the second seed was covered.
    #[test]
    fn an_unrecognised_seed_id_is_an_error() {
        assert!(seed_id_from(Some("alternative")).is_err());
        assert!(seed_id_from(Some("")).is_err());
        assert!(seed_id_from(Some("DEFAULT")).is_err());
    }

    /// The "was anything declared at all" reading keeps unset distinct from the default, while treating every other
    /// case exactly as `seed_id_from` does. A version that quietly reported `Some(SpeculosDefault)` for an unset
    /// variable would turn the vector scenarios' cross check into an assertion that every hardware device holds the
    /// default seed.
    #[test]
    fn an_unset_seed_id_declares_nothing() {
        assert_eq!(seed_id_if_set_from(None).unwrap(), None);
        assert_eq!(
            seed_id_if_set_from(Some("default")).unwrap(),
            Some(SeedId::SpeculosDefault)
        );
        assert_eq!(
            seed_id_if_set_from(Some(" alternate\n")).unwrap(),
            Some(SeedId::Alternate)
        );
        assert!(seed_id_if_set_from(Some("")).is_err());
        assert!(seed_id_if_set_from(Some("alternative")).is_err());
    }
}
