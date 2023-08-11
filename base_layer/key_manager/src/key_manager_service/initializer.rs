//  Copyright 2022, The Tari Project
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

// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::marker::PhantomData;

use tari_crypto::keys::PublicKey;
use tari_service_framework::{async_trait, ServiceInitializationError, ServiceInitializer, ServiceInitializerContext};

use crate::{
    cipher_seed::CipherSeed,
    key_manager_service::{
        storage::database::{KeyManagerBackend, KeyManagerDatabase},
        KeyManagerHandle,
    },
};

/// Initializes the key manager service by implementing the [ServiceInitializer] trait.
pub struct KeyManagerInitializer<T, PK>
where
    T: KeyManagerBackend<PK>,
    PK: PublicKey,
{
    backend: Option<T>,
    master_seed: CipherSeed,
    public_key_type: PhantomData<PK>,
}

impl<T, PK> KeyManagerInitializer<T, PK>
where
    T: KeyManagerBackend<PK> + 'static,
    PK: PublicKey,
{
    /// Creates a new [KeyManagerInitializer] from the provided [KeyManagerBackend] and [CipherSeed]
    pub fn new(backend: T, master_seed: CipherSeed) -> Self {
        Self {
            backend: Some(backend),
            master_seed,
            public_key_type: PhantomData,
        }
    }
}

#[async_trait]
impl<T, PK> ServiceInitializer for KeyManagerInitializer<T, PK>
where
    T: KeyManagerBackend<PK> + 'static,
    PK: PublicKey + Sync + Send + 'static,
{
    async fn initialize(&mut self, context: ServiceInitializerContext) -> Result<(), ServiceInitializationError> {
        let backend = self
            .backend
            .take()
            .expect("Cannot start Key Manager Service without setting a storage backend");

        let key_manager: KeyManagerHandle<T, PK> =
            KeyManagerHandle::new(self.master_seed.clone(), KeyManagerDatabase::new(backend));
        context.register_handle(key_manager);

        Ok(())
    }
}
