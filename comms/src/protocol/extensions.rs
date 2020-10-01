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

use crate::{
    connectivity::ConnectivityRequester,
    protocol::{ProtocolId, ProtocolNotificationTx, Protocols},
    PeerManager,
    Substream,
};
use std::sync::Arc;
use tari_shutdown::ShutdownSignal;

pub type ProtocolExtensionError = anyhow::Error;

pub trait ProtocolExtension: Send + Sync {
    // TODO: The Box<Self> is easier to do for now at the cost of ProtocolExtension being less generic.
    fn install(self: Box<Self>, context: &mut ProtocolExtensionContext) -> Result<(), ProtocolExtensionError>;
}

#[derive(Default)]
pub struct ProtocolExtensions {
    inner: Vec<Box<dyn ProtocolExtension>>,
}

impl ProtocolExtensions {
    pub fn new() -> Self {
        Self { inner: Vec::new() }
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn add<T: ProtocolExtension + 'static>(&mut self, ext: T) -> &mut Self {
        self.inner.push(Box::new(ext));
        self
    }

    pub(crate) fn install_all(self, context: &mut ProtocolExtensionContext) -> Result<(), ProtocolExtensionError> {
        for ext in self.inner {
            ext.install(context)?;
        }
        Ok(())
    }
}

impl Extend<Box<dyn ProtocolExtension>> for ProtocolExtensions {
    fn extend<T: IntoIterator<Item = Box<dyn ProtocolExtension>>>(&mut self, iter: T) {
        self.inner.extend(iter)
    }
}

impl From<Protocols<Substream>> for ProtocolExtensions {
    fn from(protocols: Protocols<Substream>) -> Self {
        let mut p = Self::new();
        p.add(protocols);
        p
    }
}

impl IntoIterator for ProtocolExtensions {
    type IntoIter = <Vec<Self::Item> as IntoIterator>::IntoIter;
    type Item = Box<dyn ProtocolExtension>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

pub struct ProtocolExtensionContext {
    connectivity: ConnectivityRequester,
    peer_manager: Arc<PeerManager>,
    protocols: Option<Protocols<Substream>>,
    complete_signals: Vec<ShutdownSignal>,
    shutdown_signal: ShutdownSignal,
}

impl ProtocolExtensionContext {
    pub(crate) fn new(
        connectivity: ConnectivityRequester,
        peer_manager: Arc<PeerManager>,
        shutdown_signal: ShutdownSignal,
    ) -> Self
    {
        Self {
            connectivity,
            peer_manager,
            protocols: Some(Protocols::new()),
            complete_signals: Vec::new(),
            shutdown_signal,
        }
    }

    pub fn add_protocol<I: AsRef<[ProtocolId]>>(
        &mut self,
        protocols: I,
        notifier: ProtocolNotificationTx<Substream>,
    ) -> &mut Self
    {
        self.protocols
            .as_mut()
            .expect("CommsContext::protocols taken!")
            .add(protocols, notifier);
        self
    }

    pub fn register_complete_signal(&mut self, signal: ShutdownSignal) -> &mut Self {
        self.complete_signals.push(signal);
        self
    }

    pub fn connectivity(&self) -> ConnectivityRequester {
        self.connectivity.clone()
    }

    pub fn peer_manager(&self) -> Arc<PeerManager> {
        self.peer_manager.clone()
    }

    pub fn shutdown_signal(&self) -> ShutdownSignal {
        self.shutdown_signal.clone()
    }

    pub(crate) fn drain_complete_signals(&mut self) -> Vec<ShutdownSignal> {
        self.complete_signals.drain(..).collect()
    }

    pub(crate) fn take_protocols(&mut self) -> Option<Protocols<Substream>> {
        self.protocols.take()
    }
}
