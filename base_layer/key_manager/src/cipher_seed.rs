// Copyright 2021. The Tari Project
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

use std::{convert::TryFrom, mem::size_of};

use argon2::{password_hash::SaltString, Argon2, PasswordHasher};
use arrayvec::ArrayVec;
use blake2::{digest::VariableOutput, VarBlake2b};
use chacha20::{
    cipher::{NewCipher, StreamCipher},
    ChaCha20,
    Key,
    Nonce,
};
use crc32fast::Hasher as CrcHasher;
use digest::Update;
use rand::{rngs::OsRng, RngCore};
use tari_crypto::tari_utilities::ByteArray;

use crate::{
    error::KeyManagerError,
    mnemonic::{from_bytes, to_bytes, to_bytes_with_language, Mnemonic, MnemonicLanguage},
};

const CIPHER_SEED_VERSION: u8 = 0u8;
pub const DEFAULT_CIPHER_SEED_PASSPHRASE: &str = "TARI_CIPHER_SEED";
pub const CIPHER_SEED_ENTROPY_BYTES: usize = 16;
pub const CIPHER_SEED_SALT_BYTES: usize = 5;
pub const CIPHER_SEED_MAC_BYTES: usize = 5;

/// This is an implementation of a Cipher Seed based on the `aezeed` encoding scheme (https://github.com/lightningnetwork/lnd/tree/master/aezeed)
/// The goal of the scheme is produce a wallet seed that is versioned, contains the birthday of the wallet,
/// starting entropy of the wallet to seed key generation, can be enciphered with a passphrase and has a checksum.
/// The `aezeed` scheme uses a new AEZ AEAD scheme which allows for enciphering arbitrary length texts and choosing
/// custom MAC sizes. AEZ is unfortunately not available in the RustCrypto implementations yet so we use a similar
/// AEAD scheme using the primitives available in RustCrypto.
/// Our scheme must be able to be represented with the 24 word seed phrase using the BIP-39 word lists. The world
/// lists contain 2048 words which are 11 bits of information giving us a total of 33 bytes to work with for the
/// final encoding.
/// In our scheme we will have the following data:
/// version     1 byte
/// birthday    2 bytes     Days since Unix Epoch
/// entropy     16 bytes
/// MAC         5 bytes     Hash(birthday||entropy||version||salt||passphrase)
/// salt        5 bytes
/// checksum    4 bytes
///
/// In its enciphered form we will use the MAC-the-Encrypt pattern of AE so that the birthday and entropy will be
/// encrypted. The version and salt are associated data that are included in the MAC but not encrypted.
/// The enciphered data will look as follows:
/// version     1 byte
/// ciphertext  23 bytes
/// salt        5 bytes
/// checksum    4 bytes
///
/// The final 33 byte enciphered data is what will be encoded using the Mnemonic Word lists to create a 24 word
/// seed phrase.
///
/// The checksum allows us to confirm that a given seed phrase decodes into an intact enciphered CipherSeed.
/// The MAC allows us to confirm that a given passphrase correctly decrypts the CipherSeed and that the version and
/// salt are not tampered with. If no passphrase is provided a default string will be used
///
/// The Birthday is included to enable more efficient recoveries. Knowing the birthday of the seed phrase means we
/// only have to scan the blocks in the chain since that day for full recovery, rather than scanning the entire
/// blockchain.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CipherSeed {
    version: u8,
    birthday: u16,
    entropy: [u8; CIPHER_SEED_ENTROPY_BYTES],
    salt: [u8; CIPHER_SEED_SALT_BYTES],
}

impl CipherSeed {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new() -> Self {
        const SECONDS_PER_DAY: u64 = 24 * 60 * 60;
        let days = chrono::Utc::now().timestamp() as u64 / SECONDS_PER_DAY;
        let birthday = u16::try_from(days).unwrap_or(0u16);
        CipherSeed::new_with_birthday(birthday)
    }

    #[cfg(target_arch = "wasm32")]
    pub fn new() -> Self {
        const MILLISECONDS_PER_DAY: u64 = 24 * 60 * 60 * 1000;
        let millis = js_sys::Date::now() as u64;
        let days = millis / MILLISECONDS_PER_DAY;
        let birthday = u16::try_from(days).unwrap_or(0u16);
        CipherSeed::new_with_birthday(birthday)
    }

