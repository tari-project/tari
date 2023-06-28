//  Copyright 2022. The Tari Project
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

use borsh::{BorshDeserialize, BorshSerialize};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use tari_common_types::types::{FixedHash, PrivateKey, PublicKey, Signature};
use tari_crypto::{hash::blake2::Blake256, hash_domain, hashing::DomainSeparatedHasher, keys::PublicKey as PublicKeyT};
use tari_utilities::ByteArray;

hash_domain!(ValidatorNodeHashDomain, "com.tari.dan_layer.validator_node", 0);

#[derive(Debug, Clone, Hash, PartialEq, Eq, Deserialize, Serialize, BorshSerialize, BorshDeserialize)]
pub struct ValidatorNodeSignature {
    public_key: PublicKey,
    signature: Signature,
}

impl ValidatorNodeSignature {
    pub fn new(public_key: PublicKey, signature: Signature) -> Self {
        Self { public_key, signature }
    }

    pub fn sign(private_key: &PrivateKey, msg: &[u8]) -> Self {
        let (secret_nonce, public_nonce) = PublicKey::random_keypair(&mut OsRng);
        let public_key = PublicKey::from_secret_key(private_key);
        let challenge = Self::construct_challenge(&public_key, &public_nonce, msg);
        let signature = Signature::sign_raw(private_key, secret_nonce, &*challenge)
            .expect("Sign cannot fail with 32-byte challenge and a RistrettoPublicKey");
        Self { public_key, signature }
    }

    fn construct_challenge(public_key: &PublicKey, public_nonce: &PublicKey, msg: &[u8]) -> FixedHash {
        let hasher = DomainSeparatedHasher::<Blake256, ValidatorNodeHashDomain>::new_with_label("registration")
            .chain(public_key.as_bytes())
            .chain(public_nonce.as_bytes())
            .chain(msg);
        digest::Digest::finalize(hasher).into()
    }

    pub fn is_valid_signature_for(&self, msg: &[u8]) -> bool {
        let challenge = Self::construct_challenge(&self.public_key, self.signature.get_public_nonce(), msg);
        self.signature.verify_challenge(&self.public_key, &*challenge)
    }

    pub fn public_key(&self) -> &PublicKey {
        &self.public_key
    }

    pub fn signature(&self) -> &Signature {
        &self.signature
    }
}
