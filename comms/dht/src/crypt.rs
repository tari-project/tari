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

use std::mem::size_of;

use chacha20::{
    cipher::{NewCipher, StreamCipher},
    ChaCha20,
    Key,
    Nonce,
};
use digest::{Digest, FixedOutput};
use rand::{rngs::OsRng, RngCore};
use tari_comms::types::{Challenge, CommsPublicKey};
use tari_crypto::{
    keys::{DiffieHellmanSharedSecret, PublicKey},
    tari_utilities::{epoch_time::EpochTime, ByteArray},
};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::{
    envelope::{DhtMessageFlags, DhtMessageHeader, DhtMessageType, NodeDestination},
    outbound::DhtOutboundError,
    version::DhtProtocolVersion,
};

#[derive(Debug, Clone, Zeroize, ZeroizeOnDrop)]
pub struct CipherKey(chacha20::Key);

/// Generates a Diffie-Hellman secret `kx.G` as a `chacha20::Key` given secret scalar `k` and public key `P = x.G`.
pub fn generate_ecdh_secret<PK>(secret_key: &PK::K, public_key: &PK) -> CipherKey
where PK: PublicKey + DiffieHellmanSharedSecret<PK = PK> {
    // TODO: PK will still leave the secret in released memory. Implementing Zerioze on RistrettoPublicKey is not
    //       currently possible because (Compressed)RistrettoPoint does not implement it.
    let k = PK::shared_secret(secret_key, public_key);
    CipherKey(*Key::from_slice(k.as_bytes()))
}

/// Decrypts cipher text using ChaCha20 stream cipher given the cipher key and cipher text with integral nonce.
pub fn decrypt(cipher_key: &CipherKey, cipher_text: &[u8]) -> Result<Vec<u8>, DhtOutboundError> {
    if cipher_text.len() < size_of::<Nonce>() {
        return Err(DhtOutboundError::CipherError(
            "Cipher text is not long enough to include nonce".to_string(),
        ));
    }

    let (nonce, cipher_text) = cipher_text.split_at(size_of::<Nonce>());
    let nonce = Nonce::from_slice(nonce);
    let mut cipher_text = cipher_text.to_vec();

    let mut cipher = ChaCha20::new(&cipher_key.0, nonce);
    cipher.apply_keystream(cipher_text.as_mut_slice());
    Ok(cipher_text)
}

/// Encrypt the plain text using the ChaCha20 stream cipher
pub fn encrypt(cipher_key: &CipherKey, plain_text: &[u8]) -> Vec<u8> {
    let mut nonce = [0u8; size_of::<Nonce>()];
    OsRng.fill_bytes(&mut nonce);

    let nonce_ga = Nonce::from_slice(&nonce);
    let mut cipher = ChaCha20::new(&cipher_key.0, nonce_ga);

    let mut buf = vec![0u8; plain_text.len() + nonce.len()];
    buf[..nonce.len()].copy_from_slice(&nonce[..]);
    buf[nonce.len()..].copy_from_slice(plain_text);
    cipher.apply_keystream(&mut buf[nonce.len()..]);
    buf
}

/// Generates a 32-byte hashed challenge that commits to the message header and body
pub fn create_message_challenge(header: &DhtMessageHeader, body: &[u8]) -> [u8; 32] {
    create_message_challenge_parts(
        header.version,
        &header.destination,
        header.message_type,
        header.flags,
        header.expires,
        header.ephemeral_public_key.as_ref(),
        body,
    )
}

/// Generates a 32-byte hashed challenge that commits to all message parts
pub fn create_message_challenge_parts(
    protocol_version: DhtProtocolVersion,
    destination: &NodeDestination,
    message_type: DhtMessageType,
    flags: DhtMessageFlags,
    expires: Option<EpochTime>,
    ephemeral_public_key: Option<&CommsPublicKey>,
    body: &[u8],
) -> [u8; 32] {
    let mut mac_challenge = Challenge::new();
    mac_challenge.update(&protocol_version.as_bytes());
    mac_challenge.update(destination.to_inner_bytes());
    mac_challenge.update(&(message_type as i32).to_le_bytes());
    mac_challenge.update(&flags.bits().to_le_bytes());
    let expires = expires.map(|t| t.as_u64().to_le_bytes()).unwrap_or_default();
    mac_challenge.update(&expires);

    let e_pk = ephemeral_public_key
        .map(|e_pk| {
            let mut buf = [0u8; 32];
            // CommsPublicKey::as_bytes returns 32-bytes
            buf.copy_from_slice(e_pk.as_bytes());
            buf
        })
        .unwrap_or_default();
    mac_challenge.update(&e_pk);

    mac_challenge.update(&body);
    mac_challenge.finalize_fixed().into()
}

#[cfg(test)]
mod test {
    use tari_utilities::hex::from_hex;

    use super::*;

    #[test]
    fn encrypt_decrypt() {
        let pk = CommsPublicKey::default();
        let key = CipherKey(*chacha20::Key::from_slice(pk.as_bytes()));
        let plain_text = "Last enemy position 0830h AJ 9863".as_bytes().to_vec();
        let encrypted = encrypt(&key, &plain_text);
        let decrypted = decrypt(&key, &encrypted).unwrap();
        assert_eq!(decrypted, plain_text);
    }

    #[test]
    fn decrypt_fn() {
        let pk = CommsPublicKey::default();
        let key = CipherKey(*chacha20::Key::from_slice(pk.as_bytes()));
        let cipher_text =
            from_hex("24bf9e698e14938e93c09e432274af7c143f8fb831f344f244ef02ca78a07ddc28b46fec536a0ca5c04737a604")
                .unwrap();
        let plain_text = decrypt(&key, &cipher_text).unwrap();
        let secret_msg = "Last enemy position 0830h AJ 9863".as_bytes().to_vec();
        assert_eq!(plain_text, secret_msg);
    }
}
