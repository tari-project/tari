// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! `verify_ledger_application`, end to end against a device, plus the version floor that gates it.
//!
//! # This module runs first, and that is load bearing
//!
//! Two reasons, and only the second is obvious.
//!
//! 1. Every other scenario assumes a verified application. [`crate::raw::send`] deliberately skips verification, so
//!    that a malformed-APDU probe cannot interleave five unrelated exchanges into whatever device state the scenario
//!    was setting up - and the script offset context is invalidated by exactly that. Something has to do the
//!    verification once, and this is it.
//! 2. `verify_ledger_application` caches its success in a process wide `static` with no way to reset it, by design. Its
//!    concurrency behaviour is therefore only observable while that cache is still cold, which is the first call in the
//!    process and nowhere else.

use std::{sync::Barrier, thread};

use minotari_ledger_wallet_common::common_types::Instruction;
use minotari_ledger_wallet_comms::{
    accessor_methods::{ledger_get_app_name, ledger_get_version, verify_ledger_application},
    ledger_wallet::{EXPECTED_NAME, MIN_LEDGER_APP_VERSION},
};

use crate::scenarios::{Approval, Scenario, ScenarioContext, ScenarioModule, ScenarioResult, WithContext, require};

pub const MODULE: ScenarioModule = ScenarioModule {
    name: "handshake",
    scenarios: SCENARIOS,
};

const SCENARIOS: &[Scenario] = &[
    Scenario {
        name: "verification succeeds against a real application, and survives concurrent callers",
        covers: &[
            Instruction::GetAppName,
            Instruction::GetVersion,
            Instruction::GetPublicKey,
            Instruction::GetScriptSchnorrSignature,
        ],
        approval: Approval::NotNeeded,
        run: verification_survives_concurrent_callers,
    },
    Scenario {
        name: "the application names itself and reports a version the host will accept",
        covers: &[Instruction::GetAppName, Instruction::GetVersion],
        approval: Approval::NotNeeded,
        run: name_and_version,
    },
];

/// How many threads race into `verify_ledger_application` at once.
///
/// Eight, matching `comms/tests/verify_ledger_application_concurrency.rs`, which asserts the same property against
/// a stub device with the exchange sequence instrumented.
const RACING_CALLERS: usize = 8;

