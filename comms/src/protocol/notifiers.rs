// Copyright 2020, The Tari Project
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

use crate::{
    peer_manager::NodeId,
    protocol::{ProtocolError, ProtocolId},
};
use futures::{channel::mpsc, SinkExt};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum ProtocolEvent<TSubstream> {
    NewInboundSubstream(Box<NodeId>, TSubstream),
}

#[derive(Debug, Clone)]
pub struct ProtocolNotification<TSubstream> {
    pub event: ProtocolEvent<TSubstream>,
    pub protocol: ProtocolId,
}

impl<TSubstream> ProtocolNotification<TSubstream> {
    pub fn new(protocol: ProtocolId, event: ProtocolEvent<TSubstream>) -> Self {
        Self { protocol, event }
    }
}

#[derive(Clone)]
pub struct ProtocolNotifier<TSubstream> {
    notifiers: HashMap<ProtocolId, mpsc::Sender<ProtocolNotification<TSubstream>>>,
}

impl<TSubstream> Default for ProtocolNotifier<TSubstream> {
    fn default() -> Self {
        Self {
            notifiers: HashMap::default(),
        }
    }
}

impl<TSubstream> ProtocolNotifier<TSubstream> {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn add(&mut self, protocols: &[ProtocolId], notifier: mpsc::Sender<ProtocolNotification<TSubstream>>) {
        self.notifiers
            .extend(protocols.iter().map(|p| (p.clone(), notifier.clone())));
    }

    pub fn get_supported_protocols(&self) -> Vec<ProtocolId> {
        self.notifiers.keys().cloned().collect()
    }

    pub async fn notify(
        &mut self,
        protocol: &ProtocolId,
        event: ProtocolEvent<TSubstream>,
    ) -> Result<(), ProtocolError>
    {
        match self.notifiers.get_mut(protocol) {
            Some(sender) => {
                sender.send(ProtocolNotification::new(protocol.clone(), event)).await?;
                Ok(())
            },
            None => Err(ProtocolError::ProtocolNotRegistered),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use futures::StreamExt;
    use tari_test_utils::unpack_enum;

    #[test]
    fn add() {
        let mut notifiers = ProtocolNotifier::<()>::new();
        let (tx, _) = mpsc::channel(1);
        let protocols = [
            ProtocolId::from_static(b"/tari/test/1"),
            ProtocolId::from_static(b"/tari/test/2"),
        ];
        notifiers.add(&protocols.clone(), tx);

        assert!(notifiers
            .get_supported_protocols()
            .iter()
            .all(|p| protocols.contains(p)));
    }

    #[tokio_macros::test_basic]
    async fn notify() {
        let mut notifiers = ProtocolNotifier::<()>::new();
        let (tx, mut rx) = mpsc::channel(1);
        let protocols = &[ProtocolId::from_static(b"/tari/test/1")];
        notifiers.add(protocols, tx);

        notifiers
            .notify(
                &ProtocolId::from_static(b"/tari/test/1"),
                ProtocolEvent::NewInboundSubstream(Box::new(NodeId::new()), ()),
            )
            .await
            .unwrap();

        let notification = rx.next().await.unwrap();
        unpack_enum!(ProtocolEvent::NewInboundSubstream(peer_id, _s) = notification.event);
        assert_eq!(*peer_id, NodeId::new());
    }

    #[tokio_macros::test_basic]
    async fn notify_fail_not_registered() {
        let mut notifiers = ProtocolNotifier::<()>::new();

        let err = notifiers
            .notify(
                &ProtocolId::from_static(b"/tari/test/0"),
                ProtocolEvent::NewInboundSubstream(Box::new(NodeId::new()), ()),
            )
            .await
            .unwrap_err();

        unpack_enum!(ProtocolError::ProtocolNotRegistered = err);
    }
}
