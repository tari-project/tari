//  Copyright 2020, The Tari Project
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
use super::{handle::BlockchainStateServiceHandle, service::BlockchainStateService};
use crate::chain_storage::{async_db::AsyncBlockchainDb, BlockchainBackend};
use futures::{channel::mpsc, future};
use tari_service_framework::{ServiceInitializationError, ServiceInitializer, ServiceInitializerContext};

/// Initializer for the blockchain state service. This service provides a service interface to to the blockchain state
/// database.
pub struct BlockchainStateServiceInitializer<T> {
    blockchain_db: AsyncBlockchainDb<T>,
}

impl<T> BlockchainStateServiceInitializer<T>
where T: BlockchainBackend
{
    pub fn new(blockchain_db: AsyncBlockchainDb<T>) -> Self {
        Self { blockchain_db }
    }
}

impl<T> ServiceInitializer for BlockchainStateServiceInitializer<T>
where T: BlockchainBackend + 'static
{
    type Future = future::Ready<Result<(), ServiceInitializationError>>;

    fn initialize(&mut self, context: ServiceInitializerContext) -> Self::Future {
        let blockchain_db = self.blockchain_db.clone();
        let (request_tx, request_rx) = mpsc::channel(10);
        let handle = BlockchainStateServiceHandle::new(request_tx);
        context.register_handle(handle);
        context.spawn_until_shutdown(move |_| BlockchainStateService::new(blockchain_db, request_rx).run());
        future::ready(Ok(()))
    }
}
