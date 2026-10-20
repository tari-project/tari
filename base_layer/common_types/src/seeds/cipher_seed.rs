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

use std::{fmt, mem::size_of, str::FromStr};

use blake2::Blake2b;
use chacha20::{
    ChaCha20,
    Key,
    Nonce,
    cipher::{NewCipher, StreamCipher},
};
use crc32fast::Hasher as CrcHasher;
use digest::consts::U32;
use rand::Rng;
use serde::{
    Deserialize,
    Deserializer,
    Serialize,
    Serializer,
    de::{self, MapAccess, Visitor},
};
use subtle::ConstantTimeEq;
use tari_crypto::hashing::DomainSeparatedHasher;
use tari_hashing::{KeyManagerDomain, ZeroizingFinalize};
use tari_utilities::{
    SafePassword,
    hex::{from_hex, to_hex},
    hidden::Hidden,
    hidden_type,
    safe_array::SafeArray,
};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::seeds::{
    HASHER_LABEL_CIPHER_SEED_ENCRYPTION_NONCE,
    HASHER_LABEL_CIPHER_SEED_MAC,
    HASHER_LABEL_CIPHER_SEED_PBKDF_SALT,
    error::CipherError,
    mnemonic::{Mnemonic, MnemonicLanguage, from_bytes, to_bytes, to_bytes_with_language},
    seed_words::SeedWords,
};

// The version should be incremented for any breaking change to the format
// NOTE: Versions 2 and 3 can be read; older versions cannot.
// History:
// 0: initial version
// 1: fixed incorrect key derivation and birthday genesis
// 2: updated hasher domain labels and MAC input ordering
// 3: Argon2id key derivation instead of Argon2d; written only when a user passphrase is given, so that seeds under
//    the default passphrase keep importing into wallets that only read version 2
const CIPHER_SEED_VERSION: u8 = 2u8;
const CIPHER_SEED_VERSION_ARGON2ID: u8 = 3u8;

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
pub const SECONDS_PER_DAY: u64 = 24 * 60 * 60;
const CIPHER_SEED_SECRET_DATA_BYTES: usize =
    CIPHER_SEED_BIRTHDAY_BYTES + CIPHER_SEED_ENTROPY_BYTES + CIPHER_SEED_MAC_BYTES;
const CIPHER_SEED_ENCIPHERED_BYTES: usize =
    1 + CIPHER_SEED_SECRET_DATA_BYTES + CIPHER_SEED_MAIN_SALT_BYTES + CIPHER_SEED_CHECKSUM_BYTES;

hidden_type!(CipherSeedEncryptionKey, SafeArray<u8, CIPHER_SEED_ENCRYPTION_KEY_BYTES>);
hidden_type!(CipherSeedMacKey, SafeArray< u8, CIPHER_SEED_MAC_KEY_BYTES>);

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
/// salt are not tampered with. If no passphrase is provided a default string will be used.
///
/// The birthday is included to enable more efficient recoveries. Knowing the birthday of the seed phrase means we
/// only have to scan the blocks in the chain since that day for full recovery, rather than scanning the entire
/// blockchain.

#[derive(Clone, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
pub struct CipherSeed {
    version: u8,
    birthday: u16,
    entropy: Box<[u8; CIPHER_SEED_ENTROPY_BYTES]>,
    salt: [u8; CIPHER_SEED_MAIN_SALT_BYTES],
}

/// `Debug` is implemented by hand rather than derived: the entropy is the wallet master secret, and every key in the
/// wallet can be re-derived from it. A derived `Debug` would render it in full, and any `{:?}` sink (log lines, error
/// messages, `Display` impls of enclosing types) would then leak the master seed. The secret fields are rendered the
/// same way `Hidden` renders its contents so that the redaction is obvious in output.
impl fmt::Debug for CipherSeed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CipherSeed")
            .field("version", &self.version)
            .field("birthday", &self.birthday)
            .field("entropy", &format_args!("Hidden<[u8; {CIPHER_SEED_ENTROPY_BYTES}]>"))
            .field("salt", &format_args!("Hidden<[u8; {CIPHER_SEED_MAIN_SALT_BYTES}]>"))
            .finish()
    }
}

// This is a separate type to make the linter happy
type DerivedCipherSeedKeys = Result<(CipherSeedEncryptionKey, CipherSeedMacKey), CipherError>;