    fn new_with_birthday(birthday: u16) -> Self {
        let mut entropy = [0u8; CIPHER_SEED_ENTROPY_BYTES];
        OsRng.fill_bytes(&mut entropy);
        let mut salt = [0u8; CIPHER_SEED_SALT_BYTES];
        OsRng.fill_bytes(&mut salt);

        Self {
            version: CIPHER_SEED_VERSION,
            birthday,
            entropy,
            salt,
        }
    }

    pub fn encipher(&self, passphrase: Option<String>) -> Result<Vec<u8>, KeyManagerError> {
        let mut plaintext = self.birthday.to_le_bytes().to_vec();
        plaintext.append(&mut self.entropy().clone().to_vec());

        let passphrase = passphrase.unwrap_or_else(|| DEFAULT_CIPHER_SEED_PASSPHRASE.to_string());

        // Construct HMAC and include the version and salt as Associated Data
        let blake2_mac_hasher: VarBlake2b =
            VarBlake2b::new(CIPHER_SEED_MAC_BYTES).expect("Should be able to create blake2 hasher");
        let mut hmac = [0u8; CIPHER_SEED_MAC_BYTES];
        blake2_mac_hasher
            .chain(plaintext.clone())
            .chain([CIPHER_SEED_VERSION])
            .chain(self.salt)
            .chain(passphrase.as_bytes())
            .finalize_variable(|res| hmac.copy_from_slice(res));

        plaintext.append(&mut hmac.to_vec());

        Self::apply_stream_cipher(&mut plaintext, &passphrase, &self.salt)?;

        let mut final_seed = vec![CIPHER_SEED_VERSION];
        final_seed.append(&mut plaintext.to_vec());
        final_seed.append(&mut self.salt.to_vec());

        let mut crc_hasher = CrcHasher::new();
        crc_hasher.update(final_seed.as_slice());
        let checksum = crc_hasher.finalize();
        final_seed.append(&mut checksum.to_le_bytes().to_vec());
        Ok(final_seed)
    }

    pub fn from_enciphered_bytes(enciphered_bytes: &[u8], passphrase: Option<String>) -> Result<Self, KeyManagerError> {
        // 1 byte Version || 2 byte Birthday || 16 byte Entropy || 5 byte MAC || 5 byte salt || 4 byte CRC32
        if enciphered_bytes.len() != 7 + CIPHER_SEED_ENTROPY_BYTES + CIPHER_SEED_SALT_BYTES + CIPHER_SEED_MAC_BYTES {
            return Err(KeyManagerError::InvalidData);
        }

        if enciphered_bytes[0] != CIPHER_SEED_VERSION {
            return Err(KeyManagerError::VersionMismatch);
        }

        let passphrase = passphrase.unwrap_or_else(|| DEFAULT_CIPHER_SEED_PASSPHRASE.to_string());

        let mut body = enciphered_bytes.to_owned();
        // extract 32 bit checksum
        let checksum_vec = body.split_off(body.len() - 4);

        let mut crc_hasher = CrcHasher::new();
        crc_hasher.update(body.as_slice());

        let calculated_checksum = crc_hasher.finalize();

        let mut checksum_bytes: [u8; 4] = [0u8; 4];
        checksum_bytes.copy_from_slice(&checksum_vec[..4]);
        let checksum = u32::from_le_bytes(checksum_bytes);

        if calculated_checksum != checksum {
            return Err(KeyManagerError::CrcError);
        }

        let salt = body.split_off(body.len() - CIPHER_SEED_SALT_BYTES);
        let mut enciphered_seed = body.split_off(1);
        let received_version = body[0];

        Self::apply_stream_cipher(&mut enciphered_seed, &passphrase, salt.as_slice())?;

        let decrypted_hmac = enciphered_seed.split_off(enciphered_seed.len() - CIPHER_SEED_MAC_BYTES);

        let decrypted_entropy_vec: ArrayVec<_, CIPHER_SEED_ENTROPY_BYTES> =
            enciphered_seed.split_off(2).into_iter().collect();
        let decrypted_entropy = decrypted_entropy_vec
            .into_inner()
            .map_err(|_| KeyManagerError::InvalidData)?;

        let mut birthday_bytes: [u8; 2] = [0u8; 2];
        birthday_bytes.copy_from_slice(&enciphered_seed);
        let decrypted_birthday = u16::from_le_bytes(birthday_bytes);

        let blake2_mac_hasher: VarBlake2b =
            VarBlake2b::new(CIPHER_SEED_MAC_BYTES).expect("Should be able to create blake2 hasher");
        let mut hmac = [0u8; CIPHER_SEED_MAC_BYTES];
        blake2_mac_hasher
            .chain(&birthday_bytes)
            .chain(&decrypted_entropy)
            .chain([CIPHER_SEED_VERSION])
            .chain(salt.as_slice())
            .chain(passphrase.as_bytes())
            .finalize_variable(|res| hmac.copy_from_slice(res));

        if decrypted_hmac != hmac.to_vec() {
            return Err(KeyManagerError::DecryptionFailed);
        }

        let salt_vec: ArrayVec<_, CIPHER_SEED_SALT_BYTES> = salt.into_iter().collect();
        let salt_bytes = salt_vec.into_inner().map_err(|_| KeyManagerError::InvalidData)?;

        Ok(Self {
            version: received_version,
            birthday: decrypted_birthday,
            entropy: decrypted_entropy,
            salt: salt_bytes,
        })
    }

