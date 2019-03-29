// Copyright 2019 The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use crate::{diacritics::*, mnemonic_wordlists::*};
use crypto::keys::SecretKey;
use derive_error::Error;
use std::slice::Iter;
use tari_utilities::{bit::*, byte_array::ByteArrayError};

/// The Mnemonic system simplifies the encoding and decoding of a secret key into and from a Mnemonic word sequence
/// It can autodetect the language of the Mnemonic word sequence
// TODO: Develop a language autodetection mechanism to distinguish between ChineseTraditional and ChineseSimplified

#[derive(Debug, Error)]
pub enum MnemonicError {
    // Only ChineseSimplified, ChineseTraditional, English, French, Italian, Japanese, Korean and Spanish are defined
    // natural languages
    UnknownLanguage,
    // Only 2048 words for each language was selected to form Mnemonic word lists
    WordNotFound,
    // A mnemonic word does not exist for the requested index
    IndexOutOfBounds,
    // A problem encountered constructing a secret key from bytes or mnemonic sequence
    ByteArrayError(ByteArrayError),
    // Encoding and decoding a mnemonic sequence from bytes require exactly 32 bytes or 24 mnemonic words
    ConversionProblem,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MnemonicLanguage {
    ChineseSimplified,
    English,
    French,
    Italian,
    Japanese,
    Korean,
    Spanish,
}

impl MnemonicLanguage {
    /// Detects the mnemonic language of a specific word by searching all defined mnemonic word lists
    pub fn from(mnemonic_word: &String) -> Result<MnemonicLanguage, MnemonicError> {
        for language in MnemonicLanguage::iterator() {
            if find_mnemonic_index_from_word(mnemonic_word, &language).is_ok() {
                return Ok((*language).clone());
            }
        }
        return Err(MnemonicError::UnknownLanguage);
    }

    /// Returns an iterator for the MnemonicLanguage enum group to allow iteration over all defined languages
    pub fn iterator() -> Iter<'static, MnemonicLanguage> {
        static MNEMONIC_LANGUAGES: [MnemonicLanguage; 7] = [
            MnemonicLanguage::ChineseSimplified,
            MnemonicLanguage::English,
            MnemonicLanguage::French,
            MnemonicLanguage::Italian,
            MnemonicLanguage::Japanese,
            MnemonicLanguage::Korean,
            MnemonicLanguage::Spanish,
        ];
        (MNEMONIC_LANGUAGES.into_iter())
    }
}

/// Finds and returns the index of a specific word in a mnemonic word list defined by the specified language
fn find_mnemonic_index_from_word(word: &String, language: &MnemonicLanguage) -> Result<usize, MnemonicError> {
    let search_result: Result<usize, usize>;
    let lowercase_word = word.to_lowercase();
    match language {
        // Search through languages are ordered according to the predominance (number of speakers in the world) of that
        // language
        MnemonicLanguage::ChineseSimplified => {
            search_result = MNEMONIC_CHINESE_SIMPLIFIED_WORDS.binary_search(&lowercase_word.as_str())
        },
        MnemonicLanguage::English => {
            search_result = MNEMONIC_ENGLISH_WORDS.binary_search(&remove_diacritics(&lowercase_word).as_str())
        },
        MnemonicLanguage::French => {
            search_result = MNEMONIC_FRENCH_WORDS.binary_search(&remove_diacritics(&lowercase_word).as_str())
        },
        MnemonicLanguage::Italian => {
            search_result = MNEMONIC_ITALIAN_WORDS.binary_search(&remove_diacritics(&lowercase_word).as_str())
        },
        MnemonicLanguage::Japanese => search_result = MNEMONIC_JAPANESE_WORDS.binary_search(&lowercase_word.as_str()),
        MnemonicLanguage::Korean => search_result = MNEMONIC_KOREAN_WORDS.binary_search(&lowercase_word.as_str()),
        MnemonicLanguage::Spanish => {
            search_result = MNEMONIC_SPANISH_WORDS.binary_search(&remove_diacritics(&lowercase_word).as_str())
        },
    }
    match search_result {
        Ok(v) => Ok(v),
        Err(_err) => Err(MnemonicError::WordNotFound),
    }
}