impl CipherSeed {
    #[cfg(not(target_arch = "wasm32"))]
    /// Generate a new seed
    pub fn random() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default() // default to the epoch on error
            .as_millis();
        let millis = u64::try_from(millis).unwrap_or(0u64);
        CipherSeed::new_with_birthday(birthday_from_unix_millis(millis))
    }

    pub fn change_birthday(&mut self, birthday: u16) {
        self.birthday = birthday;
    }

    #[cfg(target_arch = "wasm32")]
    /// Generate a new seed
    pub fn random() -> Self {
        let millis = js_sys::Date::now() as u64;
        CipherSeed::new_with_birthday(birthday_from_unix_millis(millis))
    }

    /// Generate a new seed with a given birthday
    fn new_with_birthday(birthday: u16) -> Self {
        let mut entropy = Box::new([0u8; CIPHER_SEED_ENTROPY_BYTES]);
        rand::rng().fill_bytes(entropy.as_mut());
        let mut salt = [0u8; CIPHER_SEED_MAIN_SALT_BYTES];
        rand::rng().fill_bytes(salt.as_mut());

        Self {
            version: CIPHER_SEED_VERSION,
            birthday,
            entropy,
            salt,
        }
    }

    /// Generate an encrypted seed from a passphrase. A user passphrase writes version 3 (Argon2id); no passphrase
    /// writes version 2 (Argon2d) under the default passphrase.
    pub fn encipher(&self, passphrase: Option<SafePassword>) -> Result<Zeroizing<Vec<u8>>, CipherError> {
        // Derive encryption and MAC keys from passphrase and main salt
        let (version, passphrase) = match passphrase {
            Some(passphrase) => (CIPHER_SEED_VERSION_ARGON2ID, passphrase),
            None => (CIPHER_SEED_VERSION, default_passphrase()),
        };
        let (encryption_key, mac_key) = Self::derive_keys(version, &passphrase, self.salt.as_ref())?;

        // Generate the MAC
        let mac = Self::generate_mac(
            version,
            &self.birthday.to_le_bytes(),
            self.entropy.as_ref(),
            self.salt.as_ref(),
            &mac_key,
        )?;

        // Assemble the secret data to be encrypted: birthday, entropy, MAC
        let mut secret_data = Zeroizing::new(Vec::<u8>::with_capacity(CIPHER_SEED_SECRET_DATA_BYTES));
        secret_data.extend(self.birthday.to_le_bytes());
        secret_data.extend(self.entropy.iter());
        secret_data.extend(mac.iter());

        // Encrypt the secret data
        Self::apply_stream_cipher(&mut secret_data, &encryption_key, self.salt.as_ref())?;

        // Assemble the final seed: version, main salt, secret data, checksum
        let mut encrypted_seed = Zeroizing::new(Vec::<u8>::with_capacity(CIPHER_SEED_ENCIPHERED_BYTES));
        encrypted_seed.push(version);
        encrypted_seed.extend(secret_data.iter());
        encrypted_seed.extend(self.salt.iter());

        let mut crc_hasher = CrcHasher::new();
        crc_hasher.update(encrypted_seed.as_slice());
        let checksum = crc_hasher.finalize().to_le_bytes();
        encrypted_seed.extend(checksum);

        Ok(encrypted_seed)
    }

    /// Recover a seed from encrypted data and a passphrase. Versions 2 and 3 are accepted; the version byte is bound
    /// by the MAC and selects the key derivation.
    pub fn from_enciphered_bytes(encrypted_seed: &[u8], passphrase: Option<SafePassword>) -> Result<Self, CipherError> {
        // Check the length: version, birthday, entropy, MAC, salt, checksum
        if encrypted_seed.len() != CIPHER_SEED_ENCIPHERED_BYTES {
            return Err(CipherError::InvalidData);
        }

        let version = *encrypted_seed.first().ok_or(CipherError::InvalidData)?;
        if version != CIPHER_SEED_VERSION && version != CIPHER_SEED_VERSION_ARGON2ID {
            return Err(CipherError::VersionMismatch);
        }

        // Verify the checksum first, to detect obvious errors
        let (payload, checksum) = encrypted_seed
            .split_at_checked(CIPHER_SEED_ENCIPHERED_BYTES - CIPHER_SEED_CHECKSUM_BYTES)
            .ok_or(CipherError::InvalidData)?;
        let mut crc_hasher = CrcHasher::new();
        crc_hasher.update(payload);
        let expected_checksum = crc_hasher.finalize().to_le_bytes();
        if checksum != expected_checksum {
            return Err(CipherError::CrcError);
        }

        // Derive encryption and MAC keys from passphrase and main salt
        let passphrase = passphrase.unwrap_or_else(default_passphrase);
        let mut salt = [0u8; CIPHER_SEED_MAIN_SALT_BYTES];
        salt.copy_from_slice(
            payload
                .get(1 + CIPHER_SEED_SECRET_DATA_BYTES..)
                .ok_or(CipherError::InvalidData)?,
        );
        let (encryption_key, mac_key) = Self::derive_keys(version, &passphrase, salt.as_ref())?;

        // Decrypt the secret data: birthday, entropy, MAC
        let mut secret_data = Zeroizing::new([0u8; CIPHER_SEED_SECRET_DATA_BYTES]);
        secret_data.copy_from_slice(
            payload
                .get(1..1 + CIPHER_SEED_SECRET_DATA_BYTES)
                .ok_or(CipherError::InvalidData)?,
        );
        Self::apply_stream_cipher(secret_data.as_mut(), &encryption_key, salt.as_ref())?;

        // Parse the secret data straight into the seed, which zeroizes itself if the MAC check fails
        let mut birthday_bytes = [0u8; CIPHER_SEED_BIRTHDAY_BYTES];
        birthday_bytes.copy_from_slice(
            secret_data
                .get(..CIPHER_SEED_BIRTHDAY_BYTES)
                .ok_or(CipherError::InvalidData)?,
        );
        let mut seed = Self {
            version: CIPHER_SEED_VERSION,
            birthday: u16::from_le_bytes(birthday_bytes),
            entropy: Box::new([0u8; CIPHER_SEED_ENTROPY_BYTES]),
            salt,
        };
        seed.entropy.copy_from_slice(
            secret_data
                .get(CIPHER_SEED_BIRTHDAY_BYTES..CIPHER_SEED_BIRTHDAY_BYTES + CIPHER_SEED_ENTROPY_BYTES)
                .ok_or(CipherError::InvalidData)?,
        );
        let mac = secret_data
            .get(CIPHER_SEED_BIRTHDAY_BYTES + CIPHER_SEED_ENTROPY_BYTES..)
            .ok_or(CipherError::InvalidData)?;

        // Generate the MAC
        let expected_mac =
            Self::generate_mac(version, &birthday_bytes, seed.entropy.as_ref(), salt.as_ref(), &mac_key)?;

        // Verify the MAC in constant time to avoid leaking data
        if mac.ct_eq(expected_mac.as_slice()).into() {
            Ok(seed)
        } else {
            Err(CipherError::DecryptionFailed)
        }
    }

    /// Encrypt or decrypt data using ChaCha20
    fn apply_stream_cipher(
        data: &mut [u8],
        encryption_key: &CipherSeedEncryptionKey,
        salt: &[u8],
    ) -> Result<(), CipherError> {
        // The ChaCha20 nonce is derived from the main salt
        let encryption_nonce = DomainSeparatedHasher::<Blake2b<U32>, KeyManagerDomain>::new_with_label(
            HASHER_LABEL_CIPHER_SEED_ENCRYPTION_NONCE,
        )
        .chain(salt)
        .finalize();
        let encryption_nonce = encryption_nonce
            .as_ref()
            .get(..size_of::<Nonce>())
            .expect("Index should exist");

        // Encrypt/decrypt the data
        let mut cipher = ChaCha20::new(
            Key::from_slice(encryption_key.reveal()),
            Nonce::from_slice(encryption_nonce),
        );
        cipher
            .try_apply_keystream(data)
            .map_err(|_| CipherError::CryptographicError("Unable to apply stream cipher".to_string()))?;

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
        version: u8,
        birthday: &[u8],
        entropy: &[u8],
        salt: &[u8],
        mac_key: &CipherSeedMacKey,
    ) -> Result<Zeroizing<Vec<u8>>, CipherError> {
        // Check all lengths are valid
        if birthday.len() != CIPHER_SEED_BIRTHDAY_BYTES {
            return Err(CipherError::InvalidData);
        }
        if entropy.len() != CIPHER_SEED_ENTROPY_BYTES {
            return Err(CipherError::InvalidData);
        }
        if salt.len() != CIPHER_SEED_MAIN_SALT_BYTES {
            return Err(CipherError::InvalidData);
        }

        // The hasher absorbs the entropy and MAC key; finalize into a zeroizing buffer so that no unzeroized copy of
        // the full digest is left behind.
        let mac = DomainSeparatedHasher::<Blake2b<U32>, KeyManagerDomain>::new_with_label(HASHER_LABEL_CIPHER_SEED_MAC)
            .chain([version])
            .chain(birthday)
            .chain(entropy)
            .chain(salt)
            .chain(mac_key.reveal())
            .finalize_zeroizing();
        Ok(Zeroizing::new(
            mac.get(..CIPHER_SEED_MAC_BYTES).expect("Index should exist").to_vec(),
        ))
    }

    /// Use Argon2 to derive encryption and MAC keys from a passphrase and main salt. Version 3 uses Argon2id, version 2
    /// Argon2d.
    fn derive_keys(version: u8, passphrase: &SafePassword, salt: &[u8]) -> DerivedCipherSeedKeys {
        // The Argon2 salt is derived from the main salt
        let argon2_salt = DomainSeparatedHasher::<Blake2b<U32>, KeyManagerDomain>::new_with_label(
            HASHER_LABEL_CIPHER_SEED_PBKDF_SALT,
        )
        .chain(salt)
        .finalize();
        let argon2_salt = argon2_salt
            .as_ref()
            .get(..ARGON2_SALT_BYTES)
            .expect("Index should exist");

        // Run Argon2 with enough output to accommodate both keys, so we only run it once.
        // The costs are OWASP's recommended Argon2id parameters, which version 3 uses; version 2 used them with
        // Argon2d: https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html#argon2id
        let params = argon2::Params::new(
            46 * 1024, // m-cost should be 46 MiB = 46 * 1024 KiB
            1,         // t-cost
            1,         // p-cost
            Some(CIPHER_SEED_ENCRYPTION_KEY_BYTES + CIPHER_SEED_MAC_KEY_BYTES),
        )
        .map_err(|_| CipherError::CryptographicError("Problem generating Argon2 parameters".to_string()))?;

        // Derive the main key from the password in place
        let mut main_key = Hidden::hide([0u8; CIPHER_SEED_ENCRYPTION_KEY_BYTES + CIPHER_SEED_MAC_KEY_BYTES]);
        let algorithm = if version == CIPHER_SEED_VERSION_ARGON2ID {
            argon2::Algorithm::Argon2id
        } else {
            argon2::Algorithm::Argon2d
        };
        let hasher = argon2::Argon2::new(algorithm, argon2::Version::V0x13, params);
        hasher
            .hash_password_into(passphrase.reveal(), argon2_salt, main_key.reveal_mut())
            .map_err(|_| CipherError::CryptographicError("Problem generating Argon2 password hash".to_string()))?;

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
        Self::random()
    }
}

