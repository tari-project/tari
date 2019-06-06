//  Copyright 2019 The Tari Project
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

use rand::{CryptoRng, OsRng, Rng};

use tari_crypto::ristretto::RistrettoPublicKey;
use tari_utilities::{ByteArray, Hashable};

use crate::peer_manager::NodeId;
use tari_crypto::keys::PublicKey;

/// Creates a random node ID witht he given RNG
pub fn make_random_node_id_with_rng<RNG: CryptoRng + Rng>(rng: &mut RNG) -> NodeId {
    let (_sk, pk) = RistrettoPublicKey::random_keypair(rng);
    NodeId::from_key(&pk).unwrap()
}

/// Creates a random node ID using OsRng
pub fn make_node_id() -> NodeId {
    make_random_node_id_with_rng(&mut OsRng::new().unwrap())
}

/// Creates a random node ID using OsRng
pub fn make_node_id_from_public_key<P: PublicKey + Hashable>(pk: &P) -> NodeId {
    NodeId::from_key(pk).unwrap()
}

/// Creates the same node ID
pub fn make_dummy_node_id() -> NodeId {
    NodeId::try_from(
        [
            144, 28, 106, 112, 220, 197, 216, 119, 9, 217, 42, 77, 159, 211, 53, 207, 0, 157, 5, 55, 235, 247, 160,
            195, 240, 48, 146, 168, 119, 15, 241, 54,
        ]
        .as_bytes(),
    )
    .unwrap()
}
