// Copyright 2021. The Taiji Project
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

use std::{convert::TryFrom, mem::size_of, str::FromStr};

use argon2;
use blake2::Blake2b;
use chacha20::{
    cipher::{NewCipher, StreamCipher},
    ChaCha20,
    Key,
    Nonce,
};
use crc32fast::Hasher as CrcHasher;
use digest::consts::U32;
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;
use tari_utilities::{hidden::Hidden, safe_array::SafeArray, SafePassword};
use zeroize::{Zeroize, Zeroizing};

use crate::{
    error::KeyManagerError,
    mac_domain_hasher,
    mnemonic::{from_bytes, to_bytes, to_bytes_with_language, Mnemonic, MnemonicLanguage},
    CipherSeedEncryptionKey,
    CipherSeedMacKey,
    SeedWords,
    LABEL_ARGON_ENCODING,
    LABEL_CHACHA20_ENCODING,
    LABEL_MAC_GENERATION,
};

// The version should be incremented for any breaking change to the format
// History:
// 0: initial version
// 1: fixed incorrect key derivation and birthday genesis
const CIPHER_SEED_VERSION: u8 = 1u8;

pub const BIRTHDAY_GENESIS_FROM_UNIX_EPOCH: u64 = 1640995200; // seconds to 2022-01-01 00:00:00 UTC
pub const DEFAULT_CIPHER_SEED_PASSPHRASE: &str = "TARI_CIPHER_SEED"; // the default passphrase if none is supplied

// Fixed sizes (all in bytes)
pub const CIPHER_SEED_BIRTHDAY_BYTES: usize = 2;
pub const CIPHER_SEED_ENTROPY_BYTES: usize = 16;
pub const CIPHER_SEED_MAIN_SALT_BYTES: usize = 5;
pub const ARGON2_SALT_BYTES: usize = 16;
pub const CIPHER_SEED_MAC_BYTES: usize = 5;
pub const CIPHER_SEED_ENCRYPTION_KEY_BYTES: usize = 32;
pub const CIPHER_SEED_MAC_KEY_BYTES: usize = 32;
pub const CIPHER_SEED_CHECKSUM_BYTES: usize = 4;

/// This is an implementation of a Cipher Seed based on the `aezeed` encoding scheme:
/// https://github.com/lightningnetwork/lnd/tree/master/aezeed
/// The goal of the scheme is produce a wallet seed that is versioned, contains the birthday of the wallet,
/// starting entropy of the wallet to seed key generation, can be enciphered with a passphrase and has a checksum.
/// The `aezeed` scheme uses a new AEZ AEAD scheme which allows for enciphering arbitrary length texts and choosing
/// custom MAC sizes. AEZ is unfortunately not available in the RustCrypto implementations yet so we use a similar
/// AEAD scheme using the primitives available in RustCrypto.
/// Our scheme must be able to be represented with the 24 word seed phrase using the BIP-39 word lists. The word
/// lists contain 2048 words which are 11 bits of information giving us a total of 33 bytes to work with for the
/// final encoding.
/// In our scheme we will have the following data:
/// version     1 byte
/// birthday    2 bytes     Days since fixed genesis point
/// entropy     16 bytes
/// MAC         5 bytes     Hash(birthday||entropy||version||salt||passphrase)
/// salt        5 bytes
/// checksum    4 bytes     CRC32
///
/// In its enciphered form we will use the MAC-the-Encrypt pattern of AE so that the birthday and entropy will be
/// encrypted.
///
/// It is important to note that we don't generate the MAC directly from the provided low entropy passphrase.
/// Instead, the intent is to use a password-based key derivation function to generate a derived key of higher
/// effective entropy through the use of a carefully-designed function like Argon2 that's built for this purpose.
/// The corresponding derived key has output of length 64-bytes, and we use the first and last 32-bytes for
/// the MAC and ChaCha20 encryption. In such way, we follow the motto of not reusing the same derived keys more
/// than once. Another key ingredient in our approach is the use of domain separation, via the current hashing API.
/// See https://github.com/taiji-project/taiji/issues/4182 for more information.
///
/// The version and salt are associated data that are included in the MAC but not encrypted.
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
/// salt are not tampered with. If no passphrase is provided a default string will be used.
///
/// The birthday is included to enable more efficient recoveries. Knowing the birthday of the seed phrase means we
/// only have to scan the blocks in the chain since that day for full recovery, rather than scanning the entire
/// blockchain.

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Zeroize)]
#[zeroize(drop)]
pub struct CipherSeed {
    version: u8,
    birthday: u16,
    entropy: Box<[u8; CIPHER_SEED_ENTROPY_BYTES]>,
    salt: Box<[u8; CIPHER_SEED_MAIN_SALT_BYTES]>,
}

