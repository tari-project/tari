// Copyright 2020. The Tari Project
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

use aes_gcm::{
    aead::{generic_array::GenericArray, Aead, Error as AeadError},
    Aes256Gcm,
};
use rand::{rngs::OsRng, RngCore};
use tari_crypto::{
    hash::blake2::Blake256,
    hashing::{DomainSeparatedHasher, MacDomain},
};
use tari_utilities::ByteArray;

pub const AES_NONCE_BYTES: usize = 12;
pub const AES_KEY_BYTES: usize = 32;
pub const AES_MAC_BYTES: usize = 32;

pub trait Encryptable<C> {
    fn source_key(&self, field_name: &'static str) -> Vec<u8>;
    fn encrypt(&mut self, cipher: &C) -> Result<(), String>;
    fn decrypt(&mut self, cipher: &C) -> Result<(), String>;
}

pub fn decrypt_bytes_integral_nonce(
    cipher: &Aes256Gcm,
    source_key: Vec<u8>,
    ciphertext: Vec<u8>,
) -> Result<Vec<u8>, String> {
    if ciphertext.len() < AES_NONCE_BYTES + AES_MAC_BYTES {
        return Err(AeadError.to_string());
    }

    let (nonce, ciphertext) = ciphertext.split_at(AES_NONCE_BYTES);
    let (ciphertext, stored_mac) = ciphertext.split_at(ciphertext.len() - AES_MAC_BYTES);
    let nonce = GenericArray::from_slice(nonce);
    let plaintext = cipher.decrypt(nonce, ciphertext.as_ref()).map_err(|e| e.to_string())?;

    let mut mac = DomainSeparatedHasher::<Blake256, MacDomain>::new("com.tari.storage_encryption_mac")
        .chain(nonce.as_slice())
        .chain(plaintext.as_bytes())
        .chain(source_key.as_bytes())
        .finalize()
        .into_vec();

    mac = DomainSeparatedHasher::<Blake256, MacDomain>::new("com.tari.storage_encryption_mac")
        .chain(ciphertext)
        .chain(mac)
        .finalize()
        .into_vec();

    if stored_mac != mac {
        return Err(AeadError.to_string());
    }

    Ok(plaintext)
}

pub fn encrypt_bytes_integral_nonce(
    cipher: &Aes256Gcm,
    source_key: Vec<u8>,
    plaintext: Vec<u8>,
) -> Result<Vec<u8>, String> {
    let mut nonce = [0u8; AES_NONCE_BYTES];
    OsRng.fill_bytes(&mut nonce);
    let nonce_ga = GenericArray::from_slice(&nonce);

    let mut mac = DomainSeparatedHasher::<Blake256, MacDomain>::new("com.tari.storage_encryption_mac")
        .chain(nonce.as_slice())
        .chain(plaintext.as_bytes())
        .chain(source_key.as_bytes())
        .finalize()
        .into_vec();

    let mut ciphertext = cipher
        .encrypt(nonce_ga, plaintext.as_bytes())
        .map_err(|e| e.to_string())?;

    mac = DomainSeparatedHasher::<Blake256, MacDomain>::new("com.tari.storage_encryption_mac")
        .chain(ciphertext.clone())
        .chain(mac.as_slice())
        .finalize()
        .into_vec();

    let mut ciphertext_integral_nonce = nonce.to_vec();
    ciphertext_integral_nonce.append(&mut ciphertext);
    ciphertext_integral_nonce.append(&mut mac);

    Ok(ciphertext_integral_nonce)
}

#[cfg(test)]
mod test {
    use aes_gcm::{
        aead::{generic_array::GenericArray, NewAead},
        Aes256Gcm,
    };

    use crate::util::encryption::{decrypt_bytes_integral_nonce, encrypt_bytes_integral_nonce};

    #[test]
    fn test_encrypt_decrypt() {
        let plaintext = b"The quick brown fox was annoying".to_vec();
        let key = GenericArray::from_slice(b"an example very very secret key.");
        let cipher = Aes256Gcm::new(key);

        let ciphertext = encrypt_bytes_integral_nonce(&cipher, b"source_key".to_vec(), plaintext.clone()).unwrap();
        let decrypted_text = decrypt_bytes_integral_nonce(&cipher, b"source_key".to_vec(), ciphertext).unwrap();
        assert_eq!(decrypted_text, plaintext);
    }
}
