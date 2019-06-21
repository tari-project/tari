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

// use crate::text_message_service::TextMessageService;
use derive_error::Error;
use std::sync::{Arc, RwLock};
use tari_p2p::{
    initialization::{initialize_comms, CommsConfig, CommsInitializationError},
    ping_pong::PingPongService,
    services::{ServiceExecutor, ServiceRegistry},
};

#[derive(Debug, Error)]
pub enum WalletError {
    CommsInitializationError(CommsInitializationError),
}

#[derive(Clone)]
pub struct WalletConfig {
    pub comms: CommsConfig,
}

pub struct Wallet {
    config: WalletConfig,
    ping_pong_service: Arc<RwLock<PingPongService>>,
    // text_message_service: Arc<TextMessageService>,
}

impl Wallet {
    pub fn new(config: WalletConfig) -> Wallet {
        Wallet {
            config: config.clone(),
            // text_message_service: Arc::new(TextMessageService::new(
            //   config.screen_name.clone(),
            //    config.comms.public_key.clone(),
            //)),
            ping_pong_service: Arc::new(RwLock::new(PingPongService::new())),
        }
    }

    pub fn start_services(&self) -> Result<ServiceExecutor, WalletError> {
        let registry = ServiceRegistry::new().register(self.ping_pong_service.clone());
        // let registry = ServiceRegistry::new().register(self.text_message_service.clone());

        let comm_routes = registry.build_comms_routes();

        let comms = initialize_comms(self.config.comms.clone(), comm_routes)?;

        Ok(ServiceExecutor::execute(comms, registry))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use tari_comms::{
        connection::NetAddress,
        control_service::ControlServiceConfig,
        types::{CommsPublicKey, CommsSecretKey},
    };
    use tari_crypto::keys::{PublicKey, SecretKey};
    use tari_p2p::tari_message::{NetMessage, TariMessageType};

    #[test]
    fn test_wallet() {
        let mut rng = rand::OsRng::new().unwrap();

        let listener_address1: NetAddress = "127.0.0.1:32775".parse().unwrap();
        let secret_key1 = CommsSecretKey::random(&mut rng);
        let public_key1 = CommsPublicKey::from_secret_key(&secret_key1);
        let config1 = WalletConfig {
            comms: CommsConfig {
                control_service: ControlServiceConfig {
                    listener_address: listener_address1,
                    socks_proxy_address: None,
                    accept_message_type: TariMessageType::new(NetMessage::Accept),
                },
                socks_proxy_address: None,
                host: "0.0.0.0".parse().unwrap(),
                public_key: public_key1,
                secret_key: secret_key1,
            },
            // screen_name: "Alice".to_string(),
        };

        let wallet1 = Wallet::new(config1);

        wallet1.start_services();

        let listener_address2: NetAddress = "127.0.0.1:32776".parse().unwrap();
        let secret_key2 = CommsSecretKey::random(&mut rng);
        let public_key2 = CommsPublicKey::from_secret_key(&secret_key2);
        let config2 = WalletConfig {
            comms: CommsConfig {
                control_service: ControlServiceConfig {
                    listener_address: listener_address2,
                    socks_proxy_address: None,
                    accept_message_type: TariMessageType::new(NetMessage::Accept),
                },
                socks_proxy_address: None,
                host: "0.0.0.0".parse().unwrap(),
                public_key: public_key2,
                secret_key: secret_key2,
            },
            // screen_name: "Alice".to_string(),
        };

        let wallet2 = Wallet::new(config2);

        wallet2.start_services();
    }
}