// This is a separate type to make the linter happy
type DerivedCipherSeedKeys = Result<(CipherSeedEncryptionKey, CipherSeedMacKey), KeyManagerError>;

impl CipherSeed {
    #[cfg(not(target_arch = "wasm32"))]
    /// Generate a new seed
    pub fn new() -> Self {
        use std::time::{Duration, SystemTime, UNIX_EPOCH};
        const SECONDS_PER_DAY: u64 = 24 * 60 * 60;
        let birthday_genesis_date = UNIX_EPOCH + Duration::from_secs(BIRTHDAY_GENESIS_FROM_UNIX_EPOCH);
        let days = SystemTime::now()
            .duration_since(birthday_genesis_date)
            .unwrap()
            .as_secs() /
            SECONDS_PER_DAY;
        let birthday = u16::try_from(days).unwrap_or(0u16);
        CipherSeed::new_with_birthday(birthday)
    }

    #[cfg(target_arch = "wasm32")]
    /// Generate a new seed
    pub fn new() -> Self {
        const MILLISECONDS_PER_DAY: u64 = 24 * 60 * 60 * 1000;
        let millis = js_sys::Date::now() as u64;
        let days = millis / MILLISECONDS_PER_DAY;
        let birthday = u16::try_from(days).unwrap_or(0u16);
        CipherSeed::new_with_birthday(birthday)
    }

    /// Generate a new seed with a given birthday
    fn new_with_birthday(birthday: u16) -> Self {
        let mut entropy = Box::new([0u8; CIPHER_SEED_ENTROPY_BYTES]);
        OsRng.fill_bytes(entropy.as_mut());
        let mut salt = Box::new([0u8; CIPHER_SEED_MAIN_SALT_BYTES]);
        OsRng.fill_bytes(salt.as_mut());

        Self {
            version: CIPHER_SEED_VERSION,
            birthday,
            entropy,
            salt,
        }
    }

    /// Generate an encrypted seed from a passphrase
    pub fn encipher(&self, passphrase: Option<SafePassword>) -> Result<Vec<u8>, KeyManagerError> {
        // Derive encryption and MAC keys from passphrase and main salt
        let passphrase = passphrase.unwrap_or_else(|| {
            SafePassword::from_str(DEFAULT_CIPHER_SEED_PASSPHRASE)
                .expect("Failed to parse default cipher seed passphrase to SafePassword")
        });
        let (encryption_key, mac_key) = Self::derive_keys(&passphrase, self.salt.as_ref())?;

        // Generate the MAC
        let mac = Self::generate_mac(
            &self.birthday.to_le_bytes(),
            self.entropy.as_ref(),
            CIPHER_SEED_VERSION,
            self.salt.as_ref(),
            &mac_key,
        )?;

        // Assemble the secret data to be encrypted: birthday, entropy, MAC
        let mut secret_data = Zeroizing::new(Vec::<u8>::with_capacity(
            CIPHER_SEED_BIRTHDAY_BYTES + CIPHER_SEED_ENTROPY_BYTES + CIPHER_SEED_MAC_BYTES,
        ));
        secret_data.extend(self.birthday.to_le_bytes());
        secret_data.extend(self.entropy.iter());
        secret_data.extend(&mac);

        // Encrypt the secret data
        Self::apply_stream_cipher(&mut secret_data, &encryption_key, self.salt.as_ref())?;

        // Assemble the final seed: version, main salt, secret data, checksum
        let mut encrypted_seed =
            Vec::<u8>::with_capacity(1 + CIPHER_SEED_MAIN_SALT_BYTES + secret_data.len() + CIPHER_SEED_CHECKSUM_BYTES);
        encrypted_seed.push(CIPHER_SEED_VERSION);
        encrypted_seed.extend(secret_data.iter());
        encrypted_seed.extend(self.salt.iter());

        let mut crc_hasher = CrcHasher::new();
        crc_hasher.update(encrypted_seed.as_slice());
        let checksum = crc_hasher.finalize().to_le_bytes();
        encrypted_seed.extend(checksum);

        Ok(encrypted_seed)
    }

