// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! The hardware frontend: every scenario the Speculos suite runs, run against a **real Ledger**, with assertions.
//!
//! ```text
//! cargo run --manifest-path applications/minotari_ledger_wallet/comms_testing/Cargo.toml --example ledger_demo
//! cargo run --manifest-path applications/minotari_ledger_wallet/comms_testing/Cargo.toml --example ledger_demo -- legacy_nonce
//! ```
//!
//! # What changed, and why it moved here
//!
//! This replaces `comms/examples/ledger_demo/main.rs`, which printed results for a human to read and **asserted
//! nothing** - so it only caught a regression if somebody happened to be looking at the right line of output. Its
//! own header said as much, and said the plan was for it to become a thin frontend over the shared scenario
//! library.
//!
//! It could not become that where it was. The scenario library lives in `minotari_ledger_wallet_comms_testing`,
//! which depends on `minotari_ledger_wallet_comms`; an example inside `comms` cannot depend back on it. So the
//! frontend is here, next to the library it runs, alongside `examples/human_review.rs`, which reaches real
//! hardware from this crate in exactly the same way and for the same reason.
//!
//! # No transport is registered, so this cannot reach a simulator
//!
//! This crate enables `minotari_ledger_wallet_comms/test_transport`, but nothing here calls `register_transport`,
//! so `Command::execute` takes its normal HID path and talks to whatever Ledger is plugged in. There is no way for
//! this example to reach a simulator by accident, and no way for the Speculos tests to reach hardware: those drive
//! the device over an HTTP control API that a real Ledger does not have.
//!
//! # What you need before running it
//!
//! * A Ledger with the MinoTari Wallet application open.
//! * **One of the two published test mnemonics in `seeds.rs` restored onto it.** The vector scenarios assert derived
//!   keys, so they can only run against a device whose seed is known - and they refuse, loudly and without printing
//!   anything the device said, against one they cannot identify. That refusal is a feature: this frontend prints and
//!   asserts on values the device returns, including a `GetViewKey` secret scalar.
//! * **A device that will never hold value.** Those mnemonics are public. Anything sent to an address derived from them
//!   can be swept by anyone, instantly, with no compromise of your machine.
//! * A terminal. Exactly one scenario raises a review, and `HumanApprover` asks you two questions about it.
//!
//! # One scenario needs you; the rest do not
//!
//! `GetOneSidedMetadataSignature` is the only handler in the application that puts anything on the screen, so it is
//! the only scenario you have to answer. Everything else - the malformed APDU probes, the nonce store eviction
//! probes, the whole legacy nonce whitelist - runs unattended on hardware, which is what makes running the *same*
//! scenarios on both frontends affordable in the first place.
//!
//! # The transport probes at the end, which only hardware can run at all
//!
//! After the scenarios, a full run asks you to close the application, unplug the device, plug it back in locked,
//! and reopen the application - checking after each that the client reports a `Processing` error rather than
//! returning a key or hanging. These are the transport's "the device went away" paths, which is the most common
//! real failure for a wallet that ships with `ledger` on by default.
//!
//! They are **not** scenarios and cannot be. A scenario runs on the simulator too, and there is no way to unplug a
//! Speculos container from a test; these states only exist when a person moves a physical device. That is exactly
//! why they have to live here - deleting them along with the old `ledger_demo` would have left those paths with no
//! coverage anywhere in the repository.
//!
//! Naming any module on the command line skips them, so iterating on one module does not cost four prompts.

use std::{io::IsTerminal, process::ExitCode};

use minotari_ledger_wallet_comms::{accessor_methods::ledger_get_view_key, error::LedgerDeviceError};
use minotari_ledger_wallet_comms_testing::{
    approver::HumanApprover,
    fixtures,
    scenarios::{Approval, MODULES, ScenarioContext, ScenarioModule},
};

