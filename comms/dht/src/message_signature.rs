//  Copyright 2022, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::convert::TryFrom;

use rand::rngs::OsRng;
use tari_comms::types::{CommsPublicKey, CommsSecretKey, Signature};
use tari_crypto::keys::PublicKey;
use tari_utilities::ByteArray;

use crate::comms_dht_hash_domain_message_signature;

#[derive(Debug, Clone)]
pub struct MessageSignature {
    signer_public_key: CommsPublicKey,
    signature: Signature,
}

fn construct_message_signature_hash(
    signer_public_key: &CommsPublicKey,
    public_nonce: &CommsPublicKey,
    message: &[u8],
) -> [u8; 32] {
    // produce domain separated hash of input data, in such a way that e = H_mac(P||R||m)
    let domain_separated_hash = comms_dht_hash_domain_message_signature()
        .chain(signer_public_key.as_bytes())
        .chain(public_nonce.as_bytes())
        .chain(message)
        .finalize();

    let mut output = [0u8; 32];
    output.copy_from_slice(domain_separated_hash.as_ref());

    output
}

impl MessageSignature {
    /// Create a new signed [MessageSignature](self::MessageSignature) for the given message.
    pub fn new_signed(signer_secret_key: CommsSecretKey, message: &[u8]) -> Self {
        let (nonce_s, nonce_pk) = CommsPublicKey::random_keypair(&mut OsRng);
        let signer_public_key = CommsPublicKey::from_secret_key(&signer_secret_key);
        let challenge = construct_message_signature_hash(&signer_public_key, &nonce_pk, message);
        let signature = Signature::sign_raw(&signer_secret_key, nonce_s, &challenge)
            .expect("challenge is [u8;32] but SchnorrSignature::sign failed");

        Self {
            signer_public_key,
            signature,
        }
    }

    /// Returns true if the provided message valid for this message signature, otherwise false.
    pub fn verify(&self, message: &[u8]) -> bool {
        let challenge =
            construct_message_signature_hash(&self.signer_public_key, self.signature.get_public_nonce(), message);
        self.signature.verify_challenge(&self.signer_public_key, &challenge)
    }

    /// Consume this instance, returning the public key of the signer.
    pub fn into_signer_public_key(self) -> CommsPublicKey {
        self.signer_public_key
    }

    /// Converts to a protobuf struct
    pub fn to_proto(&self) -> ProtoMessageSignature {
        ProtoMessageSignature {
            signer_public_key: self.signer_public_key.to_vec(),
            public_nonce: self.signature.get_public_nonce().to_vec(),
            signature: self.signature.get_signature().to_vec(),
        }
    }
}

impl TryFrom<ProtoMessageSignature> for MessageSignature {
    type Error = MessageSignatureError;

    fn try_from(message_signature: ProtoMessageSignature) -> Result<Self, Self::Error> {
        let signer_public_key = CommsPublicKey::from_bytes(&message_signature.signer_public_key)
            .map_err(|_| MessageSignatureError::InvalidSignerPublicKeyBytes)?;

        let public_nonce = CommsPublicKey::from_bytes(&message_signature.public_nonce)
            .map_err(|_| MessageSignatureError::InvalidPublicNonceBytes)?;

        let signature = CommsSecretKey::from_bytes(&message_signature.signature)
            .map_err(|_| MessageSignatureError::InvalidSignatureBytes)?;

        Ok(Self {
            signer_public_key,
            signature: Signature::new(public_nonce, signature),
        })
    }
}

/// The Message Authentication Code (MAC) message format of the decrypted `DhtHeader::message_signature` field
#[derive(Clone, prost::Message)]
pub struct ProtoMessageSignature {
    #[prost(bytes, tag = "1")]
    pub signer_public_key: Vec<u8>,
    #[prost(bytes, tag = "2")]
    pub public_nonce: Vec<u8>,
    #[prost(bytes, tag = "3")]
    pub signature: Vec<u8>,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum MessageSignatureError {
    #[error("Message signature does not contain valid scalar bytes")]
    InvalidSignatureBytes,
    #[error("Message signature contained an invalid public nonce")]
    InvalidPublicNonceBytes,
    #[error("Message signature contained an invalid signer public key")]
    InvalidSignerPublicKeyBytes,
    #[error("Message signature failed to verify")]
    VerificationFailed,
}

#[cfg(test)]
mod test {
    use tari_crypto::keys::SecretKey;

    use super::*;
    const MSG: &[u8] = b"100% genuine";

    fn setup() -> (MessageSignature, CommsSecretKey) {
        let signer_k = CommsSecretKey::random(&mut OsRng);
        (MessageSignature::new_signed(signer_k.clone(), MSG), signer_k)
    }

    #[test]
    fn it_secures_the_message() {
        let (mac, _) = setup();
        assert!(mac.verify(MSG));
        assert!(!mac.verify(b"99.9% genuine"));
    }

    #[test]
    fn it_is_secure_against_related_key_attack() {
        let (mut mac, signer_k) = setup();
        let signer_pk = CommsPublicKey::from_secret_key(&signer_k);
        let msg = construct_message_signature_hash(&signer_pk, mac.signature.get_public_nonce(), MSG);
        let msg_scalar = CommsSecretKey::from_bytes(&msg).unwrap();

        // Some `a` key
        let (bad_signer_k, bad_signer_pk) = CommsPublicKey::random_keypair(&mut OsRng);
        mac.signer_public_key = &bad_signer_pk + &signer_pk;
        // s' = s + e.a
        mac.signature = Signature::new(
            mac.signature.get_public_nonce().clone(),
            mac.signature.get_signature() + (&msg_scalar * bad_signer_k),
        );

        assert!(!mac.verify(MSG));
    }

    #[test]
    fn it_secures_the_public_nonce() {
        let (mut mac, signer_k) = setup();
        let (nonce_k, _) = CommsPublicKey::random_keypair(&mut OsRng);
        // Get the original hashed challenge
        let signer_pk = CommsPublicKey::from_secret_key(&signer_k);
        let msg = construct_message_signature_hash(&signer_pk, mac.signature.get_public_nonce(), MSG);

        // Change <R, s> to <R', s>. Note: We need signer_k because the Signature interface does not provide a way to
        // change just the public nonce, an attacker does not need the secret key.
        mac.signature = Signature::sign_raw(&signer_k, nonce_k, &msg).unwrap();
        assert!(!mac.verify(MSG));
    }
}
