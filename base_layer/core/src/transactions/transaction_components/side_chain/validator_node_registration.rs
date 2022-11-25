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
use tari_crypto::keys::PublicKey as PublicKeyT;

use crate::{consensus::DomainSeparatedConsensusHasher, transactions::TransactionHashDomain};

#[derive(Debug, Clone, Hash, PartialEq, Eq, Deserialize, Serialize, BorshSerialize, BorshDeserialize)]
pub struct ValidatorNodeRegistration {
    pub public_key: PublicKey,
    pub signature: Signature,
}

impl ValidatorNodeRegistration {
    pub fn is_valid_signature_for(&self, msg: &[u8]) -> bool {
        let challenge = Self::construct_challenge(&self.public_key, self.signature.get_public_nonce(), msg);
        self.signature.verify_challenge(&self.public_key, &*challenge)
    }

    pub fn new_signed(private_key: &PrivateKey, msg: &[u8]) -> Self {
        let (secret_nonce, public_nonce) = PublicKey::random_keypair(&mut OsRng);
        let public_key = PublicKey::from_secret_key(private_key);
        let challenge = Self::construct_challenge(&public_key, &public_nonce, msg);
        let signature = Signature::sign_raw(private_key, secret_nonce, &*challenge)
            .expect("Sign cannot fail with 32-byte challenge and a RistrettoPublicKey");
        Self { public_key, signature }
    }

    pub fn construct_challenge(public_key: &PublicKey, public_nonce: &PublicKey, msg: &[u8]) -> FixedHash {
        DomainSeparatedConsensusHasher::<TransactionHashDomain>::new("validator_node_registration")
            .chain(public_key)
            .chain(public_nonce)
            .chain(&msg)
            .finalize()
            .into()
    }

    pub fn derive_shard_key(&self, block_hash: &FixedHash) -> [u8; 32] {
        DomainSeparatedConsensusHasher::<TransactionHashDomain>::new("validator_node_root")
            // <pk, sig>
            .chain(self)
            .chain(block_hash)
            .finalize()
    }
}

#[cfg(test)]
mod test {
    use rand::rngs::OsRng;
    use tari_crypto::keys::SecretKey;

    use super::*;

    fn create_instance() -> ValidatorNodeRegistration {
        let sk = PrivateKey::random(&mut OsRng);
        ValidatorNodeRegistration::new_signed(&sk, b"valid")
    }

    mod is_valid_signature_for {
        use super::*;

        #[test]
        fn it_returns_true_for_invalid_signature() {
            let reg = create_instance();
            assert!(reg.is_valid_signature_for(b"valid"));
        }

        #[test]
        fn it_returns_false_for_invalid_challenge() {
            let reg = create_instance();
            assert!(!reg.is_valid_signature_for(b"there's wally"));
        }

        #[test]
        fn it_returns_false_for_invalid_signature() {
            let mut reg = create_instance();
            reg.public_key = create_instance().public_key;
            assert!(!reg.is_valid_signature_for(b"valid"));
        }
    }
}