fn default_passphrase() -> SafePassword {
    SafePassword::from_str(DEFAULT_CIPHER_SEED_PASSPHRASE)
        .expect("Failed to parse default cipher seed passphrase to SafePassword")
}

/// The birthday (days since the birthday genesis) for a unix time in milliseconds; 0 before the genesis or past the
/// `u16` range.
fn birthday_from_unix_millis(millis: u64) -> u16 {
    const MILLISECONDS_PER_DAY: u64 = SECONDS_PER_DAY * 1000;
    let days = millis.saturating_sub(BIRTHDAY_GENESIS_FROM_UNIX_EPOCH * 1000) / MILLISECONDS_PER_DAY;
    u16::try_from(days).unwrap_or(0u16)
}

/// Serialized as the seed enciphered under the default passphrase: hex when human readable, bytes otherwise.
impl Serialize for CipherSeed {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        let bytes = self.encipher(None).map_err(serde::ser::Error::custom)?;
        if serializer.is_human_readable() {
            let hex = Zeroizing::new(to_hex(&bytes));
            serializer.serialize_str(&hex)
        } else {
            serializer.serialize_bytes(&bytes)
        }
    }
}

/// The plaintext struct form `CipherSeed` was serialized as before it was serialized enciphered.
#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
struct LegacyCipherSeed {
    version: u8,
    birthday: u16,
    #[serde(deserialize_with = "deserialize_legacy_entropy")]
    entropy: Zeroizing<Vec<u8>>,
    salt: [u8; CIPHER_SEED_MAIN_SALT_BYTES],
}

