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
use tari_common_types::{
    epoch::VnEpoch,
    types::{CompressedPublicKey, PrivateKey, Signature, UncompressedSignature},
};
use tari_hashing::layer2::validator_registration_hasher;

#[derive(Default, Debug, Clone, PartialEq, Eq, Deserialize, Serialize, BorshSerialize, BorshDeserialize)]
pub struct ValidatorNodeSignature {
    public_key: CompressedPublicKey,
    signature: Signature,
}

impl ValidatorNodeSignature {
    pub fn new(public_key: CompressedPublicKey, signature: Signature) -> Self {
        Self { public_key, signature }
    }

    pub fn sign_for_registration(
        private_key: &PrivateKey,
        sidechain_pk: Option<&CompressedPublicKey>,
        claim_public_key: &CompressedPublicKey,
        epoch: VnEpoch,
    ) -> Self {
        let (secret_nonce, public_nonce) = CompressedPublicKey::random_keypair(&mut OsRng);
        let public_key = CompressedPublicKey::from_secret_key(private_key);
        let message = Self::construct_registration_signature_message(
            &public_key,
            &public_nonce,
            sidechain_pk,
            claim_public_key,
            epoch,
        );
        let signature = UncompressedSignature::sign_raw_uniform(private_key, secret_nonce, &message)
            .expect("Sign cannot fail with 64-byte challenge and a RistrettoPublicKey");
        Self {
            public_key,
            signature: Signature::new_from_schnorr(signature),
        }
    }

    pub fn sign_for_exit(private_key: &PrivateKey, sidechain_pk: Option<&CompressedPublicKey>, epoch: VnEpoch) -> Self {
        let (secret_nonce, public_nonce) = CompressedPublicKey::random_keypair(&mut OsRng);
        let public_key = CompressedPublicKey::from_secret_key(private_key);
        let message = Self::construct_exit_signature_message(&public_key, &public_nonce, sidechain_pk, epoch);
        let signature = UncompressedSignature::sign_raw_uniform(private_key, secret_nonce, &message)
            .expect("Sign cannot fail with 64-byte challenge and a RistrettoPublicKey");
        Self {
            public_key,
            signature: Signature::new_from_schnorr(signature),
        }
    }

    fn construct_registration_signature_message(
        public_key: &CompressedPublicKey,
        public_nonce: &CompressedPublicKey,
        sidechain_pk: Option<&CompressedPublicKey>,
        claim_public_key: &CompressedPublicKey,
        epoch: VnEpoch,
    ) -> [u8; 64] {
        validator_registration_hasher()
            .chain(public_key)
            .chain(public_nonce)
            .chain(&sidechain_pk)
            .chain(claim_public_key)
            .chain(&epoch)
            .finalize_into_array()
    }

    fn construct_exit_signature_message(
        public_key: &CompressedPublicKey,
        public_nonce: &CompressedPublicKey,
        sidechain_pk: Option<&CompressedPublicKey>,
        epoch: VnEpoch,
    ) -> [u8; 64] {
        validator_registration_hasher()
            .chain(public_key)
            .chain(public_nonce)
            .chain(&sidechain_pk)
            .chain(&epoch)
            .finalize_into_array()
    }

    pub fn is_valid_registration_signature_for(
        &self,
        sidechain_pk: Option<&CompressedPublicKey>,
        claim_public_key: &CompressedPublicKey,
        epoch: VnEpoch,
    ) -> bool {
        let message = Self::construct_registration_signature_message(
            &self.public_key,
            self.signature.get_compressed_public_nonce(),
            sidechain_pk,
            claim_public_key,
            epoch,
        );
        match (self.signature.to_schnorr_signature(), self.public_key.to_public_key()) {
            (Ok(sig), Ok(public_key)) => sig.verify_raw_uniform(&public_key, &message),
            _ => false,
        }
    }

    pub fn is_valid_exit_signature_for(&self, sidechain_pk: Option<&CompressedPublicKey>, epoch: VnEpoch) -> bool {
        let message = Self::construct_exit_signature_message(
            &self.public_key,
            self.signature.get_compressed_public_nonce(),
            sidechain_pk,
            epoch,
        );
        match (self.signature.to_schnorr_signature(), self.public_key.to_public_key()) {
            (Ok(sig), Ok(public_key)) => sig.verify_raw_uniform(&public_key, &message),
            _ => false,
        }
    }

    pub fn public_key(&self) -> &CompressedPublicKey {
        &self.public_key
    }

    pub fn signature(&self) -> &Signature {
        &self.signature
    }
}
