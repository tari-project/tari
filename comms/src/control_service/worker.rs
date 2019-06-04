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

use log::*;

use super::{
    error::ControlServiceError,
    service::ControlServiceConfig,
    types::{ControlMessage, ControlServiceDispatcher, ControlServiceMessageContext, Result},
};

use crate::{
    connection::{
        connection::EstablishedConnection,
        monitor::{ConnectionMonitor, SocketEvent, SocketEventType},
        Connection,
        Context,
        Direction,
        InprocAddress,
    },
    connection_manager::ConnectionManager,
    dispatcher::{DispatchResolver, DispatchableKey},
    message::{FrameSet, Message, MessageEnvelope},
    peer_manager::PeerManager,
    types::{CommsPublicKey, MessageEnvelopeHeader},
};
use std::{
    convert::TryInto,
    sync::{
        mpsc::{sync_channel, Receiver, SyncSender},
        Arc,
    },
    thread,
    time::Duration,
};
use tari_storage::lmdb::LMDBStore;

const LOG_TARGET: &'static str = "comms::control_service::worker";
const CONTROL_SERVICE_MAX_MSG_SIZE: u64 = 1024; // 1kb

/// The [ControlService] worker is responsible for handling incoming messages
/// to the control port and dispatching them using the message dispatcher.
pub struct ControlServiceWorker<MType, R>
where MType: DispatchableKey
{
    context: Context,
    config: ControlServiceConfig,
    monitor_address: InprocAddress,
    receiver: Receiver<ControlMessage>,
    is_running: bool,
    dispatcher: ControlServiceDispatcher<MType, R>,
    connection_manager: Arc<ConnectionManager>,
    peer_manager: Arc<PeerManager<CommsPublicKey, LMDBStore>>,
}

impl<MType, R> ControlServiceWorker<MType, R>
where
    MType: DispatchableKey,
    R: DispatchResolver<MType, ControlServiceMessageContext>,
{
    /// Start the worker
    ///
    /// # Arguments
    /// - `context` - Connection context
    /// - `config` - ControlServiceConfig
    /// - `dispatcher` - the `Dispatcher` to use when message are received
    /// - `connection_manager` - the `ConnectionManager`
    /// - `peer_manager` - the `PeerManager`
    pub fn start(
        context: Context,
        config: ControlServiceConfig,
        dispatcher: ControlServiceDispatcher<MType, R>,
        connection_manager: Arc<ConnectionManager>,
        peer_manager: Arc<PeerManager<CommsPublicKey, LMDBStore>>,
    ) -> (thread::JoinHandle<Result<()>>, SyncSender<ControlMessage>)
    {
        let (sender, receiver) = sync_channel(5);

        let mut worker = Self {
            context,
            config,
            monitor_address: InprocAddress::random(),
            receiver,
            is_running: false,
            dispatcher,
            connection_manager,
            peer_manager,
        };

        let handle = thread::spawn(move || {
            loop {
                match worker.main_loop() {
                    Ok(_) => {
                        info!(target: LOG_TARGET, "Control service exiting loop.");
                        break;
                    },

                    Err(err) => {
                        error!(target: LOG_TARGET, "Worker exited with an error: {:?}", err);
                        info!(target: LOG_TARGET, "Restarting control service.");
                    },
                }
            }

            Ok(())
        });

        (handle, sender)
    }

    fn main_loop(&mut self) -> Result<()> {
        self.is_running = true;
        let monitor = ConnectionMonitor::connect(&self.context, &self.monitor_address)?;
        let listener = self.establish_listener()?;

        loop {
            // Read incoming messages
            if let Some(frames) = connection_try!(listener.receive(100)) {
                match self.process_message(frames) {
                    Ok(_) => info!(target: LOG_TARGET, "Message processed"),
                    Err(err) => error!(target: LOG_TARGET, "Error when processing message: {:?}", err),
                }
            }

            // Read socket events
            if let Some(event) = connection_try!(monitor.read(5)) {
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

        let envelope_header: MessageEnvelopeHeader = envelope.to_header()?;
        let message: Message = envelope.message_body()?;

        // TODO: Add outbound message service, peer etc.
        let context = ControlServiceMessageContext {
            envelope_header,
            message,
            connection_manager: self.connection_manager.clone(),
            peer_manager: self.peer_manager.clone(),
        };

        // TODO: Decryption of message
        debug!(target: LOG_TARGET, "Dispatching message");
        self.dispatcher.dispatch(context).map_err(|e| e.into())
    }

    fn process_socket_event(&mut self, event: SocketEvent) -> Result<()> {
        use SocketEventType::*;
        debug!(target: LOG_TARGET, "{:?}", event);

        match event.event_type {
            Listening => {
                info!(target: LOG_TARGET, "Started listening on '{}'", event.address);
            },
            Accepted => {
                info!(target: LOG_TARGET, "Accepted connection from '{}'", event.address);
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
        Connection::new(&self.context, Direction::Inbound)
            .set_receive_hwm(10)
            .set_max_message_size(Some(CONTROL_SERVICE_MAX_MSG_SIZE))
            .set_socks_proxy_addr(self.config.socks_proxy_address.clone())
            .set_monitor_addr(self.monitor_address.clone())
            .establish(&self.config.listener_address)
            .map_err(|e| ControlServiceError::BindFailed(e))
    }
}
