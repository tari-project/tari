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
use tari_utilities::ByteArray;

use crate::types::WalletEncryptionHasher;

pub const AES_NONCE_BYTES: usize = 12;
pub const AES_KEY_BYTES: usize = 32;
pub const AES_MAC_BYTES: usize = 32;

pub trait Encryptable<C> {
    const KEY_MANAGER: &'static [u8] = b"KEY_MANAGER";
    const OUTPUT: &'static [u8] = b"OUTPUT";
    const WALLET_SETTING_MASTER_SEED: &'static [u8] = b"MASTER_SEED";
    const WALLET_SETTING_TOR_ID: &'static [u8] = b"TOR_ID";
    const INBOUND_TRANSACTION: &'static [u8] = b"INBOUND_TRANSACTION";
    const OUTBOUND_TRANSACTION: &'static [u8] = b"OUTBOUND_TRANSACTION";
    const COMPLETED_TRANSACTION: &'static [u8] = b"COMPLETED_TRANSACTION";
    const KNOWN_ONESIDED_PAYMENT_SCRIPT: &'static [u8] = b"KNOWN_ONESIDED_PAYMENT_SCRIPT";
    const CLIENT_KEY_VALUE: &'static [u8] = b"CLIENT_KEY_VALUE";

    fn domain(&self, field_name: &'static str) -> Vec<u8>;
    fn encrypt(&mut self, cipher: &C) -> Result<(), String>;
    fn decrypt(&mut self, cipher: &C) -> Result<(), String>;
}

pub fn decrypt_bytes_integral_nonce(
    cipher: &Aes256Gcm,
    domain: Vec<u8>,
    ciphertext: Vec<u8>,
) -> Result<Vec<u8>, String> {
    if ciphertext.len() < AES_NONCE_BYTES + AES_MAC_BYTES {
        return Err(AeadError.to_string());
    }

    let (nonce, ciphertext) = ciphertext.split_at(AES_NONCE_BYTES);
    let (ciphertext, appended_mac) = ciphertext.split_at(ciphertext.len().saturating_sub(AES_MAC_BYTES));
    let nonce = GenericArray::from_slice(nonce);

    let expected_mac = WalletEncryptionHasher::new_with_label("storage_encryption_mac")
        .chain(nonce.as_slice())
        .chain(ciphertext)
        .chain(domain)
        .finalize();

    if appended_mac != expected_mac.as_ref() {
        return Err(AeadError.to_string());
    }

    let plaintext = cipher.decrypt(nonce, ciphertext.as_ref()).map_err(|e| e.to_string())?;

    Ok(plaintext)
}

pub fn encrypt_bytes_integral_nonce(
    cipher: &Aes256Gcm,
    domain: Vec<u8>,
    plaintext: Vec<u8>,
) -> Result<Vec<u8>, String> {
    let mut nonce = [0u8; AES_NONCE_BYTES];
    OsRng.fill_bytes(&mut nonce);
    let nonce_ga = GenericArray::from_slice(&nonce);

    let mut ciphertext = cipher
        .encrypt(nonce_ga, plaintext.as_bytes())
        .map_err(|e| e.to_string())?;

    let mut mac = WalletEncryptionHasher::new_with_label("storage_encryption_mac")
        .chain(nonce.as_slice())
        .chain(ciphertext.clone())
        .chain(domain.as_slice())
        .finalize()
        .as_ref()
        .to_vec();

    let mut ciphertext_integral_nonce = nonce.to_vec();
    ciphertext_integral_nonce.append(&mut ciphertext);
    ciphertext_integral_nonce.append(&mut mac);

    Ok(ciphertext_integral_nonce)
}

#[cfg(test)]
mod test {
    use aes_gcm::{aead::generic_array::GenericArray, Aes256Gcm, KeyInit};

    use crate::util::encryption::{decrypt_bytes_integral_nonce, encrypt_bytes_integral_nonce};

    #[test]
    fn test_encrypt_decrypt() {
        let plaintext = b"The quick brown fox was annoying".to_vec();
        let key = GenericArray::from_slice(b"an example very very secret key.");
        let cipher = Aes256Gcm::new(key);

        let ciphertext = encrypt_bytes_integral_nonce(&cipher, b"correct_domain".to_vec(), plaintext.clone()).unwrap();
        let decrypted_text =
            decrypt_bytes_integral_nonce(&cipher, b"correct_domain".to_vec(), ciphertext.clone()).unwrap();

        // decrypted text must be equal to the original plaintext
        assert_eq!(decrypted_text, plaintext);

        // must fail with a wrong domain
        assert!(decrypt_bytes_integral_nonce(&cipher, b"wrong_domain".to_vec(), ciphertext.clone()).is_err());

        // must fail without nonce
        assert!(decrypt_bytes_integral_nonce(&cipher, b"correct_domain".to_vec(), ciphertext[0..12].to_vec()).is_err());

        // must fail without mac
        assert!(decrypt_bytes_integral_nonce(
            &cipher,
            b"correct_domain".to_vec(),
            ciphertext[0..ciphertext.len().saturating_sub(32)].to_vec()
        )
        .is_err());
    }
}