    /// Recover a seed from encrypted data and a passphrase
    pub fn from_enciphered_bytes(
        encrypted_seed: &[u8],
        passphrase: Option<SafePassword>,
    ) -> Result<Self, KeyManagerError> {
        // Check the length: version, birthday, entropy, MAC, salt, checksum
        if encrypted_seed.len() !=
            1 + CIPHER_SEED_BIRTHDAY_BYTES +
                CIPHER_SEED_ENTROPY_BYTES +
                CIPHER_SEED_MAC_BYTES +
                CIPHER_SEED_MAIN_SALT_BYTES +
                CIPHER_SEED_CHECKSUM_BYTES
        {
            return Err(KeyManagerError::InvalidData);
        }

        // We only support one version right now
        let version = encrypted_seed[0];
        if version != CIPHER_SEED_VERSION {
            return Err(KeyManagerError::VersionMismatch);
        }

        let mut encrypted_seed = encrypted_seed.to_owned();

        // Verify the checksum first, to detect obvious errors
        let checksum = encrypted_seed.split_off(
            1 + CIPHER_SEED_BIRTHDAY_BYTES +
                CIPHER_SEED_ENTROPY_BYTES +
                CIPHER_SEED_MAC_BYTES +
                CIPHER_SEED_MAIN_SALT_BYTES,
        );
        let mut crc_hasher = CrcHasher::new();
        crc_hasher.update(encrypted_seed.as_slice());
        let expected_checksum = crc_hasher.finalize().to_le_bytes();
        if checksum != expected_checksum {
            return Err(KeyManagerError::CrcError);
        }

        // Derive encryption and MAC keys from passphrase and main salt
        let passphrase = passphrase.unwrap_or_else(|| {
            SafePassword::from_str(DEFAULT_CIPHER_SEED_PASSPHRASE)
                .expect("Failed to parse default cipher seed passphrase to SafePassword")
        });
        let salt: Box<[u8; CIPHER_SEED_MAIN_SALT_BYTES]> = encrypted_seed
            .split_off(1 + CIPHER_SEED_BIRTHDAY_BYTES + CIPHER_SEED_ENTROPY_BYTES + CIPHER_SEED_MAC_BYTES)
            .into_boxed_slice()
            .try_into()
            .map_err(|_| KeyManagerError::InvalidData)?;
        let (encryption_key, mac_key) = Self::derive_keys(&passphrase, salt.as_ref())?;

        // Decrypt the secret data: birthday, entropy, MAC
        let mut secret_data = Zeroizing::new(encrypted_seed.split_off(1));
        Self::apply_stream_cipher(&mut secret_data, &encryption_key, salt.as_ref())?;

        // Parse secret data
        let mac = secret_data.split_off(CIPHER_SEED_BIRTHDAY_BYTES + CIPHER_SEED_ENTROPY_BYTES);
        let entropy: Hidden<[u8; CIPHER_SEED_ENTROPY_BYTES]> = Hidden::hide(
            secret_data
                .split_off(CIPHER_SEED_BIRTHDAY_BYTES)
                .try_into()
                .map_err(|_| KeyManagerError::InvalidData)?,
        ); // wrapped in case of MAC failure
        let mut birthday_bytes = [0u8; CIPHER_SEED_BIRTHDAY_BYTES];
        birthday_bytes.copy_from_slice(&secret_data);
        let birthday = u16::from_le_bytes(birthday_bytes);

        // Generate the MAC
        let expected_mac = Self::generate_mac(&birthday_bytes, entropy.reveal(), version, salt.as_ref(), &mac_key)?;

        // Verify the MAC in constant time to avoid leaking data
        if mac.ct_eq(&expected_mac).unwrap_u8() == 0 {
            return Err(KeyManagerError::DecryptionFailed);
        }

        Ok(Self {
            version,
            birthday,
            entropy: Box::from(*entropy.reveal()),
            salt,
        })
    }