/// Acceptance: the whole `verify_ledger_application` sequence is one a real application satisfies, and concurrent
/// callers do not break it.
///
/// # What this does *not* check, stated plainly
///
/// **This scenario cannot detect the `try_lock` bug that `verify_ledger_application` was fixed for**, and it is not
/// named as though it can. Under the old implementation a caller that lost the `try_lock` fell through to a
/// trailing `Ok(())` and returned `Ok` without the device having been checked - so "all eight callers returned
/// `Ok`", which is all a scenario can observe from here, was true then and is true now.
///
/// Detecting it needs the *number of device exchanges* each caller saw before being told the device was fine, and
/// that needs a transport wrapped in a counter. `comms/tests/verify_ledger_application_concurrency.rs` does exactly
/// that against a stub device, asserts the five-instruction sequence ran exactly once, and is where that property
/// lives. It cannot move here: this scenario also runs on hardware through `examples/ledger_demo.rs`, where
/// registering a wrapping transport is precisely the thing that file guarantees it never does - that guarantee is
/// what makes it impossible for the hardware frontend to reach a simulator by accident.
///
/// # What it does check, which the stub cannot
///
/// That the sequence the fix runs is one a **real application** actually satisfies end to end: the name, the
/// version floor, a signature, the public key it must verify against, and a second signature that has to differ
/// from the first. A device that failed the signature step, or answered a version below the floor, would pass every
/// stub test in the repository and fail here.
///
/// The concurrency is kept because it is free and because a handshake that desynchronised the transport under
/// contention would show up in the follow-up instruction below. Note the racing threads do not race on the
/// *device*: the lock is held across the whole of `verify()`, so exactly one thread performs I/O and the rest
/// block.
fn verification_survives_concurrent_callers(_context: &ScenarioContext<'_>) -> ScenarioResult {
    let barrier = Barrier::new(RACING_CALLERS);
    let results = thread::scope(|scope| {
        let handles: Vec<_> = (0..RACING_CALLERS)
            .map(|_| {
                let barrier = &barrier;
                scope.spawn(move || {
                    barrier.wait();
                    verify_ledger_application()
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().map_err(|_| "a verification thread panicked".to_string()))
            .collect::<Vec<_>>()
    });

    for (index, result) in results.into_iter().enumerate() {
        let verification = result.context(|| format!("caller {index}"))?;
        verification.context(|| format!("caller {index} could not verify the ledger application"))?;
    }

    // The device is still answering afterwards. A handshake that left the transport desynchronised under
    // contention - a reply read by the wrong caller, say - would surface here rather than in whichever unrelated
    // scenario happened to run next.
    let name = ledger_get_app_name().context(|| "GetAppName after the concurrent verification".to_string())?;
    require(name == EXPECTED_NAME, || {
        format!("after concurrent verification the device answered '{name}', not '{EXPECTED_NAME}'")
    })
}

/// Acceptance: the device is the MinoTari Wallet application, at a version this host is willing to drive.
///
/// The version is not pinned to an exact string. It moves every release, and a test that has to be edited on each
/// version bump gets edited without being thought about. What is pinned is the relation the host actually enforces:
/// the device's version is at or above [`MIN_LEDGER_APP_VERSION`]. `verify_ledger_application` checks the same
/// thing, but it is cached process wide and may well have been satisfied by an earlier scenario, so this states it
/// where a failure names the versions involved.
fn name_and_version(_context: &ScenarioContext<'_>) -> ScenarioResult {
    let name = ledger_get_app_name().context(|| "GetAppName".to_string())?;
    require(name == EXPECTED_NAME, || {
        format!("the device is running '{name}', not '{EXPECTED_NAME}'")
    })?;

    let version = ledger_get_version().context(|| "GetVersion".to_string())?;
    let reported = semver::Version::parse(&version)
        .context(|| format!("the device reported '{version}', which is not a semantic version"))?;
    let minimum = semver::Version::parse(MIN_LEDGER_APP_VERSION)
        .context(|| format!("MIN_LEDGER_APP_VERSION is '{MIN_LEDGER_APP_VERSION}', which is not a semantic version"))?;
    require(reported >= minimum, || {
        format!("the device is at '{reported}', below the host's minimum of '{minimum}'")
    })
}

/// The version in `wallet/Cargo.toml`, read at compile time.
///
/// Read from the manifest rather than from `CARGO_PKG_VERSION`, because `CARGO_PKG_VERSION` here is *this* crate's
/// version and the two are separate numbers that move separately.
#[cfg(test)]
const DEVICE_MANIFEST: &str = include_str!("../../../wallet/Cargo.toml");

/// The `version = "..."` of the `[package]` section of a Cargo manifest.
#[cfg(test)]
fn package_version(manifest: &str) -> Option<&str> {
    let mut in_package = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_package = line == "[package]";
            continue;
        }
        if !in_package {
            continue;
        }
        if let Some(value) = line.strip_prefix("version") {
            return value
                .trim_start()
                .strip_prefix('=')
                .map(str::trim)
                .and_then(|v| v.strip_prefix('"'))
                .and_then(|v| v.split('"').next());
        }
    }
    None
}

#[cfg(test)]
mod test {
    use super::*;

    /// The host's minimum application version must not be above the version the device application actually is.
    ///
    /// These two numbers live in different crates and are kept in step today by a comment on
    /// `MIN_LEDGER_APP_VERSION` - "Keep this in step with the ledger application's `version` in its `Cargo.toml`" -
    /// and by nothing else. Raising the floor past the application is not a subtle bug: `verify_ledger_application`
    /// then refuses **every** device, including a freshly built one, and it refuses it with a message telling the
    /// user to update firmware that is already newer than the host asked for. It also cannot be caught by any
    /// device test, because there is no device that would pass.
    ///
    /// Host only, deliberately: it needs no simulator and runs on every `cargo test`, which is the point. It is the
    /// cheapest assertion in this crate and one of the few that protects a *released* build rather than a test run.
    #[test]
    fn the_hosts_minimum_version_is_not_above_the_device_applications_own() {
        let device = package_version(DEVICE_MANIFEST).expect("wallet/Cargo.toml must have a [package] version");
        let device = semver::Version::parse(device)
            .unwrap_or_else(|e| panic!("wallet/Cargo.toml version '{device}' is not a semantic version: {e}"));
        let minimum = semver::Version::parse(MIN_LEDGER_APP_VERSION).unwrap_or_else(|e| {
            panic!("MIN_LEDGER_APP_VERSION '{MIN_LEDGER_APP_VERSION}' is not a semantic version: {e}")
        });

        assert!(
            minimum <= device,
            "MIN_LEDGER_APP_VERSION is '{minimum}' but the device application in wallet/Cargo.toml is '{device}'. The \
             host would refuse every device, including one built from this very commit, and would tell the user to \
             update firmware that is already newer than it asked for. Lower MIN_LEDGER_APP_VERSION in \
             comms/src/ledger_wallet.rs, or raise the application's version."
        );
    }

    /// The manifest reader takes the `[package]` version and not some other table's.
    ///
    /// `wallet/Cargo.toml` today has `version` under `[package]` and nowhere else, but manifests grow `[dependencies]`
    /// entries with their own `version` keys all the time, and a reader that took the first one it saw would start
    /// comparing the host's floor against a dependency's version without any sign that it had.
    #[test]
    fn the_manifest_reader_takes_the_package_version() {
        let manifest = "\
[workspace]

[package]
name = \"minotari_ledger_wallet\"
version = \"5.7.0-pre.9\"

[dependencies]
something = { version = \"1.2.3\" }
";
        assert_eq!(package_version(manifest), Some("5.7.0-pre.9"));

        // A manifest whose dependency table comes first must still not contribute its version.
        let reordered = "\
[dependencies]
something = { version = \"1.2.3\" }

[package]
version = \"9.9.9\"
";
        assert_eq!(package_version(reordered), Some("9.9.9"));

        assert_eq!(package_version("[dependencies]\nversion = \"1.0.0\"\n"), None);
    }

    /// The manifest this test reads really is the device application's, and really does carry a version. An
    /// `include_str!` pointed at the wrong file would compile and then compare the host's floor against something
    /// irrelevant.
    #[test]
    fn the_included_manifest_is_the_device_applications() {
        assert!(
            DEVICE_MANIFEST.contains("name = \"minotari_ledger_wallet\""),
            "the included manifest is not the device application's"
        );
        assert!(package_version(DEVICE_MANIFEST).is_some());
    }
}
