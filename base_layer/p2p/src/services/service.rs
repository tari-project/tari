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

use super::executor::ServiceContext;
use crate::{services::ServiceError, tari_message::TariMessageType};
use crossbeam_channel as channel;
use std::{sync::Arc, time::Duration};

/// This trait should be implemented for services
pub trait Service: Send + Sync {
    /// A 'friendly' name used for logging purposes
    fn get_name(&self) -> String;
    /// Returns the message types this service requires. These will be
    /// registered in the comms routing.
    fn get_message_types(&self) -> Vec<TariMessageType>;
    /// The entry point of the service. This will be executed in a dedicated thread.
    /// The service should use `context.create_connector(message_type)` to create a `DomainConnector`
    /// for the registered message types returned from `Service::get_message_types`.
    /// This should contain a loop which reads control messages (`context.get_control_message`)
    /// and connector messages and processes them.
    fn execute(&mut self, context: ServiceContext) -> Result<(), ServiceError>;
}

/// Default duration that a API 'client' will wait for a response from the service before returning a timeout error
pub const DEFAULT_API_TIMEOUT_MS: u64 = 200;

/// Thin convenience wrapper for any service api
pub struct ServiceApiWrapper<T, Req, Res> {
    api: Arc<T>,
    receiver: channel::Receiver<Req>,
    sender: channel::Sender<Res>,
}

impl<T, Req, Res> ServiceApiWrapper<T, Req, Res> {
    /// Create a new service API
    pub fn new(receiver: channel::Receiver<Req>, sender: channel::Sender<Res>, api: Arc<T>) -> Self {
        Self { api, receiver, sender }
    }

    /// Send a reply to the calling API
    pub fn send_reply(&self, msg: Res) -> Result<(), channel::SendError<Res>> {
        self.sender.send(msg)
    }

    /// Attempt to receive a service API message
    pub fn recv_timeout(&self, timeout: Duration) -> Result<Option<Req>, channel::RecvTimeoutError> {
        match self.receiver.recv_timeout(timeout) {
            Ok(msg) => Ok(Some(msg)),
            Err(channel::RecvTimeoutError::Timeout) => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// Return the API
    pub fn get_api(&self) -> Arc<T> {
        self.api.clone()
    }
}