    /// Encrypt or decrypt data using ChaCha20
    fn apply_stream_cipher(
        data: &mut [u8],
        encryption_key: &CipherSeedEncryptionKey,
        salt: &[u8],
    ) -> Result<(), KeyManagerError> {
        // The ChaCha20 nonce is derived from the main salt
        let encryption_nonce = mac_domain_hasher::<Blake2b<U32>>(LABEL_CHACHA20_ENCODING)
            .chain(salt)
            .finalize();
        let encryption_nonce = &encryption_nonce.as_ref()[..size_of::<Nonce>()];

        // Encrypt/decrypt the data
        let mut cipher = ChaCha20::new(
            Key::from_slice(encryption_key.reveal()),
            Nonce::from_slice(encryption_nonce),
        );
        cipher.apply_keystream(data);

        Ok(())
    }

    /// Get a reference to the seed entropy
    pub fn entropy(&self) -> &[u8] {
        self.entropy.as_ref()
    }

    /// Get the seed birthday
    pub fn birthday(&self) -> u16 {
        self.birthday
    }

    /// Generate a MAC using Blake2b
    fn generate_mac(
        birthday: &[u8],
        entropy: &[u8],
        cipher_seed_version: u8,
        salt: &[u8],
        mac_key: &CipherSeedMacKey,
    ) -> Result<Vec<u8>, KeyManagerError> {
        // Check all lengths are valid
        if birthday.len() != CIPHER_SEED_BIRTHDAY_BYTES {
            return Err(KeyManagerError::InvalidData);
        }
        if entropy.len() != CIPHER_SEED_ENTROPY_BYTES {
            return Err(KeyManagerError::InvalidData);
        }
        if salt.len() != CIPHER_SEED_MAIN_SALT_BYTES {
            return Err(KeyManagerError::InvalidData);
        }

        Ok(mac_domain_hasher::<Blake2b<U32>>(LABEL_MAC_GENERATION)
            .chain(birthday)
            .chain(entropy)
            .chain([cipher_seed_version])
            .chain(salt)
            .chain(mac_key.reveal())
            .finalize()
            .as_ref()[..CIPHER_SEED_MAC_BYTES]
            .to_vec())
    }

