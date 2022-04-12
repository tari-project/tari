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

use std::{cmp::Ordering, slice::Iter};

use serde::{Deserialize, Serialize};
use strum_macros::{Display, EnumString};
use tari_crypto::tari_utilities::bit::{bytes_to_bits, checked_bits_to_uint};

use crate::{
    diacritics::*,
    error::{KeyManagerError, MnemonicError},
    mnemonic_wordlists::*,
};

/// The Mnemonic system simplifies the encoding and decoding of a secret key into and from a Mnemonic word sequence
/// It can autodetect the language of the Mnemonic word sequence
// TODO: Develop a language autodetection mechanism to distinguish between ChineseTraditional and ChineseSimplified
// #LOGGED

#[derive(Clone, Debug, PartialEq, EnumString, Display, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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
    pub fn from(mnemonic_word: &str) -> Result<MnemonicLanguage, MnemonicError> {
        let words = vec![mnemonic_word.to_string()];
        MnemonicLanguage::detect_language(&words)
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
        MNEMONIC_LANGUAGES.iter()
    }

    /// Returns the mnemonic word list count for the specified language
    pub fn word_count(language: &MnemonicLanguage) -> usize {
        match language {
            MnemonicLanguage::ChineseSimplified => MNEMONIC_CHINESE_SIMPLIFIED_WORDS.len(),
            MnemonicLanguage::English => MNEMONIC_ENGLISH_WORDS.len(),
            MnemonicLanguage::French => MNEMONIC_FRENCH_WORDS.len(),
            MnemonicLanguage::Italian => MNEMONIC_ITALIAN_WORDS.len(),
            MnemonicLanguage::Japanese => MNEMONIC_JAPANESE_WORDS.len(),
            MnemonicLanguage::Korean => MNEMONIC_KOREAN_WORDS.len(),
            MnemonicLanguage::Spanish => MNEMONIC_SPANISH_WORDS.len(),
        }
    }

    /// Detects the language of a list of words
    pub fn detect_language(words: &[String]) -> Result<MnemonicLanguage, MnemonicError> {
        let count = words.iter().len();
        match count.cmp(&1) {
            Ordering::Less => {
                return Err(MnemonicError::UnknownLanguage);
            },
            Ordering::Equal => {
                let word = words.get(0).ok_or(MnemonicError::EncodeInvalidLength)?;
                for language in MnemonicLanguage::iterator() {
                    if find_mnemonic_index_from_word(word, *language).is_ok() {
                        return Ok(*language);
                    }
                }
                return Err(MnemonicError::UnknownLanguage);
            },
            Ordering::Greater => {
                for word in words {
                    let mut languages = Vec::with_capacity(MnemonicLanguage::iterator().len());
                    // detect all languages in which a word falls into
                    for language in MnemonicLanguage::iterator() {
                        if find_mnemonic_index_from_word(word, *language).is_ok() {
                            languages.push(*language);
                        }
                    }
                    // check if at least one of the languages is consistent for all other words against languages
                    // yielded from the initial word for this iteration
                    for language in languages {
                        let mut consistent = true;
                        for compare in words {
                            if compare != word && find_mnemonic_index_from_word(compare, language).is_err() {
                                consistent = false;
                            }
                        }
                        if consistent {
                            return Ok(language);
                        }
                    }
                }
            },
        }

        Err(MnemonicError::UnknownLanguage)
    }
}

/// Finds and returns the index of a specific word in a mnemonic word list defined by the specified language
fn find_mnemonic_index_from_word(word: &str, language: MnemonicLanguage) -> Result<usize, MnemonicError> {
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
        Err(_err) => Err(MnemonicError::WordNotFound(word.to_string())),
    }
}

