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

use std::{io::IsTerminal, process::ExitCode};

use minotari_ledger_wallet_comms_testing::{
    approver::HumanApprover,
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
    if needs_a_human > 0 && !std::io::stdin().is_terminal() {
        eprintln!(
            "{needs_a_human} of the selected scenarios put a review on the device and ask you about it, so this needs \
             a terminal on stdin. Run it directly rather than through a pipe; for an unattended run use the Speculos \
             suite in tests/speculos_scenarios.rs, or select only the modules that need no approval."
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

    println!();
    if failures.is_empty() {
        println!("All {passed} scenarios passed against the device.");
        return ExitCode::SUCCESS;
    }
    eprintln!("{} scenarios failed, {passed} passed:", failures.len());
    for failure in &failures {
        eprintln!("{failure}");
    }
    ExitCode::FAILURE
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
