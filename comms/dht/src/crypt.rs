// Copyright 2020, The Tari Project
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

use tari_comms::types::CommsPublicKey;
use tari_crypto::{
    keys::{DiffieHellmanSharedSecret, PublicKey},
    tari_utilities::{
        ciphers::{
            chacha20::ChaCha20,
            cipher::{Cipher, CipherError},
        },
        ByteArray,
    },
};

pub fn generate_ecdh_secret<PK>(secret_key: &PK::K, public_key: &PK) -> PK
where PK: PublicKey + DiffieHellmanSharedSecret<PK = PK> {
    PK::shared_secret(secret_key, public_key)
}

pub fn decrypt(cipher_key: &CommsPublicKey, cipher_text: &[u8]) -> Result<Vec<u8>, CipherError> {
    ChaCha20::open_with_integral_nonce(cipher_text, cipher_key.as_bytes())
}

pub fn encrypt(cipher_key: &CommsPublicKey, plain_text: &Vec<u8>) -> Result<Vec<u8>, CipherError> {
    ChaCha20::seal_with_integral_nonce(plain_text, &cipher_key.to_vec())
}

#[cfg(test)]
mod test {
    use super::*;
    use tari_crypto::tari_utilities::hex::from_hex;

    #[test]
    fn encrypt_decrypt() {
        let key = CommsPublicKey::default();
        let plain_text = "Last enemy position 0830h AJ 9863".as_bytes().to_vec();
        let encrypted = encrypt(&key, &plain_text).unwrap();
        let decrypted = decrypt(&key, &encrypted).unwrap();
        assert_eq!(decrypted, plain_text);
    }

    #[test]
    fn decrypt_fn() {
        let key = CommsPublicKey::default();
        let cipher_text =
            from_hex("7ecafb4c0a88325c984517fca1c529b3083e9976290a50c43ff90b2ccb361aeaabfaf680e744b96fc3649a447b")
                .unwrap();
        let plain_text = decrypt(&key, &cipher_text).unwrap();
        let secret_msg = "Last enemy position 0830h AJ 9863".as_bytes().to_vec();
        assert_eq!(plain_text, secret_msg);
    }
}
