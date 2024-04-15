// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::str::FromStr;

use cipher_seed::BIRTHDAY_GENESIS_FROM_UNIX_EPOCH;
use digest::Digest;
use tari_crypto::{
    hash_domain,
    hashing::{DomainSeparatedHasher, LengthExtensionAttackResistant},
};
use tari_utilities::{hidden::Hidden, hidden_type, safe_array::SafeArray};
use zeroize::Zeroize;

use crate::{
    cipher_seed::{CIPHER_SEED_ENCRYPTION_KEY_BYTES, CIPHER_SEED_MAC_KEY_BYTES},
    error::MnemonicError,
};

pub mod cipher_seed;
pub mod diacritics;
pub mod error;
pub mod key_manager;
#[cfg(feature = "key_manager_service")]
pub mod key_manager_service;
pub mod mnemonic;
pub mod mnemonic_wordlists;
#[cfg(feature = "key_manager_service")]
pub mod schema;

hash_domain!(KeyManagerDomain, "com.tari.base_layer.key_manager", 1);

const LABEL_ARGON_ENCODING: &str = "argon2_encoding";
const LABEL_CHACHA20_ENCODING: &str = "chacha20_encoding";
const LABEL_MAC_GENERATION: &str = "mac_generation";
const LABEL_DERIVE_KEY: &str = "derive_key";

pub(crate) fn mac_domain_hasher<D: Digest + LengthExtensionAttackResistant>(
    label: &'static str,
) -> DomainSeparatedHasher<D, KeyManagerDomain> {
    DomainSeparatedHasher::<D, KeyManagerDomain>::new_with_label(label)
}

hidden_type!(CipherSeedEncryptionKey, SafeArray<u8, CIPHER_SEED_ENCRYPTION_KEY_BYTES>);
hidden_type!(CipherSeedMacKey, SafeArray< u8, CIPHER_SEED_MAC_KEY_BYTES>);

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

impl PartialEq for SeedWords {
    fn eq(&self, other: &Self) -> bool {
        (other.len() == self.len()) && (0..self.len()).all(|i| self.get_word(i).unwrap() == other.get_word(i).unwrap())
    }
}

impl SeedWords {
    pub fn new(words: Vec<Hidden<String>>) -> Self {
        Self { words }
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

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    pub fn push(&mut self, word: String) {
        let word = Hidden::hide(word);
        self.words.push(word);
    }

    pub fn join(&self, sep: &str) -> Hidden<String> {
        Hidden::hide(
            self.words
                .iter()
                .map(|s| s.reveal().as_str())
                .collect::<Vec<_>>()
                .join(sep),
        )
    }
}

impl FromStr for SeedWords {
    type Err = MnemonicError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let words = s.split(' ').map(|s| Hidden::hide(String::from(s))).collect::<Vec<_>>();
        Ok(Self { words })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub fn test_seed_words_len() {
        let seed_words = SeedWords::new(vec![]);
        assert_eq!(seed_words.len(), 0_usize);

        let seed_words = SeedWords::new(vec![
            Hidden::hide("hi".to_string()),
            Hidden::hide("niao".to_string()),
            Hidden::hide("hola".to_string()),
            Hidden::hide("bonjour".to_string()),
            Hidden::hide("olá".to_string()),
        ]);

        assert_eq!(seed_words.len(), 5_usize);
    }

    #[test]
    pub fn test_seed_words_get_word_at_index() {
        let seed_words = SeedWords::new(vec![
            Hidden::hide("hi".to_string()),
            Hidden::hide("niao".to_string()),
            Hidden::hide("hola".to_string()),
            Hidden::hide("bonjour".to_string()),
            Hidden::hide("olá".to_string()),
        ]);

        let vec_words = [
            "hi".to_string(),
            "niao".to_string(),
            "hola".to_string(),
            "bonjour".to_string(),
            "olá".to_string(),
        ];

        for (index, word) in vec_words.iter().enumerate().take(5_usize) {
            // should not derefence, in practice. We do it here, for testing purposes
            assert_eq!(*seed_words.get_word(index).unwrap(), *word);
        }
    }

    #[test]
    pub fn test_seed_words_is_empty() {
        let seed_words = SeedWords::new(vec![]);
        assert!(seed_words.is_empty());

        let seed_words = SeedWords::new(vec![
            Hidden::hide("hi".to_string()),
            Hidden::hide("niao".to_string()),
            Hidden::hide("hola".to_string()),
            Hidden::hide("bonjour".to_string()),
            Hidden::hide("olá".to_string()),
        ]);

        assert!(!seed_words.is_empty());
    }

    #[test]
    pub fn test_seed_words_push() {
        let mut seed_words = SeedWords::new(vec![
            Hidden::hide("hi".to_string()),
            Hidden::hide("niao".to_string()),
            Hidden::hide("hola".to_string()),
            Hidden::hide("bonjour".to_string()),
            Hidden::hide("olá".to_string()),
        ]);

        seed_words.push("ciao".to_string());
        assert_eq!(seed_words.len(), 6_usize);
        assert_eq!(seed_words.get_word(5).unwrap(), "ciao")
    }

    #[test]
    pub fn test_seed_words_join() {
        let seed_words = SeedWords::new(vec![
            Hidden::hide("hi".to_string()),
            Hidden::hide("niao".to_string()),
            Hidden::hide("hola".to_string()),
            Hidden::hide("bonjour".to_string()),
            Hidden::hide("olá".to_string()),
        ]);

        let joined = seed_words.join(", ");
        assert_eq!(joined.reveal(), "hi, niao, hola, bonjour, olá");
    }

    #[test]
    pub fn test_seed_words_partial_eq() {
        let seed_words = SeedWords::new(vec![
            Hidden::hide("hi".to_string()),
            Hidden::hide("niao".to_string()),
            Hidden::hide("hola".to_string()),
            Hidden::hide("bonjour".to_string()),
            Hidden::hide("olá".to_string()),
        ]);

        let other_seed_words = SeedWords::new(vec![
            Hidden::hide("hi".to_string()),
            Hidden::hide("niao".to_string()),
            Hidden::hide("hola".to_string()),
            Hidden::hide("bonjour".to_string()),
            Hidden::hide("olá".to_string()),
        ]);

        // equality should hold, in this case
        assert_eq!(seed_words, other_seed_words);

        let other_seed_words = SeedWords::new(vec![]);

        // equality fails, in case of distinguished len
        assert_ne!(seed_words, other_seed_words);

        let other_seed_words = SeedWords::new(vec![
            Hidden::hide("hi".to_string()),
            Hidden::hide("niao".to_string()),
            Hidden::hide("hola".to_string()),
            Hidden::hide("bonjour".to_string()),
            Hidden::hide("ciao".to_string()),
        ]);

        // equality fails, in case of same len but distinct words
        assert_ne!(seed_words, other_seed_words);
    }
}
