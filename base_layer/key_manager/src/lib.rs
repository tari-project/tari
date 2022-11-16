// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use cipher_seed::BIRTHDAY_GENESIS_FROM_UNIX_EPOCH;
use digest::Digest;
use serde::{Deserialize, Serialize};
use tari_crypto::{
    hash_domain,
    hashing::{DomainSeparatedHasher, LengthExtensionAttackResistant},
};
use tari_utilities::{hidden::Hidden, hidden_type};
use zeroize::Zeroize;

use crate::{
    cipher_seed::{CIPHER_SEED_ENCRYPTION_KEY_BYTES, CIPHER_SEED_MAC_KEY_BYTES},
    error::MnemonicError,
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

hidden_type!(CipherSeedEncryptionKey, [u8; CIPHER_SEED_ENCRYPTION_KEY_BYTES]);
hidden_type!(CipherSeedMacKey, [u8; CIPHER_SEED_MAC_KEY_BYTES]);

/// Computes the birthday duration, in seconds, from the unix epoch. Currently, birthday is stored
/// on the wallet as days since 2022-01-01, mainly to preserve space regarding u16 type. That said,
/// for wallet synchronization, it is necessary we are compatible with block timestamps (calculated
/// from unix epoch). This function adds this functionality
pub fn get_birthday_from_unix_epoch_in_seconds(birthday: u16, to_days: u16) -> u64 {
    u64::from(birthday.saturating_sub(to_days)) * 24 * 60 * 60 + BIRTHDAY_GENESIS_FROM_UNIX_EPOCH
}

#[derive(Debug, Clone)]
pub struct SeedWords {
    words: Vec<Hidden<String>>,
}

impl SeedWords {
    pub fn new(words: &[String]) -> Self {
        Self {
            words: words.into_iter().map(|m| Hidden::hide(m.clone())).collect::<Vec<_>>(),
        }
    }

    pub fn len(&self) -> usize {
        self.words.len()
    }

    pub fn get_word(&self, index: usize) -> Result<&String, MnemonicError> {
        if index > self.len() - 1 {
            return Err(MnemonicError::IndexOutOfBounds);
        }

        Ok(self.words[index].reveal())
    }
}
