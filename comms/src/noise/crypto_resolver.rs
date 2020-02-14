// Copyright 2019, The Tari Project
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
    types::{CommsPublicKey, CommsSecretKey},
};
use snow::{
    params::{CipherChoice, DHChoice, HashChoice},
    resolvers::{CryptoResolver, DefaultResolver},
    types::{Cipher, Dh, Hash, Random},
};
use tari_crypto::keys::{DiffieHellmanSharedSecret, PublicKey, SecretKey};
use tari_crypto::tari_utilities::ByteArray;
use rand::rngs::OsRng;

macro_rules! copy_slice {
    ($inslice:expr, $outslice:expr) => {
        $outslice[..$inslice.len()].copy_from_slice(&$inslice[..])
    };
}

#[derive(Default)]
pub struct TariCryptoResolver(DefaultResolver);

impl CryptoResolver for TariCryptoResolver {
    fn resolve_rng(&self) -> Option<Box<dyn Random>> {
        self.0.resolve_rng()
    }

    fn resolve_dh(&self, choice: &DHChoice) -> Option<Box<dyn Dh>> {
        match *choice {
            DHChoice::Curve25519 => Some(Box::new(CommsDiffieHellman::default())),
            _ => None,
        }
    }

    fn resolve_hash(&self, choice: &HashChoice) -> Option<Box<dyn Hash>> {
        self.0.resolve_hash(choice)
    }

    fn resolve_cipher(&self, choice: &CipherChoice) -> Option<Box<dyn Cipher>> {
        self.0.resolve_cipher(choice)
    }
}

#[derive(Default)]
struct CommsDiffieHellman {
    secret_key: CommsSecretKey,
    public_key: CommsPublicKey,
}

impl Dh for CommsDiffieHellman {
    fn name(&self) -> &'static str {
        static NAME: &str = "Ristretto";
        NAME
    }

    fn pub_len(&self) -> usize {
        CommsPublicKey::key_length()
    }

    fn priv_len(&self) -> usize {
        CommsSecretKey::key_length()
    }

    fn set(&mut self, privkey: &[u8]) {
        // `set` is used in the Builder, so this will panic if given an invalid secret key.
        self.secret_key = CommsSecretKey::from_bytes(privkey).expect("invalid secret key");
        self.public_key = CommsPublicKey::from_secret_key(&self.secret_key);
    }

    fn generate(&mut self, _: &mut dyn Random) {
        // `&mut dyn Random` is unsized and cannot be used with `CommsSecretKey::random`
        // COMMS_RNG fulfills the RNG requirements anyhow
        self.secret_key = CommsSecretKey::random(&mut OsRng);
        self.public_key = CommsPublicKey::from_secret_key(&self.secret_key);
    }

    fn pubkey(&self) -> &[u8] {
        &self.public_key.as_bytes()
    }

    fn privkey(&self) -> &[u8] {
        &self.secret_key.as_bytes()
    }

    fn dh(&self, public_key: &[u8], out: &mut [u8]) -> Result<(), ()> {
        let pk = CommsPublicKey::from_bytes(&public_key[..self.pub_len()]).map_err(|_| ())?;
        let shared = CommsPublicKey::shared_secret(&self.secret_key, &pk);
        let shared_bytes = shared.as_bytes();
        copy_slice!(shared_bytes, out);
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::noise::config::NOISE_IX_PARAMETER;
    use snow::Keypair;

    fn build_keypair() -> Keypair {
        snow::Builder::with_resolver(
            NOISE_IX_PARAMETER.parse().unwrap(),
            Box::new(TariCryptoResolver::default()),
        )
        .generate_keypair()
        .unwrap()
    }

    #[test]
    fn generate() {
        let keypair = build_keypair();

        let sk = CommsSecretKey::from_bytes(&keypair.private).unwrap();
        let expected_pk = CommsPublicKey::from_secret_key(&sk);
        let pk = CommsPublicKey::from_bytes(&keypair.public).unwrap();
        assert_eq!(pk, expected_pk);
    }

    #[test]
    fn dh() {
        let (secret_key, public_key) = CommsPublicKey::random_keypair(&mut OsRng);
        let dh = CommsDiffieHellman {
            public_key: public_key.clone(),
            secret_key: secret_key.clone(),
        };

        let (secret_key2, public_key2) = CommsPublicKey::random_keypair(&mut OsRng);
        let expected_shared = CommsPublicKey::shared_secret(&secret_key2, &public_key);

        let mut out = [0; 32];
        dh.dh(&public_key2.as_bytes(), &mut out).unwrap();
        let shared = CommsPublicKey::from_bytes(&out).unwrap();

        assert_eq!(shared, expected_shared);
    }
}
