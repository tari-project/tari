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
use serde::{Deserialize, Serialize};
use tari_common_types::{
    epoch::VnEpoch,
    types::{CompressedPublicKey, CompressedSignature, PrivateKey, UncompressedSignature},
};
use tari_hashing::layer2::{validator_exit_hasher, validator_registration_hasher};

#[derive(Default, Debug, Clone, PartialEq, Eq, Deserialize, Serialize, BorshSerialize, BorshDeserialize)]
pub struct ValidatorNodeSignature {
    public_key: CompressedPublicKey,
    signature: CompressedSignature,
}

impl ValidatorNodeSignature {
    pub fn new(public_key: CompressedPublicKey, signature: CompressedSignature) -> Self {
        Self { public_key, signature }
    }

    /// Signs a validator node registration. `network` is the network byte (`Network::as_byte`) of the chain the
    /// registration is intended for; it is part of the signed message so that the signature cannot be replayed on
    /// another network.
    pub fn sign_for_registration(
        private_key: &PrivateKey,
        network: u8,
        sidechain_pk: Option<&CompressedPublicKey>,
        claim_public_key: &CompressedPublicKey,
        max_epoch: VnEpoch,
    ) -> Self {
        let (secret_nonce, public_nonce) = CompressedPublicKey::random_keypair(&mut rand::rng());
        let public_key = CompressedPublicKey::from_secret_key(private_key);
        let message = Self::construct_registration_signature_message(
            network,
            &public_key,
            &public_nonce,
            sidechain_pk,
            claim_public_key,
            max_epoch,
        );
        let signature = UncompressedSignature::sign_raw_uniform(private_key, secret_nonce, &message)
            .expect("Sign cannot fail with 64-byte challenge and a RistrettoPublicKey");
        Self {
            public_key,
            signature: CompressedSignature::new_from_schnorr(signature),
        }
    }

    /// Signs a validator node exit. `network` is the network byte (`Network::as_byte`) of the chain the exit is
    /// intended for. `activation_epoch` is the activation epoch of the registration being exited; including it binds
    /// the exit to that specific registration instance, so it cannot be replayed against a later re-registration of
    /// the same validator node.
    pub fn sign_for_exit(
        private_key: &PrivateKey,
        network: u8,
        sidechain_pk: Option<&CompressedPublicKey>,
        activation_epoch: VnEpoch,
        max_epoch: VnEpoch,
    ) -> Self {
        let (secret_nonce, public_nonce) = CompressedPublicKey::random_keypair(&mut rand::rng());
        let public_key = CompressedPublicKey::from_secret_key(private_key);
        let message = Self::construct_exit_signature_message(
            network,
            &public_key,
            &public_nonce,
            sidechain_pk,
            activation_epoch,
            max_epoch,
        );
        let signature = UncompressedSignature::sign_raw_uniform(private_key, secret_nonce, &message)
            .expect("Sign cannot fail with 64-byte challenge and a RistrettoPublicKey");
        Self {
            public_key,
            signature: CompressedSignature::new_from_schnorr(signature),
        }
    }

    fn construct_registration_signature_message(
        network: u8,
        public_key: &CompressedPublicKey,
        public_nonce: &CompressedPublicKey,
        sidechain_pk: Option<&CompressedPublicKey>,
        claim_public_key: &CompressedPublicKey,
        max_epoch: VnEpoch,
    ) -> [u8; 64] {
        validator_registration_hasher(network)
            .chain(public_key)
            .chain(public_nonce)
            .chain(&sidechain_pk)
            .chain(claim_public_key)
            .chain(&max_epoch)
            .finalize_into_array()
    }

    fn construct_exit_signature_message(
        network: u8,
        public_key: &CompressedPublicKey,
        public_nonce: &CompressedPublicKey,
        sidechain_pk: Option<&CompressedPublicKey>,
        activation_epoch: VnEpoch,
        max_epoch: VnEpoch,
    ) -> [u8; 64] {
        validator_exit_hasher(network)
            .chain(public_key)
            .chain(public_nonce)
            .chain(&sidechain_pk)
            .chain(&activation_epoch)
            .chain(&max_epoch)
            .finalize_into_array()
    }

