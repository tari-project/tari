// Copyright 2020, The Taiji Project
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

use std::collections::HashMap;

use tokio::sync::mpsc;

use crate::{
    peer_manager::NodeId,
    protocol::{ProtocolError, ProtocolExtension, ProtocolExtensionContext, ProtocolExtensionError, ProtocolId},
    Substream,
};

/// Protocol notification sender
pub type ProtocolNotificationTx<TSubstream> = mpsc::Sender<ProtocolNotification<TSubstream>>;
/// Protocol notification receiver
pub type ProtocolNotificationRx<TSubstream> = mpsc::Receiver<ProtocolNotification<TSubstream>>;

/// Event emitted when a new inbound substream is requested by a remote node.
#[derive(Debug, Clone)]
pub enum ProtocolEvent<TSubstream> {
    NewInboundSubstream(NodeId, TSubstream),
}

/// Notification of a new protocol
#[derive(Debug, Clone)]
pub struct ProtocolNotification<TSubstream> {
    pub event: ProtocolEvent<TSubstream>,
    pub protocol: ProtocolId,
}

impl<TSubstream> ProtocolNotification<TSubstream> {
    pub fn new(protocol: ProtocolId, event: ProtocolEvent<TSubstream>) -> Self {
        Self { event, protocol }
    }
}

/// Keeps a map of supported protocols and the sender that should be notified.
pub struct Protocols<TSubstream> {
    protocols: HashMap<ProtocolId, ProtocolNotificationTx<TSubstream>>,
}

impl<TSubstream> Clone for Protocols<TSubstream> {
    fn clone(&self) -> Self {
        Self {
            protocols: self.protocols.clone(),
        }
    }
}

impl<TSubstream> Default for Protocols<TSubstream> {
    fn default() -> Self {
        Self {
            protocols: HashMap::default(),
        }
    }
}

impl<TSubstream> Protocols<TSubstream> {
    /// New empty protocol map
    pub fn new() -> Self {
        Default::default()
    }

    /// New empty protocol map
    pub fn empty() -> Self {
        Default::default()
    }

    /// Add a new protocol ID and notifier
    pub fn add<I: AsRef<[ProtocolId]>>(
        &mut self,
        protocols: I,
        notifier: &ProtocolNotificationTx<TSubstream>,
    ) -> &mut Self {
        self.protocols
            .extend(protocols.as_ref().iter().map(|p| (p.clone(), notifier.clone())));
        self
    }

    /// Extend this instance with all the protocols from another instance
    pub fn extend(&mut self, protocols: Self) -> &mut Self {
        self.protocols.extend(protocols.protocols);
        self
    }

    /// Returns all registered protocol IDs
    pub fn get_supported_protocols(&self) -> Vec<ProtocolId> {
        self.protocols.keys().cloned().collect()
    }

    /// Send a notification to the registered notifier for the protocol ID.
    pub async fn notify(
        &mut self,
        protocol: &ProtocolId,
        event: ProtocolEvent<TSubstream>,
    ) -> Result<(), ProtocolError> {
        match self.protocols.get_mut(protocol) {
            Some(sender) => {
                sender
                    .send(ProtocolNotification::new(protocol.clone(), event))
                    .await
                    .map_err(|_| ProtocolError::NotificationSenderDisconnected)?;
                Ok(())
            },
            None => Err(ProtocolError::ProtocolNotRegistered),
        }
    }

    /// Returns an iterator of currently registered [ProtocolId](self::ProtocolId)
    pub fn iter(&self) -> impl Iterator<Item = &ProtocolId> {
        self.protocols.keys()
    }
}

/// Protocols<Substream> itself is a `ProtocolExtension`. When installed the protocol names and notifiers are simply
/// moved (drained) over to the `ExtensionContext`.
impl ProtocolExtension for Protocols<Substream> {
    fn install(mut self: Box<Self>, context: &mut ProtocolExtensionContext) -> Result<(), ProtocolExtensionError> {
        for (protocol, notifier) in self.protocols.drain() {
            context.add_protocol(&[protocol], &notifier);
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use taiji_test_utils::unpack_enum;

    use super::*;

    #[test]
    fn add() {
        let (tx, _) = mpsc::channel(1);
        let protos = [
            ProtocolId::from_static(b"/taiji/test/1"),
            ProtocolId::from_static(b"/taiji/test/2"),
        ];
        let mut protocols = Protocols::<()>::new();
        protocols.add(&protos, &tx);

        assert!(protocols.get_supported_protocols().iter().all(|p| protos.contains(p)));
    }

    #[tokio::test]
    async fn notify() {
        let (tx, mut rx) = mpsc::channel(1);
        let protos = [ProtocolId::from_static(b"/taiji/test/1")];
        let mut protocols = Protocols::<()>::new();
        protocols.add(&protos, &tx);

        protocols
            .notify(&protos[0], ProtocolEvent::NewInboundSubstream(NodeId::new(), ()))
            .await
            .unwrap();

        let notification = rx.recv().await.unwrap();
        unpack_enum!(ProtocolEvent::NewInboundSubstream(peer_id, _s) = notification.event);
        assert_eq!(peer_id, NodeId::new());
    }

    #[tokio::test]
    async fn notify_fail_not_registered() {
        let mut protocols = Protocols::<()>::new();

        let err = protocols
            .notify(
                &ProtocolId::from_static(b"/taiji/test/0"),
                ProtocolEvent::NewInboundSubstream(NodeId::new(), ()),
            )
            .await
            .unwrap_err();

        unpack_enum!(ProtocolError::ProtocolNotRegistered = err);
    }
}