/// Finds and returns the word for a specific index in a mnemonic word list defined by the specified language
fn find_mnemonic_word_from_index(index: usize, language: MnemonicLanguage) -> Result<String, MnemonicError> {
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
pub fn from_bytes(bytes: &[u8], language: MnemonicLanguage) -> Result<Vec<String>, MnemonicError> {
    let mut bits = bytes_to_bits(bytes);

    // Pad with zeros if length not divisible by 11
    let group_bit_count = 11;
    let padded_size = ((bits.len() as f32 / group_bit_count as f32).ceil() * group_bit_count as f32) as usize;
    bits.resize(padded_size, false);

    // Group each set of 11 bits to form one mnemonic word
    let mut mnemonic_sequence: Vec<String> = Vec::new();
    for i in 0..bits.len() / group_bit_count {
        let start_index = i * group_bit_count;
        let stop_index = start_index + group_bit_count;
        let sub_v = &bits[start_index..stop_index].to_vec();
        let word_index = checked_bits_to_uint(sub_v).ok_or(MnemonicError::BitsToIntConversion)?;
        match find_mnemonic_word_from_index(word_index, language) {
            Ok(mnemonic_word) => mnemonic_sequence.push(mnemonic_word),
            Err(err) => return Err(err),
        }
    }

    Ok(mnemonic_sequence)
}

/// Generates a vector of bytes that represent the provided mnemonic sequence of words, the language of the mnemonic
/// sequence is detected
pub fn to_bytes(mnemonic_seq: &[String]) -> Result<Vec<u8>, MnemonicError> {
    let language = MnemonicLanguage::detect_language(mnemonic_seq)?;
    to_bytes_with_language(mnemonic_seq, &language)
}

/// Generates a vector of bytes that represent the provided mnemonic sequence of words using the specified language
/// Each of the input string map to a 11bit long word. So if we write the bit representation of the whole input, it will
/// look something like this:
/// .....CCCCCCCCCCCBBBBBBBBBBBAAAAAAAAAAA, the input represented as one very large number would look like
/// A+B*2^11+C*2^22+... And we want to cut it (from the right) to 8 bit long numbers like this:
/// .....eddddddddccccccccbbbbbbbbaaaaaaaa, the output represented as one very large number would look liek
/// a+b*2^8+c*2^16+... Where 'A' is the first mnemonic word in the seq and 'a' is the first byte output.
/// So the algo works like this:
/// We add 11bits number to what we have 'rest' shited by the number of bit representation of rest ('rest_bits').
/// We now have enough bits to get some output, we take 8 bits and produce output byte. We do this as long as we have at
/// least 8 bits in the 'rest'.
/// Sample of couple first steps:
/// 1) the first output 'a' is last 8 bits from input 'A', we have leftover 3 bits from 'A'
/// 2) We add 5 bits from 'B' to generate 'b', the leftover is 6 bits from 'B'
/// 3) We add 2 bits from 'C to generate 'c', now we have 8 bits needed to generate 'd' and we have 1 bit leftover.
pub fn to_bytes_with_language(mnemonic_seq: &[String], language: &MnemonicLanguage) -> Result<Vec<u8>, MnemonicError> {
    let mut bytes: Vec<u8> = Vec::new();
    let mut rest = 0;
    let mut rest_bits: u8 = 0;

    for curr_word in mnemonic_seq {
        let index = find_mnemonic_index_from_word(curr_word, *language)?;
        // Add 11 bits to the front
        rest += index << rest_bits;
        rest_bits += 11;
        while rest_bits >= 8 {
            // Get last 8 bits and shift it
            bytes.push(rest as u8);
            rest >>= 8;
            rest_bits -= 8;
        }
    }
    // If we have any leftover, we write it.
    if rest > 0 {
        bytes.push(rest as u8);
    }
    Ok(bytes)
}

pub trait Mnemonic<T> {
    fn from_mnemonic(mnemonic_seq: &[String], passphrase: Option<String>) -> Result<T, KeyManagerError>;
    fn from_mnemonic_with_language(
        mnemonic_seq: &[String],
        language: MnemonicLanguage,
        passphrase: Option<String>,
    ) -> Result<T, KeyManagerError>;
    fn to_mnemonic(
        &self,
        language: MnemonicLanguage,
        passphrase: Option<String>,
    ) -> Result<Vec<String>, KeyManagerError>;
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use rand::{self, rngs::OsRng};
    use tari_crypto::{keys::SecretKey, ristretto::RistrettoSecretKey, tari_utilities::byte_array::ByteArray};

    use super::*;
    use crate::mnemonic;

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
                panic!();
            }
        }
    }

    #[test]
    fn test_string_to_enum_conversion() {
        let my_enum = MnemonicLanguage::from_str("ChineseSimplified").unwrap();
        assert_eq!(my_enum, MnemonicLanguage::ChineseSimplified);
        let my_enum = MnemonicLanguage::from_str("English").unwrap();
        assert_eq!(my_enum, MnemonicLanguage::English);
        let my_enum = MnemonicLanguage::from_str("French").unwrap();
        assert_eq!(my_enum, MnemonicLanguage::French);
        let my_enum = MnemonicLanguage::from_str("Italian").unwrap();
        assert_eq!(my_enum, MnemonicLanguage::Italian);
        let my_enum = MnemonicLanguage::from_str("Japanese").unwrap();
        assert_eq!(my_enum, MnemonicLanguage::Japanese);
        let my_enum = MnemonicLanguage::from_str("Korean").unwrap();
        assert_eq!(my_enum, MnemonicLanguage::Korean);
        let my_enum = MnemonicLanguage::from_str("Spanish").unwrap();
        assert_eq!(my_enum, MnemonicLanguage::Spanish);
        let my_language = "TariVerse";
        match MnemonicLanguage::from_str(my_language) {
            Ok(_) => panic!("Language '{}' is not a member of 'MnemonicLanguage'!", my_language),
            Err(e) => assert_eq!(e, strum::ParseError::VariantNotFound),
        }
    }

    #[test]
    fn test_language_detection() {
        // Test valid Mnemonic words
        let language = MnemonicLanguage::from(&"目".to_string()).expect("");
        assert_eq!(language, MnemonicLanguage::ChineseSimplified);
        let language = MnemonicLanguage::from(&"trick".to_string()).expect("");
        assert_eq!(language, MnemonicLanguage::English);
        let language = MnemonicLanguage::from(&"risque".to_string()).expect("");
        assert_eq!(language, MnemonicLanguage::French);
        let language = MnemonicLanguage::from(&"topazio".to_string()).expect("");
        assert_eq!(language, MnemonicLanguage::Italian);
        let language = MnemonicLanguage::from(&"ふりる".to_string()).expect("");
        assert_eq!(language, MnemonicLanguage::Japanese);
        let language = MnemonicLanguage::from(&"마지막".to_string()).expect("");
        assert_eq!(language, MnemonicLanguage::Korean);
        let language = MnemonicLanguage::from(&"sala".to_string()).expect("");
        assert_eq!(language, MnemonicLanguage::Spanish);

        // Test Invalid Mnemonic words
        assert!(MnemonicLanguage::from(&"馕".to_string()).is_err()); // Invalid Mnemonic Chinese Simplified word
        assert!(MnemonicLanguage::from(&"retro".to_string()).is_err()); // Invalid Mnemonic English word
        assert!(MnemonicLanguage::from(&"flâner".to_string()).is_err()); // Invalid Mnemonic French word
        assert!(MnemonicLanguage::from(&"meriggiare".to_string()).is_err()); // Invalid Mnemonic Italian word
        assert!(MnemonicLanguage::from(&"おかあさん".to_string()).is_err()); // Invalid Mnemonic Japanese word
        assert!(MnemonicLanguage::from(&"답정너".to_string()).is_err()); // Invalid Mnemonic Korean word
        assert!(MnemonicLanguage::from(&"desvelado".to_string()).is_err()); // Invalid Mnemonic Spanish word

        // English/Spanish + English/French -> English
        let words1 = vec![
            "album".to_string(),
            "area".to_string(),
            "opera".to_string(),
            "abandon".to_string(),
        ];
        assert_eq!(
            MnemonicLanguage::detect_language(&words1),
            Ok(MnemonicLanguage::English)
        );

        // English/Spanish + English/French + Italian/Spanish
        let words2 = vec![
            "album".to_string(),
            "area".to_string(),
            "opera".to_string(),
            "abandon".to_string(),
            "tipico".to_string(),
        ];
        assert!(MnemonicLanguage::detect_language(&words2).is_err());

        // bounds check (last word is invalid)
        let words3 = vec![
            "album".to_string(),
            "area".to_string(),
            "opera".to_string(),
            "abandon".to_string(),
            "topazio".to_string(),
        ];
        assert!(MnemonicLanguage::detect_language(&words3).is_err());

        // building up a word list: English/French + French -> French
        let mut words = Vec::with_capacity(3);
        words.push("concert".to_string());
        assert_eq!(MnemonicLanguage::detect_language(&words), Ok(MnemonicLanguage::English));
        words.push("abandon".to_string());
        assert_eq!(MnemonicLanguage::detect_language(&words), Ok(MnemonicLanguage::English));
        words.push("barbier".to_string());
        assert_eq!(MnemonicLanguage::detect_language(&words), Ok(MnemonicLanguage::French));
    }

    #[test]
    fn test_find_index_from_word_or_word_from_index() {
        // Encoding and Decoding using Chinese Simplified
        let desired_index = 45;
        let desired_word = MNEMONIC_CHINESE_SIMPLIFIED_WORDS[desired_index].to_string();
        let index = find_mnemonic_index_from_word(&desired_word, MnemonicLanguage::ChineseSimplified).expect("");
        assert_eq!(desired_index, index);
        let word = find_mnemonic_word_from_index(desired_index, MnemonicLanguage::ChineseSimplified).expect("");
        assert_eq!(desired_word, word);

        // Encoding and Decoding using English Simplified
        let desired_index = 1717;
        let desired_word = MNEMONIC_ENGLISH_WORDS[desired_index].to_string();
        let index = find_mnemonic_index_from_word(&desired_word, MnemonicLanguage::English).expect("");
        assert_eq!(desired_index, index);
        let word = find_mnemonic_word_from_index(desired_index, MnemonicLanguage::English).expect("");
        assert_eq!(desired_word, word);

        // Encoding and Decoding using French Simplified
        let desired_index = 824;
        let desired_word = MNEMONIC_FRENCH_WORDS[desired_index].to_string();
        let index = find_mnemonic_index_from_word(&desired_word, MnemonicLanguage::French).expect("");
        assert_eq!(desired_index, index);
        let word = find_mnemonic_word_from_index(desired_index, MnemonicLanguage::French).expect("");
        assert_eq!(desired_word, word);

        // Encoding and Decoding using Italian Simplified
        let desired_index = 1123;
        let desired_word = MNEMONIC_ITALIAN_WORDS[desired_index].to_string();
        let index = find_mnemonic_index_from_word(&desired_word, MnemonicLanguage::Italian).expect("");
        assert_eq!(desired_index, index);
        let word = find_mnemonic_word_from_index(desired_index, MnemonicLanguage::Italian).expect("");
        assert_eq!(desired_word, word);

        // Encoding and Decoding using Japanese Simplified
        let desired_index = 1856;
        let desired_word = MNEMONIC_JAPANESE_WORDS[desired_index].to_string();
        let index = find_mnemonic_index_from_word(&desired_word, MnemonicLanguage::Japanese).expect("");
        assert_eq!(desired_index, index);
        let word = find_mnemonic_word_from_index(desired_index, MnemonicLanguage::Japanese).expect("");
        assert_eq!(desired_word, word);

        // Encoding and Decoding using Korean Simplified
        let desired_index = 345;
        let desired_word = MNEMONIC_KOREAN_WORDS[desired_index].to_string();
        let index = find_mnemonic_index_from_word(&desired_word, MnemonicLanguage::Korean).expect("");
        assert_eq!(desired_index, index);
        let word = find_mnemonic_word_from_index(desired_index, MnemonicLanguage::Korean).expect("");
        assert_eq!(desired_word, word);

        // Encoding and Decoding using Spanish Simplified
        let desired_index = 345;
        let desired_word = MNEMONIC_SPANISH_WORDS[desired_index].to_string();
        let index = find_mnemonic_index_from_word(&desired_word, MnemonicLanguage::Spanish).expect("");
        assert_eq!(desired_index, index);
        let word = find_mnemonic_word_from_index(desired_index, MnemonicLanguage::Spanish).expect("");
        assert_eq!(desired_word, word);
    }

    #[test]
    fn test_mnemonic_from_bytes_and_to_bytes() {
        let secretkey_bytes = RistrettoSecretKey::random(&mut OsRng).to_vec();
        let mnemonic_seq = mnemonic::from_bytes(&secretkey_bytes, MnemonicLanguage::English).expect("");
        let mnemonic_bytes = mnemonic::to_bytes(&mnemonic_seq).expect("");
        let mismatched_bytes = secretkey_bytes
            .iter()
            .zip(mnemonic_bytes.iter())
            .filter(|&(a, b)| a != b)
            .count();
        assert_eq!(mismatched_bytes, 0);
    }

    #[test]
    fn fuzzer() {
        use rand::RngCore;
        let start = 33;
        // We need the step by eleven to make sure that from_bytes will not do a padding with zeros.
        for len in (start..1024).step_by(11) {
            let mut secretkey_bytes = vec![0u8; len];
            OsRng.fill_bytes(&mut secretkey_bytes);
            let mnemonic_seq = mnemonic::from_bytes(&secretkey_bytes, MnemonicLanguage::English).unwrap();
            let mnemonic_bytes = mnemonic::to_bytes(&mnemonic_seq).unwrap();
            assert_eq!(secretkey_bytes, mnemonic_bytes, "failed len = {}", len);
        }
    }
}