/// Read the legacy entropy array into a buffer sized up front, so it never reallocates and leaves a copy behind
fn deserialize_legacy_entropy<'de, D>(deserializer: D) -> Result<Zeroizing<Vec<u8>>, D::Error>
where D: Deserializer<'de> {
    struct EntropyVisitor;

    impl<'de> Visitor<'de> for EntropyVisitor {
        type Value = Zeroizing<Vec<u8>>;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "an array of {CIPHER_SEED_ENTROPY_BYTES} bytes")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Zeroizing<Vec<u8>>, A::Error>
        where A: de::SeqAccess<'de> {
            let mut entropy = Zeroizing::new(Vec::with_capacity(CIPHER_SEED_ENTROPY_BYTES));
            while let Some(byte) = seq.next_element::<u8>()? {
                if entropy.len() == CIPHER_SEED_ENTROPY_BYTES {
                    return Err(de::Error::invalid_length(entropy.len().saturating_add(1), &self));
                }
                entropy.push(byte);
            }
            if entropy.len() != CIPHER_SEED_ENTROPY_BYTES {
                return Err(de::Error::invalid_length(entropy.len(), &self));
            }
            Ok(entropy)
        }
    }

    deserializer.deserialize_seq(EntropyVisitor)
}

struct CipherSeedVisitor;

impl<'de> Visitor<'de> for CipherSeedVisitor {
    type Value = CipherSeed;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("an enciphered cipher seed as hex or bytes, or a legacy cipher seed struct")
    }

    fn visit_str<E>(self, v: &str) -> Result<CipherSeed, E>
    where E: de::Error {
        let bytes = Zeroizing::new(from_hex(v).map_err(E::custom)?);
        CipherSeed::from_enciphered_bytes(&bytes, None).map_err(E::custom)
    }

    fn visit_bytes<E>(self, v: &[u8]) -> Result<CipherSeed, E>
    where E: de::Error {
        CipherSeed::from_enciphered_bytes(v, None).map_err(E::custom)
    }

    fn visit_map<M>(self, map: M) -> Result<CipherSeed, M::Error>
    where M: MapAccess<'de> {
        let legacy = LegacyCipherSeed::deserialize(de::value::MapAccessDeserializer::new(map))?;
        if legacy.version != CIPHER_SEED_VERSION {
            return Err(de::Error::custom(CipherError::VersionMismatch));
        }
        let mut seed = CipherSeed {
            version: CIPHER_SEED_VERSION,
            birthday: legacy.birthday,
            entropy: Box::new([0u8; CIPHER_SEED_ENTROPY_BYTES]),
            salt: legacy.salt,
        };
        seed.entropy.copy_from_slice(legacy.entropy.as_slice());
        Ok(seed)
    }
}

/// Accepts the enciphered form `Serialize` writes and, when human readable, the legacy plaintext struct.
impl<'de> Deserialize<'de> for CipherSeed {
    fn deserialize<D>(deserializer: D) -> Result<CipherSeed, D::Error>
    where D: Deserializer<'de> {
        if deserializer.is_human_readable() {
            deserializer.deserialize_any(CipherSeedVisitor)
        } else {
            deserializer.deserialize_bytes(CipherSeedVisitor)
        }
    }
}

