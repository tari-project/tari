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

use crate::peer_manager::{Peer, PeerId};
use tari_crypto::{common::Blake256, keys::PublicKey, ristretto::RistrettoPublicKey};
use tari_storage::lmdb_store::LMDBStore;
#[cfg(test)]
use tari_storage::HashmapDatabase;
#[cfg(not(test))]
use tari_storage::LMDBWrapper;
use tari_utilities::ciphers::chacha20::ChaCha20;

/// The default port that control services listen on
pub const DEFAULT_CONTROL_PORT_ADDRESS: &str = "/ip4/0.0.0.0/tcp/7899";
pub const DEFAULT_LISTENER_ADDRESS: &str = "/ip4/0.0.0.0/tcp/7898";

/// Specify the digest type for the signature challenges
pub type Challenge = Blake256;

/// Public key type
pub type CommsPublicKey = RistrettoPublicKey;
pub type CommsSecretKey = <CommsPublicKey as PublicKey>::K;

/// Specify the RNG that should be used for random selection
pub type CommsRng = rand::rngs::OsRng;

/// Specify what cipher to use for encryption/decryption
pub type CommsCipher = ChaCha20;

/// Datastore and Database used for persistence storage
pub type CommsDataStore = LMDBStore;

#[cfg(not(test))]
pub type CommsDatabase = LMDBWrapper<PeerId, Peer>;
#[cfg(test)]
pub type CommsDatabase = HashmapDatabase<PeerId, Peer>;
