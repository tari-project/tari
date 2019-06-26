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

use super::{
    error::ControlServiceError,
    service::ControlServiceConfig,
    types::{ControlMessage, ControlServiceDispatcher, ControlServiceMessageContext, Result},
};
use crate::{
    connection::{
        connection::EstablishedConnection,
        monitor::{ConnectionMonitor, SocketEvent, SocketEventType},
        types::Direction,
        Connection,
        InprocAddress,
        ZmqContext,
    },
    connection_manager::ConnectionManager,
    message::{Frame, FrameSet, Message, MessageEnvelope, MessageEnvelopeHeader, MessageFlags},
    peer_manager::NodeIdentity,
    types::{CommsCipher, CommsPublicKey},
};
use log::*;
use serde::{de::DeserializeOwned, Serialize};
use std::{
    convert::TryInto,
    sync::{
        mpsc::{sync_channel, Receiver, SyncSender},
        Arc,
    },
    thread,
    time::Duration,
};
use tari_crypto::keys::DiffieHellmanSharedSecret;
use tari_utilities::{byte_array::ByteArray, ciphers::cipher::Cipher, message_format::MessageFormat};

const LOG_TARGET: &'static str = "comms::control_service::worker";
/// The maximum message size allowed for the control service.
/// Messages will transparently drop if this size is exceeded.
const CONTROL_SERVICE_MAX_MSG_SIZE: u64 = 1024; // 1kb

/// The [ControlService] worker is responsible for handling incoming messages
/// to the control port and dispatching them using the message dispatcher.
pub struct ControlServiceWorker<MType>
where MType: Clone
{
    context: ZmqContext,
    config: ControlServiceConfig<MType>,
    monitor_address: InprocAddress,
    receiver: Receiver<ControlMessage>,
    is_running: bool,
    dispatcher: ControlServiceDispatcher<MType>,
    connection_manager: Arc<ConnectionManager>,
    node_identity: Arc<NodeIdentity>,
}

