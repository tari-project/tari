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

use crate::{
    envelope::{DhtMessageFlags, DhtMessageHeader, DhtMessageType, NodeDestination},
    outbound::DhtOutboundError,
    version::DhtProtocolVersion,
};
use chacha20::{
    cipher::{NewCipher, StreamCipher},
    ChaCha20,
    Key,
    Nonce,
};
use digest::Digest;
use rand::{rngs::OsRng, RngCore};
use std::mem::size_of;
use tari_comms::types::{Challenge, CommsPublicKey};
use tari_crypto::{
    keys::{DiffieHellmanSharedSecret, PublicKey},
    tari_utilities::{epoch_time::EpochTime, ByteArray},
};

pub fn generate_ecdh_secret<PK>(secret_key: &PK::K, public_key: &PK) -> PK
where PK: PublicKey + DiffieHellmanSharedSecret<PK = PK> {
    PK::shared_secret(secret_key, public_key)
}

pub fn decrypt(cipher_key: &CommsPublicKey, cipher_text: &[u8]) -> Result<Vec<u8>, DhtOutboundError> {
    if cipher_text.len() < size_of::<Nonce>() {
        return Err(DhtOutboundError::CipherError(
            "Cipher text is not long enough to include nonce".to_string(),
        ));
    }
    let (nonce, cipher_text) = cipher_text.split_at(size_of::<Nonce>());
    let nonce = Nonce::from_slice(nonce);
    let mut cipher_text = cipher_text.to_vec();

    let key = Key::from_slice(cipher_key.as_bytes()); // 32-bytes
    let mut cipher = ChaCha20::new(&key, &nonce);

    cipher.apply_keystream(cipher_text.as_mut_slice());

    Ok(cipher_text)
}

pub fn encrypt(cipher_key: &CommsPublicKey, plain_text: &[u8]) -> Result<Vec<u8>, DhtOutboundError> {
    let mut nonce = [0u8; size_of::<Nonce>()];

    OsRng.fill_bytes(&mut nonce);
    let nonce_ga = Nonce::from_slice(&nonce);

    let key = Key::from_slice(cipher_key.as_bytes()); // 32-bytes
    let mut cipher = ChaCha20::new(&key, &nonce_ga);

    // Cloning the plain text to avoid a caller thinking we have encrypted in place and losing the integral nonce added
    // below
    let mut plain_text_clone = plain_text.to_vec();

    cipher.apply_keystream(plain_text_clone.as_mut_slice());

    let mut ciphertext_integral_nonce = nonce.to_vec();
    ciphertext_integral_nonce.append(&mut plain_text_clone);
    Ok(ciphertext_integral_nonce)
}

pub fn create_origin_mac_challenge(header: &DhtMessageHeader, body: &[u8]) -> Challenge {
    create_origin_mac_challenge_parts(
        header.version,
        &header.destination,
        &header.message_type,
        header.flags,
        header.expires,
        header.ephemeral_public_key.as_ref(),
        body,
    )
}

pub fn create_origin_mac_challenge_parts(
    protocol_version: DhtProtocolVersion,
    destination: &NodeDestination,
    message_type: &DhtMessageType,
    flags: DhtMessageFlags,
    expires: Option<EpochTime>,
    ephemeral_public_key: Option<&CommsPublicKey>,
    body: &[u8],
) -> Challenge {
    let mut mac_challenge = Challenge::new();
    // TODO: #testnetreset remove conditional
    if protocol_version.as_major() > 1 {
        mac_challenge.update(&protocol_version.to_bytes());
        mac_challenge.update(destination.to_inner_bytes().as_slice());
        mac_challenge.update(&(*message_type as i32).to_le_bytes());
        mac_challenge.update(&flags.bits().to_le_bytes());
        if let Some(t) = expires {
            mac_challenge.update(&t.as_u64().to_le_bytes());
        }
        if let Some(e_pk) = ephemeral_public_key.as_ref() {
            mac_challenge.update(e_pk.as_bytes());
        }
    }
    mac_challenge.update(&body);
    mac_challenge
}

#[cfg(test)]
mod test {
    use super::*;
    use tari_utilities::hex::from_hex;

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
            from_hex("24bf9e698e14938e93c09e432274af7c143f8fb831f344f244ef02ca78a07ddc28b46fec536a0ca5c04737a604")
                .unwrap();
        let plain_text = decrypt(&key, &cipher_text).unwrap();
        let secret_msg = "Last enemy position 0830h AJ 9863".as_bytes().to_vec();
        assert_eq!(plain_text, secret_msg);
    }
}