    pub fn is_valid_registration_signature_for(
        &self,
        network: u8,
        sidechain_pk: Option<&CompressedPublicKey>,
        claim_public_key: &CompressedPublicKey,
        max_epoch: VnEpoch,
    ) -> bool {
        let message = Self::construct_registration_signature_message(
            network,
            &self.public_key,
            self.signature.get_compressed_public_nonce(),
            sidechain_pk,
            claim_public_key,
            max_epoch,
        );
        match (self.signature.to_schnorr_signature(), self.public_key.to_public_key()) {
            (Ok(sig), Ok(public_key)) => sig.verify_raw_uniform(&public_key, &message),
            _ => false,
        }
    }

    pub fn is_valid_exit_signature_for(
        &self,
        network: u8,
        sidechain_pk: Option<&CompressedPublicKey>,
        activation_epoch: VnEpoch,
        max_epoch: VnEpoch,
    ) -> bool {
        let message = Self::construct_exit_signature_message(
            network,
            &self.public_key,
            self.signature.get_compressed_public_nonce(),
            sidechain_pk,
            activation_epoch,
            max_epoch,
        );
        match (self.signature.to_schnorr_signature(), self.public_key.to_public_key()) {
            (Ok(sig), Ok(public_key)) => sig.verify_raw_uniform(&public_key, &message),
            _ => false,
        }
    }

    pub fn public_key(&self) -> &CompressedPublicKey {
        &self.public_key
    }

    pub fn signature(&self) -> &CompressedSignature {
        &self.signature
    }
}

#[cfg(test)]
mod test {
    use tari_crypto::keys::SecretKey;

    use super::*;

    const IGOR: u8 = 0x24;
    const LOCALNET: u8 = 0x10;

    #[test]
    fn registration_signature_is_bound_to_the_network() {
        let sk = PrivateKey::random(&mut rand::rng());
        let claim = CompressedPublicKey::from_secret_key(&sk);
        let sig = ValidatorNodeSignature::sign_for_registration(&sk, IGOR, None, &claim, VnEpoch(5));
        assert!(sig.is_valid_registration_signature_for(IGOR, None, &claim, VnEpoch(5)));
        assert!(!sig.is_valid_registration_signature_for(LOCALNET, None, &claim, VnEpoch(5)));
    }

    #[test]
    fn exit_signature_is_bound_to_the_network_and_activation_epoch() {
        let sk = PrivateKey::random(&mut rand::rng());
        let sig = ValidatorNodeSignature::sign_for_exit(&sk, IGOR, None, VnEpoch(3), VnEpoch(5));
        assert!(sig.is_valid_exit_signature_for(IGOR, None, VnEpoch(3), VnEpoch(5)));
        assert!(!sig.is_valid_exit_signature_for(LOCALNET, None, VnEpoch(3), VnEpoch(5)));
        assert!(!sig.is_valid_exit_signature_for(IGOR, None, VnEpoch(4), VnEpoch(5)));
    }

    #[test]
    fn registration_and_exit_signatures_are_not_interchangeable() {
        // With a zero claim key / activation epoch the two messages used to differ only by layout. The distinct
        // labels ("vn_registration" vs "vn_exit") separate them regardless of the remaining fields.
        let sk = PrivateKey::random(&mut rand::rng());
        let claim = CompressedPublicKey::default();
        let reg = ValidatorNodeSignature::sign_for_registration(&sk, IGOR, None, &claim, VnEpoch(5));
        assert!(!reg.is_valid_exit_signature_for(IGOR, None, VnEpoch(0), VnEpoch(5)));
        let exit = ValidatorNodeSignature::sign_for_exit(&sk, IGOR, None, VnEpoch(0), VnEpoch(5));
        assert!(!exit.is_valid_registration_signature_for(IGOR, None, &claim, VnEpoch(5)));
    }
}
