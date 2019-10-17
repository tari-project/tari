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

// use crate::text_message_service::{handle::TextMessageHandle, TextMessageServiceInitializer};
use derive_error::Error;
use std::sync::Arc;
use tari_comms::{
    builder::{CommsError, CommsNode},
    types::CommsPublicKey,
};
use tari_comms_dht::Dht;
use tari_p2p::{
    comms_connector::pubsub_connector,
    initialization::{initialize_comms, CommsConfig, CommsInitializationError},
    services::{
        comms_outbound::CommsOutboundServiceInitializer,
        liveness::{handle::LivenessHandle, LivenessInitializer},
    },
};
use tari_service_framework::StackBuilder;
use tokio::runtime::Runtime;

#[derive(Debug, Error)]
pub enum WalletError {
    CommsInitializationError(CommsInitializationError),
    CommsError(CommsError),
}

#[derive(Clone)]
pub struct WalletConfig {
    pub comms_config: CommsConfig,
    pub inbound_message_buffer_size: usize,
    pub public_key: CommsPublicKey,
    pub database_path: String,
}

/// A structure containing the config and services that a Wallet application will require. This struct will start up all
/// the services and provide the APIs that applications will use to interact with the services
pub struct Wallet {
    pub comms: CommsNode,
    pub dht_service: Dht,
    //    pub text_message_service: TextMessageHandle,
    pub liveness_service: LivenessHandle,
    pub public_key: CommsPublicKey,
    pub runtime: Runtime,
}

impl Wallet {
    pub fn new(config: WalletConfig, runtime: Runtime) -> Result<Wallet, WalletError> {
        let (publisher, subscription_factory) =
            pubsub_connector(runtime.executor(), config.inbound_message_buffer_size);
        let subscription_factory = Arc::new(subscription_factory);

        let (comms, dht) = initialize_comms(runtime.executor(), config.comms_config, publisher)?;

        let fut = StackBuilder::new(runtime.executor(), comms.shutdown_signal())
            .add_initializer(CommsOutboundServiceInitializer::new(dht.outbound_requester()))
            .add_initializer(LivenessInitializer::new(Arc::clone(&subscription_factory)))
//            .add_initializer(TextMessageServiceInitializer::new(
//                subscription_factory,
//                config.public_key.clone(),
//                config.database_path,
//            ))
            .finish();

        let handles = runtime.block_on(fut).expect("Service initialization failed");

        Ok(Wallet {
            comms,
            dht_service: dht,
            //            text_message_service: handles
            //                .get_handle::<TextMessageHandle>()
            //                .expect("Could not get Text Message Service Handle"),
            liveness_service: handles
                .get_handle::<LivenessHandle>()
                .expect("Could not get Liveness Service Handle"),
            public_key: config.public_key.clone(),
            runtime,
        })
    }

    // This method consumes the wallet so that the handles are dropped which will result in the services async loops
    // exiting.
    pub fn shutdown(self) -> Result<(), WalletError> {
        self.comms.shutdown()?;
        Ok(())
    }
}
