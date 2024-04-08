// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use ledger_device_sdk::ui::{
    bitmaps::{CROSSMARK, EYE, VALIDATE_14},
    gadgets::{Field, MultiFieldReview},
};
use numtoa::NumToA;

use crate::{handlers::sign_tx::Tx, utils::concatenate, AppSW};

const MAX_COIN_LENGTH: usize = 10;

/// Displays a transaction and returns true if user approved it.
///
/// This method can return [`AppSW::TxDisplayFail`] error if the coin name length is too long.
///
/// # Arguments
///
/// * `tx` - Transaction to be displayed for validation
pub fn ui_display_tx(tx: &Tx) -> Result<bool, AppSW> {
    // Generate string for amount
    let mut numtoa_buf = [0u8; 20];
    let mut value_buf = [0u8; 20 + MAX_COIN_LENGTH + 1];

    let value_str = concatenate(
        &[tx.coin, " ", tx.value.numtoa_str(10, &mut numtoa_buf)],
        &mut value_buf,
    )
    .map_err(|_| AppSW::TxDisplayFail)?; // Fails if value_buf is too small

    // Generate destination address string in hexadecimal format.
    let mut to_str = [0u8; 42];
    to_str[..2].copy_from_slice("0x".as_bytes());
    hex::encode_to_slice(tx.to, &mut to_str[2..]).unwrap();
    to_str[2..].make_ascii_uppercase();

    // Define transaction review fields
    let my_fields = [
        Field {
            name: "Amount",
            value: value_str,
        },
        Field {
            name: "Destination",
            value: core::str::from_utf8(&to_str).unwrap(),
        },
        Field {
            name: "Memo",
            value: tx.memo,
        },
    ];

    // Create transaction review
    let my_review = MultiFieldReview::new(
        &my_fields,
        &["Review ", "Transaction"],
        Some(&EYE),
        "Approve",
        Some(&VALIDATE_14),
        "Reject",
        Some(&CROSSMARK),
    );

    Ok(my_review.show())
}
