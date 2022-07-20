// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use tari_common::hashing_domain::HashingDomain;

pub mod cipher_seed;
pub mod diacritics;
pub mod error;
pub mod key_manager;
pub mod mnemonic;
pub mod mnemonic_wordlists;
//  https://github.com/rustwasm/wasm-bindgen/issues/2774
#[allow(clippy::unused_unit)]
#[cfg(feature = "wasm")]
pub mod wasm;

/// The base layer key manager domain separated hashing domain
/// Usage:
///   let hash = KEY_MANAGER_HASH_DOMAIN.digest::<Blake256>(b"my secret");
///   etc.
pub const KEY_MANAGER_HASH_DOMAIN: HashingDomain = HashingDomain::new("tari_project.base_layer.key_manager");