/// Finds and returns the word for a specific index in a mnemonic word list defined by the specified language
fn find_mnemonic_word_from_index(index: usize, language: &MnemonicLanguage) -> Result<String, MnemonicError> {
    if index < MNEMONIC_ENGLISH_WORDS.len() {
        Ok(match language {
            // Select word according to specified language
            MnemonicLanguage::ChineseSimplified => MNEMONIC_CHINESE_SIMPLIFIED_WORDS[index],
            MnemonicLanguage::English => MNEMONIC_ENGLISH_WORDS[index],
            MnemonicLanguage::French => MNEMONIC_FRENCH_WORDS[index],
            MnemonicLanguage::Italian => MNEMONIC_ITALIAN_WORDS[index],
            MnemonicLanguage::Japanese => MNEMONIC_JAPANESE_WORDS[index],
            MnemonicLanguage::Korean => MNEMONIC_KOREAN_WORDS[index],
            MnemonicLanguage::Spanish => MNEMONIC_SPANISH_WORDS[index],
        }
        .to_string())
    } else {
        Err(MnemonicError::IndexOutOfBounds)
    }
}

/// Converts a vector of bytes to a sequence of mnemonic words using the specified language
pub fn from_bytes(bytes: Vec<u8>, language: &MnemonicLanguage) -> Result<Vec<String>, MnemonicError> {
    let mut bits = bytes_to_bits(&bytes);

    // Pad with zeros if length not devisable by 11
    let group_bit_count = 11;
    let padded_size = ((bits.len() as f32 / group_bit_count as f32).ceil() * group_bit_count as f32) as usize;
    bits.resize(padded_size, false);

    // Group each set of 11 bits to form one mnemonic word
    let mut mnemonic_sequence: Vec<String> = Vec::new();
    for i in 0..bits.len() / group_bit_count {
        let start_index = i * group_bit_count;
        let stop_index = start_index + group_bit_count;
        let sub_v = &bits[start_index..stop_index].to_vec();
        // let word_index = bits_to_uint(sub_v);
        let word_index = bits_to_uint(sub_v);
        match find_mnemonic_word_from_index(word_index as usize, language) {
            Ok(mnemonic_word) => mnemonic_sequence.push(mnemonic_word),
            Err(err) => return Err(err),
        }
    }
    (Ok(mnemonic_sequence))
}

/// Generates a mnemonic sequence of words from the provided secret key
pub fn from_secretkey<K: SecretKey>(k: &K, language: &MnemonicLanguage) -> Result<Vec<String>, MnemonicError> {
    (from_bytes(k.to_vec(), language))
}

/// Generates a vector of bytes that represent the provided mnemonic sequence of words, the language of the mnemonic
/// sequence is autodetected
pub fn to_bytes(mnemonic_seq: &Vec<String>) -> Result<Vec<u8>, MnemonicError> {
    let language = MnemonicLanguage::from(&mnemonic_seq[0])?; // Autodetect language
    (to_bytes_with_language(mnemonic_seq, &language))
}

/// Generates a vector of bytes that represent the provided mnemonic sequence of words using the specified language
pub fn to_bytes_with_language(
    mnemonic_seq: &Vec<String>,
    language: &MnemonicLanguage,
) -> Result<Vec<u8>, MnemonicError>
{
    let mut bits: Vec<bool> = Vec::new();
    for curr_word in mnemonic_seq {
        match find_mnemonic_index_from_word(curr_word, &language) {
            Ok(index) => {
                let curr_bits = uint_to_bits(index, 11);
                bits.extend(curr_bits.iter().map(|&i| i));
            },
            Err(err) => return Err(err),
        }
    }
    // Discard unused bytes
    let mut bytes = bits_to_bytes(&bits);
    for _i in 32..bytes.len() {
        bytes.pop();
    }

    if bytes.len() == 32 {
        Ok(bytes)
    } else {
        Err(MnemonicError::ConversionProblem)
    }
}

