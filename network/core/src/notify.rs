//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use std::collections::HashMap;

use libp2p::StreamProtocol;
use tari_swarm::substream::ProtocolNotification;
use tokio::sync::mpsc;

pub struct Notifiers<T> {
    senders: HashMap<StreamProtocol, mpsc::UnboundedSender<ProtocolNotification<T>>>,
}

impl<T> Notifiers<T> {
    pub fn new() -> Self {
        Self {
            senders: HashMap::new(),
        }
    }

    pub fn add(&mut self, protocol: StreamProtocol, sender: mpsc::UnboundedSender<ProtocolNotification<T>>) {
        self.senders.insert(protocol, sender);
    }

    pub fn notify(&mut self, notification: ProtocolNotification<T>) -> bool {
        if let Some(sender) = self.senders.get_mut(&notification.protocol) {
            if sender.send(notification).is_ok() {
                return true;
            }
        }
        false
    }
}
