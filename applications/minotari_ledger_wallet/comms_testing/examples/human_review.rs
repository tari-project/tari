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

use std::process::ExitCode;

use minotari_ledger_wallet_comms::error::LedgerDeviceError;
use minotari_ledger_wallet_comms_testing::{
    approver::{HumanApprover, Outcome, while_reviewing},
    review::ExpectedReview,
};
use tari_common_types::{
    tari_address::{TariAddress, TariAddressFeatures},
    types::PrivateKey,
};

/// The same published address the simulator scenarios use, so that the two paths review the same transaction.
const RECEIVER_BASE58: &str =
    "f48ScXDKxTU3nCQsQrXHs4tnkAyLViSUpi21t7YuBNsJE1VpqFcNSeEzQWgNeCqnpRaCA9xRZ3VuV11F8pHyciegbCt";
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

    println!("Plug in a Ledger with a throwaway recovery phrase and open the MinoTari Wallet application.");
    println!("Asking it for a one sided metadata signature; it will put a review on its screen.");

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

fn receiver(payment_id_length: usize) -> Result<TariAddress, String> {
    let published = TariAddress::from_base58(RECEIVER_BASE58).map_err(|e| format!("{e}"))?;
    if payment_id_length == 0 {
        return Ok(published);
    }
    let view_key = published
        .public_view_key()
        .ok_or_else(|| "a dual address must have a view key".to_string())?
        .clone();
    TariAddress::new_dual_address(
        view_key,
        published.public_spend_key().clone(),
        published.network(),
        TariAddressFeatures::default() | TariAddressFeatures::PAYMENT_ID,
        Some(vec![0xAB; payment_id_length]),
    )
    .map_err(|e| format!("{e}"))
}
