// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use tari_crypto::{hash::blake2::Blake256, hash_domain, hashing::DomainSeparatedHasher};

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

hash_domain!(KeyManagerMacDomain, "com.tari.tari_project.base_layer.key_manager", 1);

hash_domain!(KeyManagerHashDomain, "com.tari.base_layer.key_manager");

pub fn base_layer_key_manager_mac_generation() -> DomainSeparatedHasher<Blake256, KeyManagerHashDomain> {
    DomainSeparatedHasher::<Blake256, KeyManagerHashDomain>::new("cipher_seed.mac_generation")
}

pub fn base_layer_key_manager_argon2_encoding() -> DomainSeparatedHasher<Blake256, KeyManagerHashDomain> {
    DomainSeparatedHasher::<Blake256, KeyManagerHashDomain>::new("cipher_seed.argon2_encoding")
}

pub fn base_layer_key_manager_chacha20_encoding() -> DomainSeparatedHasher<Blake256, KeyManagerHashDomain> {
    DomainSeparatedHasher::<Blake256, KeyManagerHashDomain>::new("cipher_seed.chacha20_encoding")
}