    fn apply_stream_cipher(data: &mut Vec<u8>, passphrase: &str, salt: &[u8]) -> Result<(), KeyManagerError> {
        let argon2 = Argon2::default();
        let blake2_nonce_hasher: VarBlake2b =
            VarBlake2b::new(size_of::<Nonce>()).expect("Should be able to create blake2 hasher");

        let mut encryption_nonce = [0u8; size_of::<Nonce>()];
        blake2_nonce_hasher
            .chain(salt)
            .finalize_variable(|res| encryption_nonce.copy_from_slice(res));
        let nonce_ga = Nonce::from_slice(&encryption_nonce);

        // Create salt string stretched to the chacha nonce size, we only have space for 5 bytes of salt in the seed but
        // will use key stretching to produce a longer nonce for the passphrase hash and the encryption nonce.
        let salt_b64 = SaltString::b64_encode(&encryption_nonce)?;

        let derived_encryption_key = argon2
            .hash_password_simple(passphrase.as_bytes(), salt_b64.as_str())?
            .hash
            .ok_or_else(|| KeyManagerError::CryptographicError("Problem generating encryption key hash".to_string()))?;
        let key = Key::from_slice(derived_encryption_key.as_bytes());
        let mut cipher = ChaCha20::new(key, nonce_ga);
        cipher.apply_keystream(data.as_mut_slice());

        Ok(())
    }

    pub fn entropy(&self) -> [u8; CIPHER_SEED_ENTROPY_BYTES] {
        self.entropy
    }

    pub fn birthday(&self) -> u16 {
        self.birthday
    }
}

impl Drop for CipherSeed {
    fn drop(&mut self) {
        use clear_on_drop::clear::Clear;
        Clear::clear(&mut self.entropy);
    }
}

impl Default for CipherSeed {
    fn default() -> Self {
        Self::new()
    }
}

impl Mnemonic<CipherSeed> for CipherSeed {
    /// Generates a CipherSeed that represent the provided mnemonic sequence of words, the language of the mnemonic
    /// sequence is autodetected
    fn from_mnemonic(mnemonic_seq: &[String], passphrase: Option<String>) -> Result<CipherSeed, KeyManagerError> {
        let bytes = to_bytes(mnemonic_seq)?;
        CipherSeed::from_enciphered_bytes(&bytes, passphrase)
    }

    /// Generates a SecretKey that represent the provided mnemonic sequence of words using the specified language
    fn from_mnemonic_with_language(
        mnemonic_seq: &[String],
        language: &MnemonicLanguage,
        passphrase: Option<String>,
    ) -> Result<CipherSeed, KeyManagerError> {
        let bytes = to_bytes_with_language(mnemonic_seq, language)?;
        CipherSeed::from_enciphered_bytes(&bytes, passphrase)
    }

    /// Generates a mnemonic sequence of words from the provided secret key
    fn to_mnemonic(
        &self,
        language: &MnemonicLanguage,
        passphrase: Option<String>,
    ) -> Result<Vec<String>, KeyManagerError> {
        Ok(from_bytes(self.encipher(passphrase)?, language)?)
    }
}

#[cfg(test)]
mod test {
    use crate::{
        cipher_seed::CipherSeed,
        error::KeyManagerError,
        mnemonic::{Mnemonic, MnemonicLanguage},
    };