/// Generates a SecretKey that represent the provided mnemonic sequence of words, the language of the mnemonic sequence
/// is autodetected
pub fn to_secretkey<K: SecretKey>(mnemonic_seq: &Vec<String>) -> Result<K, MnemonicError> {
    let bytes = to_bytes(mnemonic_seq)?;
    match K::from_bytes(&bytes) {
        Ok(k) => Ok(k),
        Err(e) => Err(MnemonicError::from(e)),
    }
}

/// Generates a SecretKey that represent the provided mnemonic sequence of words using the specified language
pub fn to_secretkey_with_language<K: SecretKey>(
    mnemonic_seq: &Vec<String>,
    language: &MnemonicLanguage,
) -> Result<K, MnemonicError>
{
    let bytes = to_bytes_with_language(mnemonic_seq, language)?;
    match K::from_bytes(&bytes) {
        Ok(k) => Ok(k),
        Err(e) => Err(MnemonicError::from(e)),
    }
}

pub trait Mnemonic<T> {
    fn from_mnemonic(mnemonic_seq: &Vec<String>) -> Result<T, MnemonicError>;
    fn from_mnemonic_with_language(mnemonic_seq: &Vec<String>, language: &MnemonicLanguage)
        -> Result<T, MnemonicError>;
    fn to_mnemonic(&self, language: &MnemonicLanguage) -> Result<Vec<String>, MnemonicError>;
}

impl<T: SecretKey> Mnemonic<T> for T {
    /// Generates a SecretKey that represent the provided mnemonic sequence of words, the language of the mnemonic
    /// sequence is autodetected
    fn from_mnemonic(mnemonic_seq: &Vec<String>) -> Result<T, MnemonicError> {
        (to_secretkey(mnemonic_seq))
    }

    /// Generates a SecretKey that represent the provided mnemonic sequence of words using the specified language
    fn from_mnemonic_with_language(
        mnemonic_seq: &Vec<String>,
        language: &MnemonicLanguage,
    ) -> Result<T, MnemonicError>
    {
        (to_secretkey_with_language(mnemonic_seq, language))
    }

