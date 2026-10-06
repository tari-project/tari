// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Run one review scenario against a **real Ledger**, with a human as the assertion oracle.
//!
//! ```text
//! cargo run --manifest-path applications/minotari_ledger_wallet/comms_testing/Cargo.toml \
//!   --example human_review -- [approve|reject] [payment-id-bytes]
//! ```
//!
//! # Why this exists
//!
//! `SpeculosApprover` drives a simulator over an HTTP API that a real device does not have, so the automated
//! scenarios cannot be pointed at hardware even deliberately. [`HumanApprover`] is the other implementation of the
//! same trait: it prints the fields the scenario expects, asks whether the device in your hand shows exactly
//! those, and then waits while you press the button yourself.
//!
//! That is not a weaker assertion for want of a better one. On real hardware there is nothing to read the screen
//! with except a person, and the alternative - running the scenario on hardware while asserting nothing - would
//! mean the hardware path silently tested less than the simulator path while looking identical in a report.
//!
//! **Read the values off the device, not off the terminal.** The whole point of the exercise is that the terminal
//! is the untrusted side.
//!
//! # No transport is registered here
//!
//! This crate enables `minotari_ledger_wallet_comms/test_transport`, but nothing in this example calls
//! `register_transport`, so `Command::execute` takes its normal HID path and talks to whatever Ledger is plugged
//! in. There is no way for this example to reach a simulator by accident.
//!
//! # This will ask a real device to sign
//!
//! Approving produces a real signature from a real key. Use a device with a throwaway recovery phrase, and never
//! one holding value.

use std::{io::IsTerminal, process::ExitCode};

use minotari_ledger_wallet_comms::error::LedgerDeviceError;
use minotari_ledger_wallet_comms_testing::{
    approver::{HumanApprover, Outcome, while_reviewing},
    fixtures,
    review::ExpectedReview,
};
use tari_common_types::{tari_address::TariAddress, types::PrivateKey};

const ACCOUNT: u64 = 0;
const SENDER_OFFSET_KEY_INDEX: u64 = 7;
const VALUE: u64 = 12_345;

fn main() -> ExitCode {
    let mut arguments = std::env::args().skip(1);
    let outcome = match arguments.next().as_deref() {
        None | Some("approve") => Outcome::Approve,
        Some("reject") => Outcome::Reject,
        Some(other) => {
            eprintln!("Unknown outcome '{other}'; expected 'approve' or 'reject'");
            return ExitCode::FAILURE;
        },
    };
    let payment_id_length: usize = match arguments.next().as_deref().map(str::parse) {
        None => 0,
        Some(Ok(length)) => length,
        Some(Err(e)) => {
            eprintln!("The payment ID length must be a number of bytes: {e}");
            return ExitCode::FAILURE;
        },
    };

    let receiver = match receiver(payment_id_length) {
        Ok(receiver) => receiver,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        },
    };
    let expected = ExpectedReview::one_sided_metadata_signature(VALUE, &receiver.to_base58(), payment_id_length);

    // Checked *before* the instruction goes out, and that ordering is the whole point.
    //
    // `HumanApprover` cannot ask its questions without a terminal, and every one of its failure paths happens with
    // a review already on the device and an APDU exchange already outstanding - which on the HID transport has no
    // timeout and cannot be cancelled from this side. Discovering "there is nobody to ask" at that moment leaves
    // the operator, if there is one, with a process that will not return until somebody presses a button. Refusing
    // to start is free; refusing to start afterwards is not possible.
    if !std::io::stdin().is_terminal() {
        eprintln!(
            "This example asks a human questions, so it needs a terminal on stdin. Run it directly rather than \
             through a pipe or a CI step; for an unattended run use the Speculos scenarios in \
             tests/speculos_review.rs instead."
        );
        return ExitCode::FAILURE;
    }

    println!("Plug in a Ledger with a throwaway recovery phrase and open the MinoTari Wallet application.");
    println!("Asking it for a one sided metadata signature; it will put a review on its screen.");
    println!("If anything is wrong, say so at the prompt - you will be asked to reject it on the device, and the");
    println!("run will then end with what you found. Do not close this window instead: the instruction is already");
    println!("on its way and only the device can answer it.");

    let (signature, review) = while_reviewing(&HumanApprover, &expected, outcome, || {
        minotari_ledger_wallet_comms::accessor_methods::ledger_get_one_sided_metadata_signature(
            ACCOUNT,
            receiver.network(),
            0,
            VALUE,
            SENDER_OFFSET_KEY_INDEX,
            &PrivateKey::from(42u64),
            &receiver,
            &[7u8; 32],
        )
    });

    if let Err(e) = review {
        eprintln!("\nThe operator did not confirm the review: {e}");
        return ExitCode::FAILURE;
    }

    match (outcome, signature) {
        (Outcome::Approve, Ok(_)) => {
            println!("\nThe device signed. The screen showed what the host asked it to show.");
            ExitCode::SUCCESS
        },
        (Outcome::Reject, Err(LedgerDeviceError::UserCancelled)) => {
            println!("\nThe device returned UserCancelled, as it should for a rejected review.");
            ExitCode::SUCCESS
        },
        (Outcome::Reject, other) => {
            eprintln!("\nRejecting should have returned LedgerDeviceError::UserCancelled, got {other:?}");
            ExitCode::FAILURE
        },
        (Outcome::Approve, Err(e)) => {
            eprintln!("\nThe review was approved but the device did not sign: {e}");
            ExitCode::FAILURE
        },
    }
}

/// The same published address every other review path uses, so that they all review one transaction.
fn receiver(payment_id_length: usize) -> Result<TariAddress, String> {
    fixtures::published_receiver(payment_id_length)
}
