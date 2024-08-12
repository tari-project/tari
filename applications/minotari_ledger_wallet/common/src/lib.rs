// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

#![no_std]

//! # Common types shared by the Ledger application and the rest of the Tari codebase.
/// Note: `ledger-device-rust-sdk` cannot be included in this crate as it can only be compiled for no-std and the
///        rest of the Tari code base is compiled for std.
extern crate alloc;

pub mod common_types;
mod utils;
pub use utils::{
    get_public_spend_key_from_tari_dual_address,
    hex_to_bytes_serialized,
    tari_dual_address_display,
    PUSH_PUBKEY_IDENTIFIER,
    TARI_DUAL_ADDRESS_SIZE,
};
