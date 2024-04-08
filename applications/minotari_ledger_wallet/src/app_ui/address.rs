// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use core::str::from_utf8_mut;

use ledger_device_sdk::ui::{
    bitmaps::{CROSSMARK, EYE, VALIDATE_14},
    gadgets::{Field, MultiFieldReview},
};

use crate::AppSW;

// Display only the last 20 bytes of the address
const DISPLAY_ADDR_BYTES_LEN: usize = 20;

pub fn ui_display_pk(addr: &[u8]) -> Result<bool, AppSW> {
    let mut addr_hex = [0u8; DISPLAY_ADDR_BYTES_LEN * 2 + 2];
    addr_hex[..2].copy_from_slice("0x".as_bytes());
    hex::encode_to_slice(&addr[addr.len() - DISPLAY_ADDR_BYTES_LEN..], &mut addr_hex[2..]).unwrap();
    let addr_hex = from_utf8_mut(&mut addr_hex).unwrap();
    addr_hex[2..].make_ascii_uppercase();

    let my_field = [Field {
        name: "Address",
        value: addr_hex,
    }];

    let my_review = MultiFieldReview::new(
        &my_field,
        &["Confirm Address"],
        Some(&EYE),
        "Approve",
        Some(&VALIDATE_14),
        "Reject",
        Some(&CROSSMARK),
    );

    Ok(my_review.show())
}
