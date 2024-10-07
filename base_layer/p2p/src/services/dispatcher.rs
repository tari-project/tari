// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::collections::HashMap;

use log::*;
use tari_network::{identity::PeerId, MessageSpec};
use tokio::{sync::mpsc, task};

use crate::{
    connector::InboundMessaging,
    message::{DomainMessage, DomainMessageHeader, MessageTag, TariNodeMessage, TariNodeMessageSpec},
    tari_message::TariMessageType,
};

const LOG_TARGET: &str = "p2p::dispatcher";

pub struct Dispatcher {
    forward: HashMap<TariMessageType, mpsc::UnboundedSender<DomainMessage<TariNodeMessage>>>,
}

impl Dispatcher {
    pub fn new() -> Self {
        Self {
            forward: HashMap::new(),
        }
    }

    pub fn register(&mut self, ty: TariMessageType, tx: mpsc::UnboundedSender<DomainMessage<TariNodeMessage>>) {
        self.forward.insert(ty, tx);
    }

    fn forward<T: Into<TariNodeMessage>>(&self, message_type: TariMessageType, peer_id: PeerId, msg: T) {
        match self.forward.get(&TariMessageType::PingPong) {
            Some(sender) => {
                let msg = DomainMessage {
                    from_peer_id: peer_id,
                    header: DomainMessageHeader {
                        message_tag: MessageTag::new(),
                    },
                    payload: msg.into(),
                };
                if sender.send(msg).is_err() {
                    warn!(target: LOG_TARGET, "Message channel for message type {:?} is closed", message_type);
                }
            },
            None => {
                warn!(target: LOG_TARGET, "No message channel registered for message type {:?}", message_type);
            },
        }
    }

    pub fn spawn(self, mut inbound: InboundMessaging<TariNodeMessageSpec>) -> task::JoinHandle<()> {
        tokio::spawn(async move {
            while let Some((peer_id, msg)) = inbound.next().await {
                match msg {
                    TariNodeMessage::PingPong(msg) => {
                        self.forward(TariMessageType::PingPong, peer_id, msg);
                    },
                    TariNodeMessage::NewTransaction(msg) => {
                        self.forward(TariMessageType::NewTransaction, peer_id, msg);
                    },
                    TariNodeMessage::NewBlock(msg) => {
                        self.forward(TariMessageType::NewBlock, peer_id, msg);
                    },
                    TariNodeMessage::BaseNodeRequest(msg) => {
                        self.forward(TariMessageType::BaseNodeRequest, peer_id, msg);
                    },
                    TariNodeMessage::BaseNodeResponse(msg) => {
                        self.forward(TariMessageType::BaseNodeResponse, peer_id, msg);
                    },
                    TariNodeMessage::SenderPartialTransaction(msg) => {
                        self.forward(TariMessageType::SenderPartialTransaction, peer_id, msg);
                    },
                    TariNodeMessage::ReceiverPartialTransactionReply(msg) => {
                        self.forward(TariMessageType::ReceiverPartialTransactionReply, peer_id, msg);
                    },
                    TariNodeMessage::TransactionFinalized(msg) => {
                        self.forward(TariMessageType::TransactionFinalized, peer_id, msg);
                    },
                    TariNodeMessage::TransactionCancelled(msg) => {
                        self.forward(TariMessageType::TransactionCancelled, peer_id, msg);
                    },
                    TariNodeMessage::Chat(msg) => {
                        self.forward(TariMessageType::Chat, peer_id, msg);
                    },
                }
            }
        })
    }
}
