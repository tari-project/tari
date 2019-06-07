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

use rand::OsRng;
use std::sync::{Arc, Mutex};
use tari_comms::{
    connection::NetAddress,
    peer_manager::{CommsNodeIdentity, NodeId, PeerNodeIdentity},
};
use tari_crypto::{
    keys::{PublicKey, SecretKey},
    ristretto::{RistrettoPublicKey, RistrettoSecretKey},
};

lazy_static! {
    static ref IDENTITY_RACE_CONDITION_LOCK: Mutex<()> = Mutex::new(());
}

/// Sets the global node identity using random values
pub fn set_test_node_identity() -> Arc<CommsNodeIdentity> {
    let _lock = IDENTITY_RACE_CONDITION_LOCK.lock().unwrap();
    match CommsNodeIdentity::global() {
        Some(identity) => identity,
        None => {
            // Generate a test identity, set it and return it
            let secret_key = RistrettoSecretKey::random(&mut OsRng::new().unwrap());
            let public_key = RistrettoPublicKey::from_secret_key(&secret_key);
            let node_id = NodeId::from_key(&public_key).unwrap();
            let node_identity = CommsNodeIdentity {
                identity: PeerNodeIdentity::new(node_id, public_key),
                secret_key,
                control_service_address: "127.0.0.1:9090".parse::<NetAddress>().unwrap(),
            };

            CommsNodeIdentity::set_global(node_identity)
        },
    }
}
