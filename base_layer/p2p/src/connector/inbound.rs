// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use tari_network::{identity::PeerId, MessageSpec};
use tokio::sync::mpsc;

#[derive(Debug)]
pub struct InboundMessaging<TMsg: MessageSpec> {
    rx: mpsc::UnboundedReceiver<(PeerId, TMsg::Message)>,
}

impl<TMsg: MessageSpec> InboundMessaging<TMsg> {
    pub fn new(rx: mpsc::UnboundedReceiver<(PeerId, TMsg::Message)>) -> Self {
        Self { rx }
    }

    pub async fn next(&mut self) -> Option<(PeerId, TMsg::Message)> {
        self.rx.recv().await
    }
}
