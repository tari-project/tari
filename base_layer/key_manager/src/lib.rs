// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

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

use tari_common::hashing_domain::*;

/// The key_manager DHT domain separated hashing domain
/// Usage:
///   let hash = comms_dht_hash_domain().digest::<Blake256>(b"my secret");
///   etc.
pub fn comms_dht_hash_domain() -> HashingDomain {
    HashingDomain::new("base_layer.key_manager")
}

/// The key manager domain separated hashing domain
/// Usage:
///   let hash = comms_dht_hash_domain().digest::<Blake256>(b"my secret");
///   etc.
pub fn base_layer_key_manager_mac_generation() -> HashingDomain {
    HashingDomain::new("base_layer.key_manager.cipher_seed.mac_generation")
}

pub fn base_layer_key_manager_argon2_encoding() -> HashingDomain {
    HashingDomain::new("base_layer.key_manager.cipher_seed.argon2_encoding")
}

pub fn base_layer_key_manager_chacha20_encoding() -> HashingDomain {
    HashingDomain::new("base_layer.key_manager.cipher_seed.chacha20_encoding")
}
