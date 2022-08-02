// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

<<<<<<< HEAD
use tari_crypto::{hash::blake2::Blake256, hash_domain, hashing::DomainSeparatedHasher};
=======
use tari_crypto::hash_domain;
>>>>>>> development

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

hash_domain!(
    KeyManagerDomain,
    "com.tari.tari_project.base_layer.key_manager",
    1
);
hash_domain!(
    KeyManagerMacGeneration,
    "com.tari.tari_project.base_layer.key_manager.mac_generation",
    1
);
hash_domain!(
    KeyManagerArgon2Encoding,
    "com.tari.tari_project.base_layer.key_manager.argon2_encoding",
    1
);
hash_domain!(
    KeyManagerChacha20Encoding,
    "com.tari.tari_project.base_layer.key_manager.chacha20_encoding",
    1
);