impl Mnemonic<CipherSeed> for CipherSeed {
    /// Generates a CipherSeed that represent the provided mnemonic sequence of words, the language of the mnemonic
    /// sequence is autodetected
    fn from_mnemonic(mnemonic_seq: &SeedWords, passphrase: Option<SafePassword>) -> Result<CipherSeed, CipherError> {
        let bytes = to_bytes(mnemonic_seq)?;
        CipherSeed::from_enciphered_bytes(bytes.reveal(), passphrase)
    }

    /// Generates a SecretKey that represent the provided mnemonic sequence of words using the specified language
    fn from_mnemonic_with_language(
        mnemonic_seq: &SeedWords,
        language: MnemonicLanguage,
        passphrase: Option<SafePassword>,
    ) -> Result<CipherSeed, CipherError> {
        let bytes = to_bytes_with_language(mnemonic_seq, &language)?;
        CipherSeed::from_enciphered_bytes(bytes.reveal(), passphrase)
    }

    /// Generates a mnemonic sequence of words from the provided secret key
    fn to_mnemonic(
        &self,
        language: MnemonicLanguage,
        passphrase: Option<SafePassword>,
    ) -> Result<SeedWords, CipherError> {
        Ok(from_bytes(&self.encipher(passphrase)?, language)?)
    }
}

#[cfg(test)]
mod test {
    #![allow(clippy::indexing_slicing)]
    use std::str::FromStr;

    use chrono::{DateTime, TimeZone, Utc};
    use crc32fast::Hasher as CrcHasher;
    use tari_utilities::{Hidden, SafePassword, hex::to_hex};

    use super::{BIRTHDAY_GENESIS_FROM_UNIX_EPOCH, DEFAULT_CIPHER_SEED_PASSPHRASE, SECONDS_PER_DAY};
    use crate::seeds::{
        cipher_seed::{
            CIPHER_SEED_BIRTHDAY_BYTES,
            CIPHER_SEED_CHECKSUM_BYTES,
            CIPHER_SEED_ENTROPY_BYTES,
            CIPHER_SEED_MAC_BYTES,
            CIPHER_SEED_MAIN_SALT_BYTES,
            CIPHER_SEED_VERSION,
            CIPHER_SEED_VERSION_ARGON2ID,
            CipherSeed,
            birthday_from_unix_millis,
        },
        error::CipherError,
        mnemonic::{Mnemonic, MnemonicLanguage},
        seed_words::{SeedWords, get_birthday_from_unix_epoch_in_seconds},
    };

    #[test]
    fn test_cipher_seed_debug_does_not_leak_entropy() {
        let seed = CipherSeed::random();
        let rendered = format!("{seed:?}");

        // Neither the entropy nor the salt may appear, in hex or in the byte-slice rendering a derived `Debug` uses
        assert!(
            !rendered.contains(&to_hex(seed.entropy())),
            "CipherSeed Debug leaked the entropy: {rendered}"
        );
        assert!(
            !rendered.contains(&format!("{:?}", seed.entropy())),
            "CipherSeed Debug leaked the entropy: {rendered}"
        );
        assert!(
            !rendered.contains(&format!("{:?}", seed.salt)),
            "CipherSeed Debug leaked the salt: {rendered}"
        );
        assert!(
            rendered.contains(&format!("Hidden<[u8; {CIPHER_SEED_ENTROPY_BYTES}]>")),
            "entropy was not redacted: {rendered}"
        );
        assert!(
            rendered.contains(&format!("Hidden<[u8; {CIPHER_SEED_MAIN_SALT_BYTES}]>")),
            "salt was not redacted: {rendered}"
        );
    }

    #[test]
    fn test_cipher_seed_generation_and_deciphering() {
        let passphrase = "Passphrase";

        // Generate a new encrypted cipher seed
        let seed = CipherSeed::random();
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
            Err(CipherError::DecryptionFailed) => (),
            _ => panic!("Version should not match"),
        }

        // An unsupported version fails
        let mut malleated_seed = enciphered_seed.to_vec();
        malleated_seed[0] = CIPHER_SEED_VERSION_ARGON2ID + 1;
        match CipherSeed::from_enciphered_bytes(&malleated_seed, Some(SafePassword::from_str(passphrase).unwrap())) {
            Err(CipherError::VersionMismatch) => (),
            _ => panic!("Version should not match"),
        }

        // Malleated ciphertext should fail the checksum
        const MALLEATED_BYTE: usize = 1;
        let mut malleated_seed = enciphered_seed.to_vec();
        malleated_seed[MALLEATED_BYTE] = !malleated_seed[MALLEATED_BYTE];
        match CipherSeed::from_enciphered_bytes(&malleated_seed, Some(SafePassword::from_str(passphrase).unwrap())) {
            Err(CipherError::CrcError) => (),
            _ => panic!("Checksum should fail"),
        }

        // Malleate the birthday, but with a correct checksum
        const MALLEATED_BIRTHDAY_BYTE: usize = 1;
        let mut malleated_seed = enciphered_seed.to_vec();
        malleated_seed[MALLEATED_BIRTHDAY_BYTE] = !malleated_seed[MALLEATED_BIRTHDAY_BYTE];

