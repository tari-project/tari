// Copyright 2019. The Tari Project
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

use crate::error::WalletError;
use rand::rngs::OsRng;
use tari_common_types::types::{PrivateKey, PublicKey};
use tari_crypto::{common::Blake256, keys::PublicKey as OtherPublicKey};
use tari_key_manager::key_manager::KeyManager;

/// Specify the Hash function used by the key manager
pub type KeyDigest = Blake256;

/// Specify the Hash function used when constructing challenges during transaction building
pub type HashDigest = Blake256;

pub(crate) trait PersistentKeyManager {
    fn create_and_store_new(&mut self) -> Result<PublicKey, WalletError>;
}

pub(crate) struct MockPersistentKeyManager {
    key_manager: KeyManager<PrivateKey, KeyDigest>,
}

impl MockPersistentKeyManager {
    pub fn new() -> Self {
        todo!()
        // Self {
        //     key_manager: KeyManager::new(&mut OsRng),
        // }
    }
}

impl PersistentKeyManager for MockPersistentKeyManager {
    fn create_and_store_new(&mut self) -> Result<PublicKey, WalletError> {
        Ok(PublicKey::from_secret_key(&self.key_manager.next_key().unwrap().k))
    }
}
