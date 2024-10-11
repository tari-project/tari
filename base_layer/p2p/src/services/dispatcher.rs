// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use log::*;
use tari_network::identity::PeerId;
use tokio::{sync::mpsc, task};

use crate::{
    connector::InboundMessaging,
    message::{tari_message, DomainMessage, DomainMessageHeader, MessageTag, TariMessageType, TariNodeMessageSpec},
    proto::message::TariMessage,
};

const LOG_TARGET: &str = "p2p::dispatcher";

#[derive(Debug, Clone)]
pub struct Dispatcher {
    inner: Arc<Mutex<Option<DispatcherInner>>>,
}

#[derive(Debug, Clone)]
struct DispatcherInner {
    forward: HashMap<TariMessageType, mpsc::UnboundedSender<DomainMessage<TariMessage>>>,
}

impl Dispatcher {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Some(DispatcherInner {
                forward: HashMap::new(),
            }))),
        }
    }

    pub fn register(&self, msg_type: TariMessageType, sender: mpsc::UnboundedSender<DomainMessage<TariMessage>>) {
        self.inner
            .lock()
            .expect("only occurs if program panics")
            .as_mut()
            .expect("always some")
            .forward
            .insert(msg_type, sender);
    }

    pub fn spawn(self, inbound: InboundMessaging<TariNodeMessageSpec>) -> task::JoinHandle<()> {
        let dispatcher = self.inner.lock().unwrap().take().expect("always some");
        dispatcher.spawn(inbound)
    }
}

impl DispatcherInner {
    fn forward<T: Into<TariMessage>>(&self, message_type: TariMessageType, peer_id: PeerId, msg: T) {
        match self.forward.get(&message_type) {
            Some(sender) => {
                let msg = DomainMessage {
                    source_peer_id: peer_id,
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

    fn spawn(self, mut inbound: InboundMessaging<TariNodeMessageSpec>) -> task::JoinHandle<()> {
        #[allow(clippy::enum_glob_use)]
        use tari_message::Message::*;
        tokio::spawn(async move {
            while let Some((peer_id, msg)) = inbound.next().await {
                let Some(msg) = msg.message else {
                    warn!(target: LOG_TARGET, "Peer {peer_id} sent empty message");
                    continue;
                };
                match msg {
                    PingPong(msg) => {
                        self.forward(TariMessageType::PingPong, peer_id, msg);
                    },
                    BaseNodeRequest(msg) => {
                        self.forward(TariMessageType::BaseNodeRequest, peer_id, msg);
                    },
                    BaseNodeResponse(msg) => {
                        self.forward(TariMessageType::BaseNodeResponse, peer_id, msg);
                    },
                    SenderPartialTransaction(msg) => {
                        self.forward(TariMessageType::SenderPartialTransaction, peer_id, msg);
                    },
                    ReceiverPartialTransactionReply(msg) => {
                        self.forward(TariMessageType::ReceiverPartialTransactionReply, peer_id, msg);
                    },
                    TransactionFinalized(msg) => {
                        self.forward(TariMessageType::TransactionFinalized, peer_id, msg);
                    },
                    TransactionCancelled(msg) => {
                        self.forward(TariMessageType::TransactionCancelled, peer_id, msg);
                    },
                    Chat(msg) => {
                        self.forward(TariMessageType::Chat, peer_id, msg);
                    },
                }
            }
        })
    }
}