impl<MType> ControlServiceWorker<MType>
where
    MType: Send + Sync + 'static,
    MType: Serialize + DeserializeOwned,
    MType: Clone,
{
    /// Start the worker
    ///
    /// # Arguments
    /// - `context` - Connection context
    /// - `config` - ControlServiceConfig
    /// - `connection_manager` - the `ConnectionManager`
    pub fn start(
        context: ZmqContext,
        node_identity: Arc<NodeIdentity>,
        dispatcher: ControlServiceDispatcher<MType>,
        config: ControlServiceConfig<MType>,
        connection_manager: Arc<ConnectionManager>,
    ) -> Result<(thread::JoinHandle<Result<()>>, SyncSender<ControlMessage>)>
    {
        info!(
            target: LOG_TARGET,
            "Control service starting on {}...", config.listener_address
        );
        let (sender, receiver) = sync_channel(5);

        let mut worker = Self {
            config,
            connection_manager,
            context,
            dispatcher,
            is_running: false,
            monitor_address: InprocAddress::random(),
            node_identity,
            receiver,
        };

        let handle = thread::Builder::new()
            .name("control-service".to_string())
            .spawn(move || {
                loop {
                    match worker.main_loop() {
                        Ok(_) => {
                            info!(target: LOG_TARGET, "Control service exiting loop.");
                            break;
                        },

                        Err(err) => {
                            error!(target: LOG_TARGET, "Worker exited with an error: {:?}", err);
                            info!(target: LOG_TARGET, "Restarting control service after 1 second.");
                            thread::sleep(Duration::from_millis(1000));
                        },
                    }
                }

                Ok(())
            })
            .map_err(|_| ControlServiceError::WorkerThreadFailedToStart)?;

        Ok((handle, sender))
    }

    fn main_loop(&mut self) -> Result<()> {
        self.is_running = true;
        let monitor = ConnectionMonitor::connect(&self.context, &self.monitor_address)?;
        let listener = self.establish_listener()?;

        debug!(target: LOG_TARGET, "Control service started");
        loop {
            // Read incoming messages
            if let Some(frames) = connection_try!(listener.receive(100)) {
                debug!(target: LOG_TARGET, "Received {} frames", frames.len());
                match self.process_message(frames) {
                    Ok(_) => info!(target: LOG_TARGET, "Message processed"),
                    Err(err) => error!(target: LOG_TARGET, "Error when processing message: {:?}", err),
                }
            }

            // Read socket events
            if let Some(event) = connection_try!(monitor.read(5)) {
                debug!(target: LOG_TARGET, "Control service socket event: {:?}", event);
                self.process_socket_event(event)?;
            }

            // Process control messages
            self.process_control_messages()?;

            if !self.is_running {
                break;
            }
        }

        Ok(())
    }

    fn process_control_messages(&mut self) -> Result<()> {
        if let Some(msg) = self.receiver.recv_timeout(Duration::from_millis(5)).ok() {
            debug!(target: LOG_TARGET, "Received control message: {:?}", msg);
            match msg {
                ControlMessage::Shutdown => {
                    info!(target: LOG_TARGET, "Shutting down control service");
                    self.is_running = false;
                },
            }
        }
        Ok(())
    }

    fn process_message(&self, mut frames: FrameSet) -> Result<()> {
        // Discard the first identity frame (we aren't sending replies) and
        // build a message envelope
        let envelope: MessageEnvelope = frames
            .drain(1..)
            .collect::<FrameSet>()
            .try_into()
            .map_err(|err| ControlServiceError::MessageError(err))?;

        let envelope_header: MessageEnvelopeHeader<CommsPublicKey> = envelope.to_header()?;
        if !envelope_header.flags.contains(MessageFlags::ENCRYPTED) {
            return Err(ControlServiceError::ReceivedUnencryptedMessage);
        }

        let decrypted_body = self.decrypt_body(envelope.body_frame(), &envelope_header.source)?;
        let message =
            Message::from_binary(decrypted_body.as_bytes()).map_err(ControlServiceError::MessageFormatError)?;

        let context = ControlServiceMessageContext {
            envelope_header,
            message,
            connection_manager: self.connection_manager.clone(),
            peer_manager: self.connection_manager.get_peer_manager(),
            node_identity: self.node_identity.clone(),
            config: self.config.clone(),
        };

        debug!(target: LOG_TARGET, "Dispatching message");
        self.dispatcher.dispatch(context).map_err(|e| e.into())
    }

    fn decrypt_body(&self, body: &Frame, public_key: &CommsPublicKey) -> Result<Frame> {
        let ecdh_shared_secret = CommsPublicKey::shared_secret(&self.node_identity.secret_key, public_key).to_vec();
        CommsCipher::open_with_integral_nonce(&body, &ecdh_shared_secret).map_err(ControlServiceError::CipherError)
    }

    fn process_socket_event(&mut self, event: SocketEvent) -> Result<()> {
        use SocketEventType::*;

        match event.event_type {
            Listening => {
                info!(target: LOG_TARGET, "Started listening on '{}'", event.address);
            },
            Accepted => {
                info!(target: LOG_TARGET, "Accepted connection on '{}'", event.address);
            },
            Disconnected => {
                info!(target: LOG_TARGET, "'{}' Disconnected", event.address);
            },
            BindFailed => {
                self.is_running = false;
                error!(
                    target: LOG_TARGET,
                    "Failed to bind on '{}'. ControlService cannot start.", event.address
                );
            },
            Closed => {
                self.is_running = false;
                warn!(target: LOG_TARGET, "Underlying socket closed on '{}'", event.address);
            },
            _ => {},
        }

        Ok(())
    }

    fn establish_listener(&self) -> Result<EstablishedConnection> {
        debug!(
            target: LOG_TARGET,
            "Binding on address: {}", self.config.listener_address
        );
        Connection::new(&self.context, Direction::Inbound)
            .set_receive_hwm(10)
            .set_max_message_size(Some(CONTROL_SERVICE_MAX_MSG_SIZE))
            .set_socks_proxy_addr(self.config.socks_proxy_address.clone())
            .set_monitor_addr(self.monitor_address.clone())
            .establish(&self.config.listener_address)
            .map_err(|e| ControlServiceError::BindFailed(e))
    }
}