    #[test]
    fn test_cipher_seed_generation_and_deciphering() {
        let seed = CipherSeed::new();

        let mut enciphered_seed = seed.encipher(Some("Passphrase".to_string())).unwrap();

        let deciphered_seed =
            CipherSeed::from_enciphered_bytes(&enciphered_seed, Some("Passphrase".to_string())).unwrap();
        assert_eq!(seed, deciphered_seed);

        match CipherSeed::from_enciphered_bytes(&enciphered_seed, Some("WrongPassphrase".to_string())) {
            Err(KeyManagerError::DecryptionFailed) => (),
            _ => panic!("Version should not match"),
        }

        enciphered_seed[0] = 1;

        match CipherSeed::from_enciphered_bytes(&enciphered_seed, Some("Passphrase".to_string())) {
            Err(KeyManagerError::VersionMismatch) => (),
            _ => panic!("Version should not match"),
        }

        enciphered_seed[0] = 0;
        // Prevent the 1 our 256 chances that it was already a zero
        if enciphered_seed[1] == 0 {
            enciphered_seed[1] = 1;
        } else {
            enciphered_seed[1] = 0;
        }
        match CipherSeed::from_enciphered_bytes(&enciphered_seed, Some("Passphrase".to_string())) {
            Err(KeyManagerError::CrcError) => (),
            _ => panic!("Crc should not match"),
        }
    }

    #[test]
    fn test_cipher_seed_to_mnemonic_and_from_mnemonic() {
        // Valid Mnemonic sequence
        let seed = CipherSeed::new();
        match seed.to_mnemonic(&MnemonicLanguage::Japanese, None) {
            Ok(mnemonic_seq) => {
                match CipherSeed::from_mnemonic(&mnemonic_seq, None) {
                    Ok(mnemonic_seed) => assert_eq!(seed, mnemonic_seed),
                    Err(e) => panic!("Couldn't create CipherSeed from Mnemonic: {}", e),
                }
                // Language known
                match CipherSeed::from_mnemonic_with_language(&mnemonic_seq, &MnemonicLanguage::Japanese, None) {
                    Ok(mnemonic_seed) => assert_eq!(seed, mnemonic_seed),
                    Err(_e) => panic!("Couldn't create CipherSeed from Mnemonic with Language"),
                }
            },
            Err(_e) => panic!("Couldn't convert CipherSeed to Mnemonic"),
        }
        // Invalid Mnemonic sequence
        let mnemonic_seq = vec![
            "stay", "what", "minor", "stay", "olive", "clip", "buyer", "know", "report", "obey", "pen", "door", "type",
            "cover", "vote", "federal", "husband", "cave", "alone", "dynamic", "reopen", "visa", "young", "gas",
        ]
        .iter()
        .map(|x| x.to_string())
        .collect::<Vec<String>>();
        // Language not known
        match CipherSeed::from_mnemonic(&mnemonic_seq, None) {
            Ok(_k) => panic!(),
            Err(_e) => {},
        }
        // Language known
        match CipherSeed::from_mnemonic_with_language(&mnemonic_seq, &MnemonicLanguage::Japanese, None) {
            Ok(_k) => panic!(),
            Err(_e) => {},
        }
    }

    #[test]
    fn cipher_seed_to_and_from_mnemonic_with_passphrase() {
        let seed = CipherSeed::new();
        match seed.to_mnemonic(&MnemonicLanguage::Spanish, Some("Passphrase".to_string())) {
            Ok(mnemonic_seq) => match CipherSeed::from_mnemonic(&mnemonic_seq, Some("Passphrase".to_string())) {
                Ok(mnemonic_seed) => assert_eq!(seed, mnemonic_seed),
                Err(e) => panic!("Couldn't create CipherSeed from Mnemonic: {}", e),
            },
            Err(_e) => panic!("Couldn't convert CipherSeed to Mnemonic"),
        }

        match seed.to_mnemonic(&MnemonicLanguage::Spanish, Some("Passphrase".to_string())) {
            Ok(mnemonic_seq) => {
                assert!(
                    !CipherSeed::from_mnemonic(&mnemonic_seq, Some("WrongPassphrase".to_string())).is_ok(),
                    "Should not be able to derive seed with wrong passphrase"
                );
            },
            Err(_e) => panic!("Couldn't convert CipherSeed to Mnemonic"),
        }
    }
}