    /// Generates a mnemonic sequence of words from the provided secret key
    fn to_mnemonic(&self, language: &MnemonicLanguage) -> Result<Vec<String>, MnemonicError> {
        (from_secretkey(self, language))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::mnemonic;
    use crypto::{keys::SecretKey, ristretto::RistrettoSecretKey};
    use rand;
    use tari_utilities::byte_array::ByteArray;

    #[test]
    fn test_check_wordlists_sorted() {
        for i in 0..2047 {
            if (MNEMONIC_CHINESE_SIMPLIFIED_WORDS[i] > MNEMONIC_CHINESE_SIMPLIFIED_WORDS[i + 1]) ||
                (MNEMONIC_ENGLISH_WORDS[i] > MNEMONIC_ENGLISH_WORDS[i + 1]) ||
                (MNEMONIC_FRENCH_WORDS[i] > MNEMONIC_FRENCH_WORDS[i + 1]) ||
                (MNEMONIC_ITALIAN_WORDS[i] > MNEMONIC_ITALIAN_WORDS[i + 1]) ||
                (MNEMONIC_JAPANESE_WORDS[i] > MNEMONIC_JAPANESE_WORDS[i + 1]) ||
                (MNEMONIC_KOREAN_WORDS[i] > MNEMONIC_KOREAN_WORDS[i + 1]) ||
                (MNEMONIC_SPANISH_WORDS[i] > MNEMONIC_SPANISH_WORDS[i + 1])
            {
                assert!(false);
            }
        }
        assert!(true);
    }

    #[test]
    fn test_language_detection() {
        // Test valid Mnemonic words
        match MnemonicLanguage::from(&"目".to_string()) {
            Ok(language) => assert_eq!(language, MnemonicLanguage::ChineseSimplified),
            Err(_e) => assert!(false),
        }
        match MnemonicLanguage::from(&"trick".to_string()) {
            Ok(language) => assert_eq!(language, MnemonicLanguage::English),
            Err(_e) => assert!(false),
        }
        match MnemonicLanguage::from(&"risque".to_string()) {
            Ok(language) => assert_eq!(language, MnemonicLanguage::French),
            Err(_e) => assert!(false),
        }
        match MnemonicLanguage::from(&"topazio".to_string()) {
            Ok(language) => assert_eq!(language, MnemonicLanguage::Italian),
            Err(_e) => assert!(false),
        }
        match MnemonicLanguage::from(&"ふりる".to_string()) {
            Ok(language) => assert_eq!(language, MnemonicLanguage::Japanese),
            Err(_e) => assert!(false),
        }
        match MnemonicLanguage::from(&"마지막".to_string()) {
            Ok(language) => assert_eq!(language, MnemonicLanguage::Korean),
            Err(_e) => assert!(false),
        }
        match MnemonicLanguage::from(&"sala".to_string()) {
            Ok(language) => assert_eq!(language, MnemonicLanguage::Spanish),
            Err(_e) => assert!(false),
        }

        // Test Invalid Mnemonic words
        assert!(MnemonicLanguage::from(&"馕".to_string()).is_err()); // Invalid Mnemonic Chinese Simplified word
        assert!(MnemonicLanguage::from(&"retro".to_string()).is_err()); // Invalid Mnemonic English word
        assert!(MnemonicLanguage::from(&"flâner".to_string()).is_err()); // Invalid Mnemonic French word
        assert!(MnemonicLanguage::from(&"meriggiare".to_string()).is_err()); // Invalid Mnemonic Italian word
        assert!(MnemonicLanguage::from(&"おかあさん".to_string()).is_err()); // Invalid Mnemonic Japanese word
        assert!(MnemonicLanguage::from(&"답정너".to_string()).is_err()); // Invalid Mnemonic Korean word
        assert!(MnemonicLanguage::from(&"desvelado".to_string()).is_err()); // Invalid Mnemonic Spanish word
    }

    #[test]
    fn test_find_index_from_word_or_word_from_index() {
        // Encoding and Decoding using Chinese Simplified
        let desired_index = 45;
        let desired_word = MNEMONIC_CHINESE_SIMPLIFIED_WORDS[desired_index].to_string();
        match find_mnemonic_index_from_word(&desired_word, &MnemonicLanguage::ChineseSimplified) {
            Ok(index) => assert_eq!(desired_index, index),
            Err(_e) => assert!(false),
        }
        match find_mnemonic_word_from_index(desired_index, &MnemonicLanguage::ChineseSimplified) {
            Ok(word) => assert_eq!(desired_word, word),
            Err(_e) => assert!(false),
        }

        // Encoding and Decoding using English Simplified
        let desired_index = 1717;
        let desired_word = MNEMONIC_ENGLISH_WORDS[desired_index].to_string();
        match find_mnemonic_index_from_word(&desired_word, &MnemonicLanguage::English) {
            Ok(index) => assert_eq!(desired_index, index),
            Err(_e) => assert!(false),
        }
        match find_mnemonic_word_from_index(desired_index, &MnemonicLanguage::English) {
            Ok(word) => assert_eq!(desired_word, word),
            Err(_e) => assert!(false),
        }

        // Encoding and Decoding using French Simplified
        let desired_index = 824;
        let desired_word = MNEMONIC_FRENCH_WORDS[desired_index].to_string();
        match find_mnemonic_index_from_word(&desired_word, &MnemonicLanguage::French) {
            Ok(index) => assert_eq!(desired_index, index),
            Err(_e) => assert!(false),
        }
        match find_mnemonic_word_from_index(desired_index, &MnemonicLanguage::French) {
            Ok(word) => assert_eq!(desired_word, word),
            Err(_e) => assert!(false),
        }

        // Encoding and Decoding using Italian Simplified
        let desired_index = 1123;
        let desired_word = MNEMONIC_ITALIAN_WORDS[desired_index].to_string();
        match find_mnemonic_index_from_word(&desired_word, &MnemonicLanguage::Italian) {
            Ok(index) => assert_eq!(desired_index, index),
            Err(_e) => assert!(false),
        }
        match find_mnemonic_word_from_index(desired_index, &MnemonicLanguage::Italian) {
            Ok(word) => assert_eq!(desired_word, word),
            Err(_e) => assert!(false),
        }

        // Encoding and Decoding using Japanese Simplified
        let desired_index = 1856;
        let desired_word = MNEMONIC_JAPANESE_WORDS[desired_index].to_string();
        match find_mnemonic_index_from_word(&desired_word, &MnemonicLanguage::Japanese) {
            Ok(index) => assert_eq!(desired_index, index),
            Err(_e) => assert!(false),
        }
        match find_mnemonic_word_from_index(desired_index, &MnemonicLanguage::Japanese) {
            Ok(word) => assert_eq!(desired_word, word),
            Err(_e) => assert!(false),
        }

        // Encoding and Decoding using Korean Simplified
        let desired_index = 345;
        let desired_word = MNEMONIC_KOREAN_WORDS[desired_index].to_string();
        match find_mnemonic_index_from_word(&desired_word, &MnemonicLanguage::Korean) {
            Ok(index) => assert_eq!(desired_index, index),
            Err(_e) => assert!(false),
        }
        match find_mnemonic_word_from_index(desired_index, &MnemonicLanguage::Korean) {
            Ok(word) => assert_eq!(desired_word, word),
            Err(_e) => assert!(false),
        }

        // Encoding and Decoding using Spanish Simplified
        let desired_index = 345;
        let desired_word = MNEMONIC_SPANISH_WORDS[desired_index].to_string();
        match find_mnemonic_index_from_word(&desired_word, &MnemonicLanguage::Spanish) {
            Ok(index) => assert_eq!(desired_index, index),
            Err(_e) => assert!(false),
        }
        match find_mnemonic_word_from_index(desired_index, &MnemonicLanguage::Spanish) {
            Ok(word) => assert_eq!(desired_word, word),
            Err(_e) => assert!(false),
        }
    }

    #[test]
    fn test_mnemonic_from_bytes_and_to_bytes() {
        let mut rng = rand::OsRng::new().unwrap();
        let secretkey_bytes = RistrettoSecretKey::random(&mut rng).to_vec();
        match mnemonic::from_bytes(secretkey_bytes.clone(), &MnemonicLanguage::English) {
            Ok(mnemonic_seq) => match mnemonic::to_bytes(&mnemonic_seq) {
                Ok(mnemonic_bytes) => {
                    let mismatched_bytes = secretkey_bytes
                        .iter()
                        .zip(mnemonic_bytes.iter())
                        .filter(|&(a, b)| a != b)
                        .count();
                    assert_eq!(mismatched_bytes, 0);
                },
                Err(_e) => assert!(false),
            },
            Err(_e) => assert!(false),
        }
    }

    #[test]
    fn test_secretkey_to_mnemonic_and_from_mnemonic() {
        // Valid Mnemonic sequence
        let mut rng = rand::OsRng::new().unwrap();
        let desired_k = RistrettoSecretKey::random(&mut rng);
        match desired_k.to_mnemonic(&MnemonicLanguage::Japanese) {
            Ok(mnemonic_seq) => {
                match RistrettoSecretKey::from_mnemonic(&mnemonic_seq) {
                    Ok(mnemonic_k) => assert_eq!(desired_k, mnemonic_k),
                    Err(_e) => assert!(false),
                }
                // Language known
                match RistrettoSecretKey::from_mnemonic_with_language(&mnemonic_seq, &MnemonicLanguage::Japanese) {
                    Ok(mnemonic_k) => assert_eq!(desired_k, mnemonic_k),
                    Err(_e) => assert!(false),
                }
            },
            Err(_e) => assert!(false),
        }

        // Invalid Mnemonic sequence
        let mnemonic_seq = vec![
            "clever", "jaguar", "bus", "engage", "oil", "august", "media", "high", "trick", "remove", "tiny", "join",
            "item", "tobacco", "orange", "pny", "tomorrow", "also", "dignity", "giraffe", "little", "board", "army",
        ]
        .iter()
        .map(|x| x.to_string())
        .collect::<Vec<String>>();
        // Language not known
        match RistrettoSecretKey::from_mnemonic(&mnemonic_seq) {
            Ok(_k) => assert!(false),
            Err(_e) => assert!(true),
        }
        // Language known
        match RistrettoSecretKey::from_mnemonic_with_language(&mnemonic_seq, &MnemonicLanguage::Japanese) {
            Ok(_k) => assert!(false),
            Err(_e) => assert!(true),
        }
    }
}
