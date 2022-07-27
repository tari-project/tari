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

use std::{
    convert::TryFrom,
    mem::size_of,
    time::{SystemTime, UNIX_EPOCH},
};

use argon2::{
    password_hash::{Salt, SaltString},
    Argon2,
    Params,
    PasswordHasher,
    Version,
};
use arrayvec::ArrayVec;
use chacha20::{
    cipher::{NewCipher, StreamCipher},
    ChaCha20,
    Key,
    Nonce,
};
use crc32fast::Hasher as CrcHasher;
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use tari_utilities::ByteArray;

use crate::{
    base_layer_key_manager_argon2_encoding,
    base_layer_key_manager_chacha20_encoding,
    base_layer_key_manager_mac_generation,
    error::KeyManagerError,
    mnemonic::{from_bytes, to_bytes, to_bytes_with_language, Mnemonic, MnemonicLanguage},
};

const CIPHER_SEED_VERSION: u8 = 0u8;
pub const DEFAULT_CIPHER_SEED_PASSPHRASE: &str = "TARI_CIPHER_SEED";
const ARGON2_SALT_BYTES: usize = 16;
pub const CIPHER_SEED_BIRTHDAY_BYTES: usize = 2;
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
/// encrypted.
///
/// It is important to note that we don't generate the MAC directly from the provided low entropy passphrase.
/// Instead, the intent is to use a password-based key derivation function to generate a derived key of higher
/// effective entropy through the use of a carefully-designed function like Argon2 that's built for this purpose.
/// The corresponding derived key has output of length 64-bytes, and we use the first and last 32-bytes for MAC and
/// ChaCha20 encryption, respectively. In such way, we follow the motto of not reusing the same derived keys more
/// than once. Another key ingredient in our approach is the use of domain separation, via the current hashing API.
/// See https://github.com/tari-project/tari/issues/4182 for more information.
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
/// salt are not tampered with. If no passphrase is provided a default string will be used
///
/// The Birthday is included to enable more efficient recoveries. Knowing the birthday of the seed phrase means we
/// only have to scan the blocks in the chain since that day for full recovery, rather than scanning the entire
/// blockchain.

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
        let days = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() / SECONDS_PER_DAY;
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

        // generate the current MAC
        let mut mac = Self::generate_mac(
            &self.birthday.to_le_bytes(),
            &self.entropy(),
            &[CIPHER_SEED_VERSION],
            &self.salt,
            passphrase.as_str(),
        )?;

        plaintext.append(&mut mac);

        // apply cipher stream
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

        // apply cipher stream
        Self::apply_stream_cipher(&mut enciphered_seed, &passphrase, salt.as_slice())?;

        let decrypted_mac = enciphered_seed.split_off(enciphered_seed.len() - CIPHER_SEED_MAC_BYTES);

        let decrypted_entropy_vec: ArrayVec<_, CIPHER_SEED_ENTROPY_BYTES> =
            enciphered_seed.split_off(2).into_iter().collect();
        let decrypted_entropy = decrypted_entropy_vec
            .into_inner()
            .map_err(|_| KeyManagerError::InvalidData)?;

        let mut birthday_bytes: [u8; CIPHER_SEED_BIRTHDAY_BYTES] = [0u8; CIPHER_SEED_BIRTHDAY_BYTES];
        birthday_bytes.copy_from_slice(&enciphered_seed);
        let decrypted_birthday = u16::from_le_bytes(birthday_bytes);

        // generate the MAC
        let mac = Self::generate_mac(
            &decrypted_birthday.to_le_bytes(),
            &decrypted_entropy,
            &[CIPHER_SEED_VERSION],
            salt.as_slice(),
            passphrase.as_str(),
        )?;

        if decrypted_mac != mac {
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
        // encryption nonce for ChaCha20 encryption, generated as a domain separated hash of the given salt. Following
        // https://libsodium.gitbook.io/doc/advanced/stream_ciphers/chacha20, as of the IEF variant, the produced encryption
        // nonce should be 96-bit long
        let encryption_nonce = &base_layer_key_manager_chacha20_encoding()
            .chain(salt)
            .finalize()
            .into_vec()[..size_of::<Nonce>()];

        let nonce_ga = Nonce::from_slice(encryption_nonce);

        // we take the last 32 bytes of the generated derived encryption key for ChaCha20 cipher, see documentation
        let derived_encryption_key = Self::generate_domain_separated_passphrase_hash(passphrase, salt)?[32..].to_vec();

        let key = Key::from_slice(derived_encryption_key.as_slice());
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

impl CipherSeed {
    fn generate_mac(
        birthday: &[u8],
        entropy: &[u8],
        cipher_seed_version: &[u8],
        salt: &[u8],
        passphrase: &str,
    ) -> Result<Vec<u8>, KeyManagerError> {
        // birthday should be 2 bytes long
        if birthday.len() != CIPHER_SEED_BIRTHDAY_BYTES {
            return Err(KeyManagerError::InvalidData);
        }

        // entropy should be 16 bytes long
        if entropy.len() != CIPHER_SEED_ENTROPY_BYTES {
            return Err(KeyManagerError::InvalidData);
        }

        // cipher_seed_version should be 1 byte long
        if cipher_seed_version.len() != 1 {
            return Err(KeyManagerError::InvalidData);
        }

        // salt should be 5 bytes long
        if salt.len() != CIPHER_SEED_SALT_BYTES {
            return Err(KeyManagerError::InvalidData);
        }

        // we take the first 32 bytes of the generated derived encryption key for MAC generation, see documentation
        let passphrase_key = Self::generate_domain_separated_passphrase_hash(passphrase, salt)?[..32].to_vec();

        let mac = base_layer_key_manager_mac_generation()
            .chain(birthday)
            .chain(entropy)
            .chain(cipher_seed_version)
            .chain(salt)
            .chain(passphrase_key.as_slice())
            .finalize()
            .into_vec();

        Ok(mac[..CIPHER_SEED_MAC_BYTES].to_vec())
    }

    fn generate_domain_separated_passphrase_hash(passphrase: &str, salt: &[u8]) -> Result<Vec<u8>, KeyManagerError> {
        let argon2 = Argon2::default();

        // we produce a domain separated hash of the given salt, for Argon2 encryption use. As suggested in
        // https://en.wikipedia.org/wiki/Argon2, we shall use a 16-byte length hash salt
        let argon2_salt = &base_layer_key_manager_argon2_encoding()
            .chain(salt)
            .finalize()
            .into_vec()[..ARGON2_SALT_BYTES];

        // produce a base64 salt string
        let argon2_salt = SaltString::b64_encode(argon2_salt)?;

        // to generate two 32-byte keys, we produce a 64-byte argon2 output, as the default output size
        // for argon is 32, we have to update its parameters accordingly

        // the following choice of parameters is based on
        // https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html#argon2id
        let params = Params {
            m_cost: 37 * 1024,       // m-cost should be 37 Mib = 37 * 1024 Kib
            t_cost: 1,               // t-cost
            p_cost: 1,               // p-cost
            output_size: 64,         // 64 bytes output size,
            version: Version::V0x13, // version
        };

        // Argon2id algorithm: https://docs.rs/argon2/0.2.4/argon2/enum.Algorithm.html#variant.Argon2id
        let algorithm = argon2::Algorithm::Argon2id;

        // generate the given derived encryption key
        let derived_encryption_key = argon2
            .hash_password(
                passphrase.as_bytes(),
                Some(algorithm.ident()),
                params,
                Salt::try_from(argon2_salt.as_str())?,
            )?
            .hash
            .ok_or_else(|| KeyManagerError::CryptographicError("Problem generating encryption key hash".to_string()))?;

        Ok(derived_encryption_key.as_bytes().into())
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
        language: MnemonicLanguage,
        passphrase: Option<String>,
    ) -> Result<CipherSeed, KeyManagerError> {
        let bytes = to_bytes_with_language(mnemonic_seq, &language)?;
        CipherSeed::from_enciphered_bytes(&bytes, passphrase)
    }

    /// Generates a mnemonic sequence of words from the provided secret key
    fn to_mnemonic(
        &self,
        language: MnemonicLanguage,
        passphrase: Option<String>,
    ) -> Result<Vec<String>, KeyManagerError> {
        Ok(from_bytes(&self.encipher(passphrase)?, language)?)
    }
}

#[cfg(test)]
mod test {
    use crc32fast::Hasher as CrcHasher;

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

        // recover correct version
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

        // the following consists of three tests in which checksum is correctly changed by adversary,
        // after changing either birthday, entropy and salt. The MAC decryption should fail in all these
        // three scenarios.

        // change birthday
        enciphered_seed[1] += 1;

        // clone the correct checksum
        let checksum: Vec<u8> = enciphered_seed[(enciphered_seed.len() - 4)..].to_vec().clone();

        // generate a new checksum that coincides with the modified value
        let mut crc_hasher = CrcHasher::new();
        crc_hasher.update(&enciphered_seed[..(enciphered_seed.len() - 4)]);

        let calculated_checksum: [u8; 4] = crc_hasher.finalize().to_le_bytes();

        // change checksum accordingly, from the viewpoint of an attacker
        let n = enciphered_seed.len();
        enciphered_seed[(n - 4)..].copy_from_slice(&calculated_checksum);

        // the MAC decryption should fail in this case
        match CipherSeed::from_enciphered_bytes(&enciphered_seed, Some("passphrase".to_string())) {
            Err(KeyManagerError::DecryptionFailed) => (),
            _ => panic!("Decryption should fail"),
        }

        // recover original data
        enciphered_seed[1] -= 1;
        enciphered_seed[(n - 4)..].copy_from_slice(&checksum[..]);

        // change entropy and repeat test

        enciphered_seed[5] += 1;

        // clone the correct checksum
        let checksum: Vec<u8> = enciphered_seed[(enciphered_seed.len() - 4)..].to_vec().clone();

        // generate a new checksum that coincides with the modified value
        let mut crc_hasher = CrcHasher::new();
        crc_hasher.update(&enciphered_seed[..(enciphered_seed.len() - 4)]);

        let calculated_checksum: [u8; 4] = crc_hasher.finalize().to_le_bytes();

        // change checksum accordingly, from the viewpoint of an attacker
        let n = enciphered_seed.len();
        enciphered_seed[(n - 4)..].copy_from_slice(&calculated_checksum);

        // the MAC decryption should fail in this case
        match CipherSeed::from_enciphered_bytes(&enciphered_seed, Some("passphrase".to_string())) {
            Err(KeyManagerError::DecryptionFailed) => (),
            _ => panic!("Decryption should fail"),
        }

        // recover original data
        enciphered_seed[5] -= 1;
        enciphered_seed[(n - 4)..].copy_from_slice(&checksum[..]);

        // change salt and repeat test
        enciphered_seed[26] += 1;

        // clone the correct checksum
        let checksum: Vec<u8> = enciphered_seed[(enciphered_seed.len() - 4)..].to_vec().clone();

        // generate a new checksum that coincides with the modified value
        let mut crc_hasher = CrcHasher::new();
        crc_hasher.update(&enciphered_seed[..(enciphered_seed.len() - 4)]);

        let calculated_checksum: [u8; 4] = crc_hasher.finalize().to_le_bytes();

        // change checksum accordingly, from the viewpoint of an attacker
        let n = enciphered_seed.len();
        enciphered_seed[(n - 4)..].copy_from_slice(&calculated_checksum);

        // the MAC decryption should fail in this case
        match CipherSeed::from_enciphered_bytes(&enciphered_seed, Some("passphrase".to_string())) {
            Err(KeyManagerError::DecryptionFailed) => (),
            _ => panic!("Decryption should fail"),
        }

        // recover original data
        enciphered_seed[26] -= 1;
        enciphered_seed[(n - 4)..].copy_from_slice(&checksum[..]);
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
        .map(|x| x.to_string())
        .collect::<Vec<String>>();
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
            .to_mnemonic(MnemonicLanguage::Spanish, Some("Passphrase".to_string()))
            .expect("Couldn't convert CipherSeed to Mnemonic");
        match CipherSeed::from_mnemonic(&mnemonic_seq, Some("Passphrase".to_string())) {
            Ok(mnemonic_seed) => assert_eq!(seed, mnemonic_seed),
            Err(e) => panic!("Couldn't create CipherSeed from Mnemonic: {}", e),
        }

        let mnemonic_seq = seed
            .to_mnemonic(MnemonicLanguage::Spanish, Some("Passphrase".to_string()))
            .expect("Couldn't convert CipherSeed to Mnemonic");
        assert!(
            CipherSeed::from_mnemonic(&mnemonic_seq, Some("WrongPassphrase".to_string())).is_err(),
            "Should not be able to derive seed with wrong passphrase"
        );
    }
}