fn main() -> ExitCode {
    let wanted: Vec<String> = std::env::args().skip(1).collect();
    let modules: Vec<&ScenarioModule> = if wanted.is_empty() {
        MODULES.iter().collect()
    } else {
        match select(&wanted) {
            Ok(modules) => modules,
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::FAILURE;
            },
        }
    };

    let needs_a_human = modules
        .iter()
        .flat_map(|module| module.scenarios.iter())
        .filter(|scenario| scenario.approval == Approval::Required)
        .count();

    // Checked before the first instruction goes out, and that ordering is the point.
    //
    // `HumanApprover` cannot ask its questions without a terminal, and every one of its failure paths happens with
    // a review already on the device and an APDU exchange already outstanding - which on the HID transport has no
    // timeout and cannot be cancelled from this side. Discovering "there is nobody to ask" at that moment leaves
    // the operator, if there is one, with a process that will not return until somebody presses a button.
    // A full run also ends with the four physically driven transport probes, which prompt as well.
    let runs_transport_probes = wanted.is_empty();
    if (needs_a_human > 0 || runs_transport_probes) && !std::io::stdin().is_terminal() {
        eprintln!(
            "This run asks you questions - {needs_a_human} scenario(s) put a review on the device, and a full run \
             ends with four physical transport probes - so it needs a terminal on stdin. Run it directly rather than \
             through a pipe; for an unattended run use the Speculos suite in tests/speculos_scenarios.rs, or name \
             only the modules that need no approval."
        );
        return ExitCode::FAILURE;
    }

    println!("Plug in a Ledger with one of the published test mnemonics restored onto it - never one holding value -");
    println!("and open the MinoTari Wallet application.");
    println!();
    println!(
        "Running {} scenarios across {} modules; {needs_a_human} of them will ask you to check the device's screen.",
        modules.iter().map(|module| module.scenarios.len()).sum::<usize>(),
        modules.len()
    );
    if runs_transport_probes {
        println!("Then four transport probes, each asking you to close, unplug, reconnect or reopen the device.");
    }

    let approver = HumanApprover;
    let context = ScenarioContext::new(&approver);
    let mut failures = Vec::new();
    let mut passed = 0usize;

    for module in &modules {
        println!();
        println!("======================================================================");
        println!("  {}", module.name);
        println!("======================================================================");
        for scenario in module.scenarios {
            match scenario.approval {
                Approval::NotNeeded => println!("  ... {}", scenario.name),
                Approval::Required => println!("  ... {} (you will be asked about the screen)", scenario.name),
            }
            // Run, record, carry on. A failing scenario must not hide the ones after it: on hardware a full run
            // costs an operator's attention, and a run that stopped at the first failure would cost it twice.
            match (scenario.run)(&context) {
                Ok(()) => {
                    passed = passed.saturating_add(1);
                    println!("      ok");
                },
                Err(e) => {
                    println!("      FAILED");
                    failures.push(format!("  [{}] {}\n      {e}", module.name, scenario.name));
                },
            }
        }
    }

    // Only on a full run: these cost four prompts and four physical actions, and somebody iterating on one module
    // does not want them.
    if runs_transport_probes {
        match transport_probes() {
            Ok(checked) => passed = passed.saturating_add(checked),
            Err(e) => failures.push(format!("  [transport] {e}")),
        }
    }

    println!();
    if failures.is_empty() {
        println!("All {passed} checks passed against the device.");
        return ExitCode::SUCCESS;
    }
    eprintln!("{} checks failed, {passed} passed:", failures.len());
    for failure in &failures {
        eprintln!("{failure}");
    }
    ExitCode::FAILURE
}

/// The four states only a person with the device in their hand can produce, and what the client must say in each.
///
/// Carried over from the `ledger_demo` this file replaces, as assertions rather than as prints followed by an early
/// return. They are weak - they check the *shape* of the error, not its content - but they are the only coverage
/// the transport's failure paths have, and the property they defend is a real one: when the device goes away, the
/// client must return an error rather than a key, and must return it rather than blocking for ever.
///
/// `ledger_get_view_key` is the probe in every case because it is cheap, takes no user interaction, and returns a
/// value that is obviously wrong to have received from an absent device.
///
/// Returns how many probes passed, so a full run's count includes them.
fn transport_probes() -> Result<usize, String> {
    println!();
    println!("======================================================================");
    println!("  transport (four physical steps, four prompts)");
    println!("======================================================================");

    let account = fixtures::random_u64();
    let probes: [(&str, &str); 3] = [
        (
            "Exit the 'MinoTari Wallet' application on the device, then press Enter",
            "with the application closed",
        ),
        ("Unplug the device, then press Enter", "with the device unplugged"),
        (
            "Plug the device back in, leave it locked or at its dashboard, then press Enter",
            "with the device reconnected but the application not open",
        ),
    ];

    let mut passed = 0usize;
    for (prompt, label) in probes {
        ask(prompt)?;
        match ledger_get_view_key(account) {
            // The specific variant matters. `Processing` is what every one of these paths is supposed to produce;
            // some other error would mean the failure took a route nobody designed, and `Ok` would mean the client
            // handed back a key it cannot possibly have obtained.
            Err(LedgerDeviceError::Processing(e)) => {
                println!("      ok: {label}, the client reported: {e}");
                passed = passed.saturating_add(1);
            },
            Ok(_) => {
                return Err(format!(
                    "GetViewKey returned a key {label}. The client cannot have obtained one - either the step was not \
                     actually performed, or the transport is answering from something other than the device."
                ));
            },
            Err(e) => {
                return Err(format!(
                    "GetViewKey {label} failed with {e:?}, but every one of these paths is supposed to surface as \
                     LedgerDeviceError::Processing. A different variant means the failure took a route nobody \
                     designed for it."
                ));
            },
        }
    }

    // And the device recovers: reopening the application makes the same call work again. Without this the three
    // probes above would pass just as happily against a client that had permanently wedged itself.
    ask("Open the 'MinoTari Wallet' application again, then press Enter")?;
    match ledger_get_view_key(account) {
        Ok(_) => {
            println!("      ok: the application is back and GetViewKey succeeds again");
            Ok(passed.saturating_add(1))
        },
        Err(e) => Err(format!(
            "GetViewKey still fails after the application was reopened ({e}), so the client did not recover from the \
             device going away"
        )),
    }
}

/// Ask the operator to do something physical and wait until they say they have.
fn ask(prompt: &str) -> Result<(), String> {
    println!();
    dialoguer::Confirm::new()
        .with_prompt(prompt)
        .default(false)
        .interact()
        .map_err(|e| format!("could not prompt the operator ({e})"))
        .and_then(|done| {
            if done {
                Ok(())
            } else {
                Err(format!("the operator did not do it: {prompt}"))
            }
        })
}

/// The modules named on the command line, or a message naming the ones that exist.
fn select(wanted: &[String]) -> Result<Vec<&'static ScenarioModule>, String> {
    let known: Vec<&str> = MODULES.iter().map(|module| module.name).collect();
    wanted
        .iter()
        .map(|name| {
            MODULES.iter().find(|module| module.name == name).ok_or_else(|| {
                format!(
                    "There is no '{name}' scenario module. Known modules: {}",
                    known.join(", ")
                )
            })
        })
        .collect()
}
