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

use std::mem::size_of;

use chacha20poly1305::{
    aead::{Aead, Payload},
    XChaCha20Poly1305,
    XNonce,
};
use rand::{rngs::OsRng, RngCore};
use tari_utilities::{ByteArray, Hidden};

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
    const BURNT_PROOF: &'static [u8] = b"BURNT_PROOF";

    fn domain(&self, field_name: &'static str) -> Vec<u8>;
    fn encrypt(self, cipher: &C) -> Result<Self, String>
    where Self: Sized;
    fn decrypt(self, cipher: &C) -> Result<Self, String>
    where Self: Sized;
}

// Decrypt data (with domain binding and authentication) using XChaCha20-Poly1305
pub fn decrypt_bytes_integral_nonce(
    cipher: &XChaCha20Poly1305,
    domain: Vec<u8>,
    ciphertext: &[u8],
) -> Result<Vec<u8>, String> {
    // Extract the nonce
    let (nonce, ciphertext) = ciphertext
        .split_at_checked(size_of::<XNonce>())
        .ok_or("Ciphertext is too short".to_string())?;
    let nonce_ga = XNonce::from_slice(nonce);

    let payload = Payload {
        msg: ciphertext,
        aad: domain.as_bytes(),
    };

    // Attempt authentication and decryption
    let plaintext = cipher
        .decrypt(nonce_ga, payload)
        .map_err(|e| format!("Decryption failed: {}", e))?;

    Ok(plaintext)
}

// Encrypt data (with domain binding and authentication) using XChaCha20-Poly1305
pub fn encrypt_bytes_integral_nonce(
    cipher: &XChaCha20Poly1305,
    domain: Vec<u8>,
    plaintext: Hidden<Vec<u8>>,
) -> Result<Vec<u8>, String> {
    // Produce a secure random nonce
    let mut nonce = [0u8; size_of::<XNonce>()];
    OsRng.fill_bytes(&mut nonce);
    let nonce_ga = XNonce::from_slice(&nonce);

    // Bind the domain as additional data
    let payload = Payload {
        msg: plaintext.reveal(),
        aad: domain.as_slice(),
    };

    // Attempt authenticated encryption
    let mut ciphertext = cipher
        .encrypt(nonce_ga, payload)
        .map_err(|e| format!("Failed to encrypt: {}", e))?;

    // Concatenate the nonce and ciphertext (which already include the tag)
    let mut ciphertext_integral_nonce = nonce.to_vec();
    ciphertext_integral_nonce.append(&mut ciphertext);

    Ok(ciphertext_integral_nonce)
}

#[cfg(test)]
mod test {
    use chacha20poly1305::{Key, KeyInit, Tag};

    use super::*;

    #[test]
    fn test_encrypt_decrypt() {
        // Encrypt a message
        let plaintext = b"The quick brown fox was annoying".to_vec();
        let mut key = [0u8; size_of::<Key>()];
        OsRng.fill_bytes(&mut key);
        let key_ga = Key::from_slice(&key);
        let cipher = XChaCha20Poly1305::new(key_ga);

        let ciphertext =
            encrypt_bytes_integral_nonce(&cipher, b"correct_domain".to_vec(), Hidden::hide(plaintext.clone())).unwrap();

        // Check the ciphertext size, which we rely on for later tests
        // It should extend the plaintext size by the nonce and tag sizes
        assert_eq!(
            ciphertext.len(),
            size_of::<XNonce>() + plaintext.len() + size_of::<Tag>()
        );

        // Valid decryption must succeed and yield correct plaintext
        let decrypted_text = decrypt_bytes_integral_nonce(&cipher, b"correct_domain".to_vec(), &ciphertext).unwrap();
        assert_eq!(decrypted_text, plaintext);

        // Must fail on an incorrect domain
        assert!(decrypt_bytes_integral_nonce(&cipher, b"wrong_domain".to_vec(), &ciphertext).is_err());

        // Must fail with an evil nonce
        let ciphertext_with_evil_nonce = ciphertext
            .clone()
            .splice(0..size_of::<XNonce>(), [0u8; size_of::<XNonce>()])
            .collect::<Vec<_>>();
        assert!(
            decrypt_bytes_integral_nonce(&cipher, b"correct_domain".to_vec(), &ciphertext_with_evil_nonce).is_err()
        );

        // Must fail with malleated ciphertext
        let ciphertext_with_evil_ciphertext = ciphertext
            .clone()
            .splice(
                size_of::<XNonce>()..(ciphertext.len() - size_of::<Tag>()),
                vec![0u8; plaintext.len()],
            )
            .collect::<Vec<_>>();
        assert!(
            decrypt_bytes_integral_nonce(&cipher, b"correct_domain".to_vec(), &ciphertext_with_evil_ciphertext)
                .is_err()
        );

        // Must fail with malleated authentication tag
        let ciphertext_with_evil_tag = ciphertext
            .clone()
            .splice((ciphertext.len() - size_of::<Tag>())..ciphertext.len(), vec![
                0u8;
                size_of::<
                    Tag,
                >(
                )
            ])
            .collect::<Vec<_>>();
        assert!(decrypt_bytes_integral_nonce(&cipher, b"correct_domain".to_vec(), &ciphertext_with_evil_tag).is_err());

        // Must fail if truncated too short (if shorter than a nonce and tag, decryption is not even attempted)
        assert!(decrypt_bytes_integral_nonce(
            &cipher,
            b"correct_domain".to_vec(),
            &ciphertext[0..(size_of::<XNonce>() + size_of::<Tag>() - 1)]
        )
        .is_err());
    }
}
