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

use crate::text_message_service::{TextMessageService, TextMessageServiceApi};
use derive_error::Error;
use std::sync::Arc;
use tari_comms::{builder::CommsServices, types::CommsPublicKey};
use tari_p2p::{
    initialization::{initialize_comms, CommsConfig, CommsInitializationError},
    ping_pong::{PingPongService, PingPongServiceApi},
    services::{ServiceExecutor, ServiceRegistry},
    tari_message::TariMessageType,
};

#[derive(Debug, Error)]
pub enum WalletError {
    CommsInitializationError(CommsInitializationError),
}

#[derive(Clone)]
pub struct WalletConfig {
    pub comms: CommsConfig,
    pub public_key: CommsPublicKey,
}

/// A structure containing the config and services that a Wallet application will require. This struct will start up all
/// the services and provide the APIs that applications will use to interact with the services
pub struct Wallet {
    pub ping_pong_service: Arc<PingPongServiceApi>,
    pub text_message_service: Arc<TextMessageServiceApi>,
    pub comms_services: Arc<CommsServices<TariMessageType>>,
    pub service_executor: Arc<ServiceExecutor>,
}

impl Wallet {
    pub fn new(config: WalletConfig) -> Result<Wallet, WalletError> {
        let ping_pong_service = PingPongService::new();
        let ping_pong_service_api = ping_pong_service.get_api();

        let text_message_service = TextMessageService::new(config.public_key.clone());
        let text_message_service_api = text_message_service.get_api();

        let registry = ServiceRegistry::new()
            .register(ping_pong_service)
            .register(text_message_service);

        let comm_routes = registry.build_comms_routes();
        let comms_services = initialize_comms(config.comms.clone(), comm_routes)?;
        let service_executor = ServiceExecutor::execute(comms_services.clone(), registry);

        Ok(Wallet {
            text_message_service: text_message_service_api,
            ping_pong_service: ping_pong_service_api,
            comms_services,
            service_executor: Arc::new(service_executor),
        })
    }
}
