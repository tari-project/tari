// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use digest::Digest;
use tari_crypto::{
    hash_domain,
    hashing::{DomainSeparatedHasher, LengthExtensionAttackResistant},
};

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

hash_domain!(KeyManagerDomain, "com.tari.tari_project.base_layer.key_manager", 1);

const LABEL_ARGON_ENCODING: &str = "argon2_encoding";
const LABEL_CHACHA20_ENCODING: &str = "chacha20_encoding";
const LABEL_MAC_GENERATION: &str = "mac_generation";
const LABEL_DERIVE_KEY: &str = "derive_key";

pub(crate) fn mac_domain_hasher<D: Digest + LengthExtensionAttackResistant>(
    label: &'static str,
) -> DomainSeparatedHasher<D, KeyManagerDomain> {
    DomainSeparatedHasher::<D, KeyManagerDomain>::new_with_label(label)
}
