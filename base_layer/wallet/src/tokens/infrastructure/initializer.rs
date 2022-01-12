// Copyright 2021. The Tari Project
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

use futures::future;
use log::*;
use tari_service_framework::{
    async_trait,
    reply_channel,
    ServiceInitializationError,
    ServiceInitializer,
    ServiceInitializerContext,
};

use crate::{
    output_manager_service::{handle::OutputManagerHandle, storage::database::OutputManagerBackend},
    tokens::{infrastructure::token_manager_service::TokenManagerService, TokenManagerHandle},
};

const LOG_TARGET: &str = "wallet::assets::infrastructure::initializer";

pub struct TokenManagerServiceInitializer<T>
where T: OutputManagerBackend
{
    backend: Option<T>,
}

impl<T> TokenManagerServiceInitializer<T>
where T: OutputManagerBackend + 'static
{
    pub fn new(backend: T) -> Self {
        Self { backend: Some(backend) }
    }
}

#[async_trait]
impl<T> ServiceInitializer for TokenManagerServiceInitializer<T>
where T: OutputManagerBackend + 'static
{
    async fn initialize(&mut self, context: ServiceInitializerContext) -> Result<(), ServiceInitializationError> {
        let (sender, receiver) = reply_channel::unbounded();

        let handle = TokenManagerHandle::new(sender);
        context.register_handle(handle);

        let backend = self.backend.take().expect("this expect pattern is dumb");

        context.spawn_when_ready(move |handles| async move {
            let output_manager = handles.expect_handle::<OutputManagerHandle>();
            // let transaction_service = handles.expect_handle::<TransactionServiceHandle>();
            let service = TokenManagerService::new(backend, output_manager);

            let running = service.start(handles.get_shutdown_signal(), receiver);

            futures::pin_mut!(running);
            future::select(running, handles.get_shutdown_signal()).await;
            info!(target: LOG_TARGET, "Token Manager Service shutdown");
        });
        Ok(())
    }
}