        let mut crc_hasher = CrcHasher::new();
        crc_hasher.update(&malleated_seed[..(n - CIPHER_SEED_CHECKSUM_BYTES)]);
        let calculated_checksum: [u8; CIPHER_SEED_CHECKSUM_BYTES] = crc_hasher.finalize().to_le_bytes();

        malleated_seed[(n - CIPHER_SEED_CHECKSUM_BYTES)..].copy_from_slice(&calculated_checksum);

        match CipherSeed::from_enciphered_bytes(&malleated_seed, Some(SafePassword::from_str(passphrase).unwrap())) {
            Err(CipherError::DecryptionFailed) => (),
            _ => panic!("Decryption should fail"),
        }

        // Malleate the entropy, but with a correct checksum
        const MALLEATED_ENTROPY_BYTE: usize = 1 + CIPHER_SEED_BIRTHDAY_BYTES;
        let mut malleated_seed = enciphered_seed.to_vec();
        malleated_seed[MALLEATED_ENTROPY_BYTE] = !malleated_seed[MALLEATED_ENTROPY_BYTE];

        let mut crc_hasher = CrcHasher::new();
        crc_hasher.update(&malleated_seed[..(n - CIPHER_SEED_CHECKSUM_BYTES)]);
        let calculated_checksum: [u8; CIPHER_SEED_CHECKSUM_BYTES] = crc_hasher.finalize().to_le_bytes();

        malleated_seed[(n - CIPHER_SEED_CHECKSUM_BYTES)..].copy_from_slice(&calculated_checksum);

        match CipherSeed::from_enciphered_bytes(&malleated_seed, Some(SafePassword::from_str(passphrase).unwrap())) {
            Err(CipherError::DecryptionFailed) => (),
            _ => panic!("Decryption should fail"),
        }

        // Malleate the MAC, but with a correct checksum
        const MALLEATED_MAC_BYTE: usize = 1 + CIPHER_SEED_BIRTHDAY_BYTES + CIPHER_SEED_ENTROPY_BYTES;
        let mut malleated_seed = enciphered_seed.to_vec();
        malleated_seed[MALLEATED_MAC_BYTE] = !malleated_seed[MALLEATED_MAC_BYTE];

        let mut crc_hasher = CrcHasher::new();
        crc_hasher.update(&malleated_seed[..(n - CIPHER_SEED_CHECKSUM_BYTES)]);
        let calculated_checksum: [u8; CIPHER_SEED_CHECKSUM_BYTES] = crc_hasher.finalize().to_le_bytes();

        malleated_seed[(n - CIPHER_SEED_CHECKSUM_BYTES)..].copy_from_slice(&calculated_checksum);

        match CipherSeed::from_enciphered_bytes(&malleated_seed, Some(SafePassword::from_str(passphrase).unwrap())) {
            Err(CipherError::DecryptionFailed) => (),
            _ => panic!("Decryption should fail"),
        }

        // Malleate the salt, but with a correct checksum
        const MALLEATED_SALT_BYTE: usize =
            1 + CIPHER_SEED_BIRTHDAY_BYTES + CIPHER_SEED_ENTROPY_BYTES + CIPHER_SEED_MAC_BYTES;
        let mut malleated_seed = enciphered_seed.to_vec();
        malleated_seed[MALLEATED_SALT_BYTE] = !malleated_seed[MALLEATED_SALT_BYTE];

        let mut crc_hasher = CrcHasher::new();
        crc_hasher.update(&malleated_seed[..(n - CIPHER_SEED_CHECKSUM_BYTES)]);
        let calculated_checksum: [u8; CIPHER_SEED_CHECKSUM_BYTES] = crc_hasher.finalize().to_le_bytes();

        malleated_seed[(n - CIPHER_SEED_CHECKSUM_BYTES)..].copy_from_slice(&calculated_checksum);

