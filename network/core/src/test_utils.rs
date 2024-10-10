// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use rand::rngs::OsRng;
use tari_crypto::{keys::PublicKey, ristretto::RistrettoPublicKey};

use crate::{identity, identity::PeerId};

pub fn random_peer_id() -> PeerId {
    let (_secret_key, public_key) = RistrettoPublicKey::random_keypair(&mut OsRng);
    PeerId::from_public_key(&identity::PublicKey::from(identity::sr25519::PublicKey::from(
        public_key,
    )))
}