    /// Use Argon2 to derive encryption and MAC keys from a passphrase and main salt
    fn derive_keys(passphrase: &SafePassword, salt: &[u8]) -> DerivedCipherSeedKeys {
        // The Argon2 salt is derived from the main salt
        let argon2_salt = mac_domain_hasher::<Blake2b<U32>>(LABEL_ARGON_ENCODING)
            .chain(salt)
            .finalize();
        let argon2_salt = &argon2_salt.as_ref()[..ARGON2_SALT_BYTES];

        // Run Argon2 with enough output to accommodate both keys, so we only run it once
        // We use the recommended OWASP parameters for this:
        // https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html#argon2id
        let params = argon2::Params::new(
            46 * 1024, // m-cost should be 46 MiB = 46 * 1024 KiB
            1,         // t-cost
            1,         // p-cost
            Some(CIPHER_SEED_ENCRYPTION_KEY_BYTES + CIPHER_SEED_MAC_KEY_BYTES),
        )
        .map_err(|_| KeyManagerError::CryptographicError("Problem generating Argon2 parameters".to_string()))?;

        // Derive the main key from the password in place
        let mut main_key = Hidden::hide([0u8; CIPHER_SEED_ENCRYPTION_KEY_BYTES + CIPHER_SEED_MAC_KEY_BYTES]);
        let hasher = argon2::Argon2::new(argon2::Algorithm::Argon2d, argon2::Version::V0x13, params);
        hasher
            .hash_password_into(passphrase.reveal(), argon2_salt, main_key.reveal_mut())
            .map_err(|_| KeyManagerError::CryptographicError("Problem generating Argon2 password hash".to_string()))?;

        // Split off the keys
        let mut encryption_key = CipherSeedEncryptionKey::from(SafeArray::default());
        encryption_key
            .reveal_mut()
            .copy_from_slice(&main_key.reveal()[..CIPHER_SEED_ENCRYPTION_KEY_BYTES]);

        let mut mac_key = CipherSeedMacKey::from(SafeArray::default());
        mac_key
            .reveal_mut()
            .copy_from_slice(&main_key.reveal()[CIPHER_SEED_ENCRYPTION_KEY_BYTES..]);

        Ok((encryption_key, mac_key))
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
    fn from_mnemonic(
        mnemonic_seq: &SeedWords,
        passphrase: Option<SafePassword>,
    ) -> Result<CipherSeed, KeyManagerError> {
        let bytes = to_bytes(mnemonic_seq)?;
        CipherSeed::from_enciphered_bytes(bytes.reveal(), passphrase)
    }

    /// Generates a SecretKey that represent the provided mnemonic sequence of words using the specified language
    fn from_mnemonic_with_language(
        mnemonic_seq: &SeedWords,
        language: MnemonicLanguage,
        passphrase: Option<SafePassword>,
    ) -> Result<CipherSeed, KeyManagerError> {
        let bytes = to_bytes_with_language(mnemonic_seq, &language)?;
        CipherSeed::from_enciphered_bytes(bytes.reveal(), passphrase)
    }

    /// Generates a mnemonic sequence of words from the provided secret key
    fn to_mnemonic(
        &self,
        language: MnemonicLanguage,
        passphrase: Option<SafePassword>,
    ) -> Result<SeedWords, KeyManagerError> {
        Ok(from_bytes(&self.encipher(passphrase)?, language)?)
    }
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use crc32fast::Hasher as CrcHasher;
    use tari_utilities::{Hidden, SafePassword};

    use super::BIRTHDAY_GENESIS_FROM_UNIX_EPOCH;
    use crate::{
        cipher_seed::{
            CipherSeed,
            CIPHER_SEED_BIRTHDAY_BYTES,
            CIPHER_SEED_CHECKSUM_BYTES,
            CIPHER_SEED_ENTROPY_BYTES,
            CIPHER_SEED_MAC_BYTES,
            CIPHER_SEED_VERSION,
        },
        error::KeyManagerError,
        get_birthday_from_unix_epoch_in_seconds,
        mnemonic::{Mnemonic, MnemonicLanguage},
        SeedWords,
    };

    #[test]
    fn test_cipher_seed_generation_and_deciphering() {
        let passphrase = "Passphrase";

        // Generate a new encrypted cipher seed
        let seed = CipherSeed::new();
        let enciphered_seed = seed
            .encipher(Some(SafePassword::from_str(passphrase).unwrap()))
            .unwrap();
        let n = enciphered_seed.len();

        // Decryption succeeds with the correct passphrase
        let deciphered_seed =
            CipherSeed::from_enciphered_bytes(&enciphered_seed, Some(SafePassword::from_str(passphrase).unwrap()))
                .unwrap();
        assert_eq!(seed, deciphered_seed);

        // Decryption fails with the wrong passphrase
        match CipherSeed::from_enciphered_bytes(
            &enciphered_seed,
            Some(SafePassword::from_str("WrongPassphrase").unwrap()),
        ) {
            Err(KeyManagerError::DecryptionFailed) => (),
            _ => panic!("Version should not match"),
        }

        // An unsupported version fails
        let mut malleated_seed = enciphered_seed.clone();
        malleated_seed[0] = CIPHER_SEED_VERSION + 1;
        match CipherSeed::from_enciphered_bytes(&malleated_seed, Some(SafePassword::from_str(passphrase).unwrap())) {
            Err(KeyManagerError::VersionMismatch) => (),
            _ => panic!("Version should not match"),
        }

        // Malleated ciphertext should fail the checksum
        const MALLEATED_BYTE: usize = 1;
        let mut malleated_seed = enciphered_seed.clone();
        malleated_seed[MALLEATED_BYTE] = !malleated_seed[MALLEATED_BYTE];
        match CipherSeed::from_enciphered_bytes(&malleated_seed, Some(SafePassword::from_str(passphrase).unwrap())) {
            Err(KeyManagerError::CrcError) => (),
            _ => panic!("Checksum should fail"),
        }

        // Malleate the birthday, but with a correct checksum
        const MALLEATED_BIRTHDAY_BYTE: usize = 1;
        let mut malleated_seed = enciphered_seed.clone();
        malleated_seed[MALLEATED_BIRTHDAY_BYTE] = !malleated_seed[MALLEATED_BIRTHDAY_BYTE];

        let mut crc_hasher = CrcHasher::new();
        crc_hasher.update(&malleated_seed[..(n - CIPHER_SEED_CHECKSUM_BYTES)]);
        let calculated_checksum: [u8; CIPHER_SEED_CHECKSUM_BYTES] = crc_hasher.finalize().to_le_bytes();

        malleated_seed[(n - CIPHER_SEED_CHECKSUM_BYTES)..].copy_from_slice(&calculated_checksum);

        match CipherSeed::from_enciphered_bytes(&malleated_seed, Some(SafePassword::from_str(passphrase).unwrap())) {
            Err(KeyManagerError::DecryptionFailed) => (),
            _ => panic!("Decryption should fail"),
        }

        // Malleate the entropy, but with a correct checksum
        const MALLEATED_ENTROPY_BYTE: usize = 1 + CIPHER_SEED_BIRTHDAY_BYTES;
        let mut malleated_seed = enciphered_seed.clone();
        malleated_seed[MALLEATED_ENTROPY_BYTE] = !malleated_seed[MALLEATED_ENTROPY_BYTE];

        let mut crc_hasher = CrcHasher::new();
        crc_hasher.update(&malleated_seed[..(n - CIPHER_SEED_CHECKSUM_BYTES)]);
        let calculated_checksum: [u8; CIPHER_SEED_CHECKSUM_BYTES] = crc_hasher.finalize().to_le_bytes();

        malleated_seed[(n - CIPHER_SEED_CHECKSUM_BYTES)..].copy_from_slice(&calculated_checksum);

        match CipherSeed::from_enciphered_bytes(&malleated_seed, Some(SafePassword::from_str(passphrase).unwrap())) {
            Err(KeyManagerError::DecryptionFailed) => (),
            _ => panic!("Decryption should fail"),
        }

        // Malleate the MAC, but with a correct checksum
        const MALLEATED_MAC_BYTE: usize = 1 + CIPHER_SEED_BIRTHDAY_BYTES + CIPHER_SEED_ENTROPY_BYTES;
        let mut malleated_seed = enciphered_seed.clone();
        malleated_seed[MALLEATED_MAC_BYTE] = !malleated_seed[MALLEATED_MAC_BYTE];

        let mut crc_hasher = CrcHasher::new();
        crc_hasher.update(&malleated_seed[..(n - CIPHER_SEED_CHECKSUM_BYTES)]);
        let calculated_checksum: [u8; CIPHER_SEED_CHECKSUM_BYTES] = crc_hasher.finalize().to_le_bytes();

        malleated_seed[(n - CIPHER_SEED_CHECKSUM_BYTES)..].copy_from_slice(&calculated_checksum);

        match CipherSeed::from_enciphered_bytes(&malleated_seed, Some(SafePassword::from_str(passphrase).unwrap())) {
            Err(KeyManagerError::DecryptionFailed) => (),
            _ => panic!("Decryption should fail"),
        }

        // Malleate the salt, but with a correct checksum
        const MALLEATED_SALT_BYTE: usize =
            1 + CIPHER_SEED_BIRTHDAY_BYTES + CIPHER_SEED_ENTROPY_BYTES + CIPHER_SEED_MAC_BYTES;
        let mut malleated_seed = enciphered_seed;
        malleated_seed[MALLEATED_SALT_BYTE] = !malleated_seed[MALLEATED_SALT_BYTE];

        let mut crc_hasher = CrcHasher::new();
        crc_hasher.update(&malleated_seed[..(n - CIPHER_SEED_CHECKSUM_BYTES)]);
        let calculated_checksum: [u8; CIPHER_SEED_CHECKSUM_BYTES] = crc_hasher.finalize().to_le_bytes();

        malleated_seed[(n - CIPHER_SEED_CHECKSUM_BYTES)..].copy_from_slice(&calculated_checksum);

        match CipherSeed::from_enciphered_bytes(&malleated_seed, Some(SafePassword::from_str(passphrase).unwrap())) {
            Err(KeyManagerError::DecryptionFailed) => (),
            _ => panic!("Decryption should fail"),
        }
    }

    #[test]
    fn test_cipher_seed_to_mnemonic_and_from_mnemonic() {
        // Valid Mnemonic sequence
        let seed = CipherSeed::new();
        let mnemonic_seq = seed
            .to_mnemonic(MnemonicLanguage::Japanese, None)
            .expect("Couldn't convert CipherSeed to Mnemonic");
        match CipherSeed::from_mnemonic(&mnemonic_seq, None) {
            Ok(mnemonic_seed) => assert_eq!(seed, mnemonic_seed),
            Err(e) => panic!("Couldn't create CipherSeed from Mnemonic: {}", e),
        }
        // Language known
        let mnemonic_seed = CipherSeed::from_mnemonic_with_language(&mnemonic_seq, MnemonicLanguage::Japanese, None)
            .expect("Couldn't create CipherSeed from Mnemonic with Language");
        assert_eq!(seed, mnemonic_seed);
        // Invalid Mnemonic sequence
        let mnemonic_seq = vec![
            "stay", "what", "minor", "stay", "olive", "clip", "buyer", "know", "report", "obey", "pen", "door", "type",
            "cover", "vote", "federal", "husband", "cave", "alone", "dynamic", "reopen", "visa", "young", "gas",
        ]
        .iter()
        .map(|x| Hidden::hide(x.to_string()))
        .collect::<Vec<Hidden<String>>>();
        let mnemonic_seq = SeedWords::new(mnemonic_seq);
        // Language not known
        match CipherSeed::from_mnemonic(&mnemonic_seq, None) {
            Ok(_k) => panic!(),
            Err(_e) => {},
        }
        // Language known
        match CipherSeed::from_mnemonic_with_language(&mnemonic_seq, MnemonicLanguage::Japanese, None) {
            Ok(_k) => panic!(),
            Err(_e) => {},
        }
    }

    #[test]
    fn cipher_seed_to_and_from_mnemonic_with_passphrase() {
        let seed = CipherSeed::new();
        let mnemonic_seq = seed
            .to_mnemonic(
                MnemonicLanguage::Spanish,
                Some(SafePassword::from_str("Passphrase").unwrap()),
            )
            .expect("Couldn't convert CipherSeed to Mnemonic");
        match CipherSeed::from_mnemonic(&mnemonic_seq, Some(SafePassword::from_str("Passphrase").unwrap())) {
            Ok(mnemonic_seed) => assert_eq!(seed, mnemonic_seed),
            Err(e) => panic!("Couldn't create CipherSeed from Mnemonic: {}", e),
        }

        let mnemonic_seq = seed
            .to_mnemonic(
                MnemonicLanguage::Spanish,
                Some(SafePassword::from_str("Passphrase").unwrap()),
            )
            .expect("Couldn't convert CipherSeed to Mnemonic");
        assert!(
            CipherSeed::from_mnemonic(&mnemonic_seq, Some(SafePassword::from_str("WrongPassphrase").unwrap())).is_err(),
            "Should not be able to derive seed with wrong passphrase"
        );
    }

    #[test]
    fn birthday_from_unix_epoch_works_for_zero_duration() {
        let birthday = 0u16;
        let to_days = 0u16;

        let birthday_genesis_time_in_seconds = get_birthday_from_unix_epoch_in_seconds(birthday, to_days);
        assert_eq!(birthday_genesis_time_in_seconds, BIRTHDAY_GENESIS_FROM_UNIX_EPOCH);
    }

    #[test]
    fn birthday_from_unix_epoch_works_for_large_to_days() {
        let birthday = 10u16;
        let to_days = 16u16;

        let birthday_genesis_time_in_seconds = get_birthday_from_unix_epoch_in_seconds(birthday, to_days);
        assert_eq!(birthday_genesis_time_in_seconds, BIRTHDAY_GENESIS_FROM_UNIX_EPOCH);
    }

    #[test]
    fn birthday_from_unix_epoch_works_generally() {
        let birthday = 100u16;
        let to_days = 20u16;

        let birthday_genesis_time_in_seconds = get_birthday_from_unix_epoch_in_seconds(birthday, to_days);
        assert_eq!(
            birthday_genesis_time_in_seconds,
            BIRTHDAY_GENESIS_FROM_UNIX_EPOCH + u64::from(birthday - to_days) * 24 * 60 * 60
        );
    }

    #[test]
    fn birthday_is_computed_correctly_from_new_wallet() {
        // birthday is at the half of the year 2022, namely 3th July 2022
        let cipher_seed = CipherSeed::new_with_birthday(183u16);
        let birthday = cipher_seed.birthday;
        let birthday_from_unix_epoch = get_birthday_from_unix_epoch_in_seconds(birthday, 0u16);

        // 1656806400 corresponds to the duration, in seconds, from unix epoch
        // to 3th July 2022 00:00:00
        assert_eq!(birthday_from_unix_epoch, 1656806400);
    }
}