        match CipherSeed::from_enciphered_bytes(&malleated_seed, Some(SafePassword::from_str(passphrase).unwrap())) {
            Err(CipherError::DecryptionFailed) => (),
            _ => panic!("Decryption should fail"),
        }
    }

    #[test]
    fn test_cipher_seed_to_mnemonic_and_from_mnemonic() {
        // Valid Mnemonic sequence
        let mut seed = CipherSeed::new_with_birthday(183u16);
        let mnemonic_seq = seed
            .to_mnemonic(MnemonicLanguage::Japanese, None)
            .expect("Couldn't convert CipherSeed to Mnemonic");
        match CipherSeed::from_mnemonic(&mnemonic_seq, None) {
            Ok(mnemonic_seed) => assert_eq!(seed, mnemonic_seed),
            Err(e) => panic!("Couldn't create CipherSeed from Mnemonic: {e}"),
        }
        // Language known
        let mnemonic_seed = CipherSeed::from_mnemonic_with_language(&mnemonic_seq, MnemonicLanguage::Japanese, None)
            .expect("Couldn't create CipherSeed from Mnemonic with Language");
        assert_eq!(seed, mnemonic_seed);
        assert_eq!(mnemonic_seed.birthday, 183u16);
        // change the birthday and check it is correctly reflected in the mnemonic
        seed.change_birthday(200u16);

        let mnemonic_seq = seed
            .to_mnemonic(MnemonicLanguage::Japanese, None)
            .expect("Couldn't convert CipherSeed to Mnemonic");
        let mnemonic_seed = CipherSeed::from_mnemonic(&mnemonic_seq, None).expect("Should decrypt");
        assert_eq!(mnemonic_seed.birthday, 200u16);

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
        let seed = CipherSeed::random();
        let mnemonic_seq = seed
            .to_mnemonic(
                MnemonicLanguage::Spanish,
                Some(SafePassword::from_str("Passphrase").unwrap()),
            )
            .expect("Couldn't convert CipherSeed to Mnemonic");
        match CipherSeed::from_mnemonic(&mnemonic_seq, Some(SafePassword::from_str("Passphrase").unwrap())) {
            Ok(mnemonic_seed) => assert_eq!(seed, mnemonic_seed),
            Err(e) => panic!("Couldn't create CipherSeed from Mnemonic: {e}"),
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
            BIRTHDAY_GENESIS_FROM_UNIX_EPOCH + u64::from(birthday - to_days) * SECONDS_PER_DAY
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

    // This test simulates a wallet that was created on the 29th January 2025, one day after the birthday genesis date
    // of 28th January 2025. The birthday is 1124 days after the genesis date of 1st January 2022. The wallet will
    // request blocks from the base node 14 days before its birthday, which is 15th January 2025.
    #[test]
    fn verify_birthday_genesis_from_unix_epoch_constant() {
        let birthday = 1124u16;
        let to_days = 14u16;
        let year = 2025;
        let month = 1;
        let day = 29;
        let day_less_to_days = day - u32::from(to_days);

        // Verify birthday genesis from unix epoch constant
        let date = Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0);
        let seconds_since_epoch = date.unwrap().timestamp();
        assert_eq!(
            BIRTHDAY_GENESIS_FROM_UNIX_EPOCH,
            u64::try_from(seconds_since_epoch).unwrap()
        );

        // The wallet birthday is 1124 days after the genesis date
        let birthday_epoch_time = i64::try_from(BIRTHDAY_GENESIS_FROM_UNIX_EPOCH).unwrap() +
            i64::from(birthday) * i64::try_from(SECONDS_PER_DAY).unwrap();
        assert_eq!(birthday_epoch_time, 1738108800i64);
        let birthday_epoch_time_date = Utc.timestamp_opt(birthday_epoch_time, 0);
        assert_eq!(
            birthday_epoch_time_date.unwrap(),
            Utc.with_ymd_and_hms(year, month, day, 0, 0, 0).unwrap()
        );

        // The wallet will request blocks from the base node 14 days before its birthday
        let birthday_epoch_time_less_to_days =
            birthday_epoch_time - i64::from(to_days) * i64::try_from(SECONDS_PER_DAY).unwrap();
        assert_eq!(
            birthday_epoch_time_less_to_days,
            i64::try_from(get_birthday_from_unix_epoch_in_seconds(birthday, to_days)).unwrap()
        );
        assert_eq!(birthday_epoch_time_less_to_days, 1736899200i64);
        let birthday_epoch_time_less_to_days_date = Utc.timestamp_opt(birthday_epoch_time_less_to_days, 0);
        assert_eq!(
            birthday_epoch_time_less_to_days_date.unwrap(),
            Utc.with_ymd_and_hms(year, month, day_less_to_days, 0, 0, 0).unwrap()
        );

        // The base node will return the genesis block as the first block to scan from
        let genesis_date_time = DateTime::parse_from_rfc2822("28 Jan 2025 08:00:00 +0200").unwrap();
        let genesis_epoch_time = genesis_date_time.timestamp();
        assert_eq!(genesis_epoch_time, 1738044000i64);
        assert!(genesis_epoch_time > birthday_epoch_time_less_to_days);
        assert!(genesis_epoch_time < birthday_epoch_time);
    }

    /// Re-encipher `seed` under `version`'s key derivation, as an older or newer wallet would have written it.
    fn encipher_as_version(seed: &CipherSeed, version: u8, passphrase: &str) -> Vec<u8> {
        let passphrase = SafePassword::from_str(passphrase).unwrap();
        let (encryption_key, mac_key) = CipherSeed::derive_keys(version, &passphrase, &seed.salt).unwrap();
        let mac = CipherSeed::generate_mac(
            version,
            &seed.birthday.to_le_bytes(),
            seed.entropy(),
            &seed.salt,
            &mac_key,
        )
        .unwrap();
        let mut secret_data = seed.birthday.to_le_bytes().to_vec();
        secret_data.extend_from_slice(seed.entropy());
        secret_data.extend_from_slice(&mac);
        CipherSeed::apply_stream_cipher(&mut secret_data, &encryption_key, &seed.salt).unwrap();
        let mut bytes = vec![version];
        bytes.extend_from_slice(&secret_data);
        bytes.extend_from_slice(&seed.salt);
        let mut crc_hasher = CrcHasher::new();
        crc_hasher.update(&bytes);
        bytes.extend_from_slice(&crc_hasher.finalize().to_le_bytes());
        bytes
    }

    #[test]
    fn a_user_passphrase_writes_version_3_and_no_passphrase_writes_version_2() {
        let seed = CipherSeed::random();

        let v2 = seed.encipher(None).unwrap();
        assert_eq!(v2[0], CIPHER_SEED_VERSION);
        assert_eq!(CipherSeed::from_enciphered_bytes(&v2, None).unwrap(), seed);
        // Byte for byte what an older wallet writes, so it imports into one
        assert_eq!(
            v2.to_vec(),
            encipher_as_version(&seed, CIPHER_SEED_VERSION, DEFAULT_CIPHER_SEED_PASSPHRASE)
        );

        let passphrase = || Some(SafePassword::from_str("Passphrase").unwrap());
        let v3 = seed.encipher(passphrase()).unwrap();
        assert_eq!(v3[0], CIPHER_SEED_VERSION_ARGON2ID);
        assert_eq!(CipherSeed::from_enciphered_bytes(&v3, passphrase()).unwrap(), seed);
        assert_eq!(
            v3.to_vec(),
            encipher_as_version(&seed, CIPHER_SEED_VERSION_ARGON2ID, "Passphrase")
        );

        // A version 2 seed under a user passphrase, as older wallets wrote it, still decodes
        let old = encipher_as_version(&seed, CIPHER_SEED_VERSION, "Passphrase");
        assert_eq!(CipherSeed::from_enciphered_bytes(&old, passphrase()).unwrap(), seed);

        // The version byte is bound by the MAC: relabelling a seed as the other version fails
        for (bytes, other_version) in [(old, CIPHER_SEED_VERSION_ARGON2ID), (v3.to_vec(), CIPHER_SEED_VERSION)] {
            let mut relabelled = bytes;
            let n = relabelled.len();
            relabelled[0] = other_version;
            let mut crc_hasher = CrcHasher::new();
            crc_hasher.update(&relabelled[..n - CIPHER_SEED_CHECKSUM_BYTES]);
            relabelled[n - CIPHER_SEED_CHECKSUM_BYTES..].copy_from_slice(&crc_hasher.finalize().to_le_bytes());
            assert!(matches!(
                CipherSeed::from_enciphered_bytes(&relabelled, passphrase()),
                Err(CipherError::DecryptionFailed)
            ));
        }
    }

    #[test]
    fn birthday_from_unix_millis_counts_days_from_the_birthday_genesis() {
        const MILLIS_PER_DAY: u64 = SECONDS_PER_DAY * 1000;
        let genesis_millis = BIRTHDAY_GENESIS_FROM_UNIX_EPOCH * 1000;
        assert_eq!(birthday_from_unix_millis(0), 0);
        assert_eq!(birthday_from_unix_millis(genesis_millis - 1), 0);
        assert_eq!(birthday_from_unix_millis(genesis_millis), 0);
        assert_eq!(birthday_from_unix_millis(genesis_millis + MILLIS_PER_DAY - 1), 0);
        assert_eq!(birthday_from_unix_millis(genesis_millis + 183 * MILLIS_PER_DAY), 183);
        // Past the u16 range
        assert_eq!(
            birthday_from_unix_millis(genesis_millis + (u64::from(u16::MAX) + 1) * MILLIS_PER_DAY),
            0
        );
        // A fresh seed's birthday is today's
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap();
        let today = birthday_from_unix_millis(u64::try_from(now.as_millis()).unwrap());
        assert!(CipherSeed::random().birthday().abs_diff(today) <= 1);
    }

    #[test]
    fn serde_writes_the_enciphered_form() {
        let seed = CipherSeed::random();

        let json = serde_json::to_string(&seed).unwrap();
        assert_eq!(json, format!("\"{}\"", to_hex(&seed.encipher(None).unwrap())));
        assert_eq!(serde_json::from_str::<CipherSeed>(&json).unwrap(), seed);
        assert!(!json.contains(&to_hex(seed.entropy())));
    }

    #[test]
    fn serde_reads_the_legacy_struct_form() {
        let seed = CipherSeed::random();
        let legacy = serde_json::json!({
            "version": CIPHER_SEED_VERSION,
            "birthday": seed.birthday,
            "entropy": seed.entropy(),
            "salt": seed.salt,
        });
        assert_eq!(serde_json::from_value::<CipherSeed>(legacy.clone()).unwrap(), seed);
        assert_eq!(serde_json::from_str::<CipherSeed>(&legacy.to_string()).unwrap(), seed);

        let mut short_entropy = legacy.clone();
        short_entropy["entropy"] = serde_json::json!(&seed.entropy()[..15]);
        assert!(serde_json::from_value::<CipherSeed>(short_entropy).is_err());
        let mut long_entropy = legacy.clone();
        long_entropy["entropy"] = serde_json::json!(vec![0u8; 17]);
        assert!(serde_json::from_value::<CipherSeed>(long_entropy).is_err());

        let mut wrong_version = legacy;
        wrong_version["version"] = serde_json::json!(CIPHER_SEED_VERSION - 1);
        assert!(serde_json::from_value::<CipherSeed>(wrong_version).is_err());

        // Enciphered hex with an unsupported version byte is rejected too
        let mut bytes = seed.encipher(None).unwrap().to_vec();
        bytes[0] = CIPHER_SEED_VERSION - 1;
        assert!(serde_json::from_str::<CipherSeed>(&format!("\"{}\"", to_hex(&bytes))).is_err());
    }
}
