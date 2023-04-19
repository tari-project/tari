// Copyright 2019, The Tari Project
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

use std::{
    mem,
    sync::Arc,
    time::{Duration, Instant},
};

use log::*;
use tari_comms::{
    message::{MessageTag, MessagingReplyTx},
    protocol::messaging::SendFailReason,
    BytesMut,
};
use tokio::{
    sync::{mpsc, oneshot, watch, Mutex, RwLock},
    time,
    time::sleep,
};

use crate::{
    broadcast_strategy::BroadcastStrategy,
    outbound::{
        message::{SendFailure, SendMessageResponse},
        message_params::FinalSendMessageParams,
        message_send_state::MessageSendState,
        DhtOutboundRequest,
        OutboundMessageRequester,
    },
};

const LOG_TARGET: &str = "mock::outbound_requester";

/// Creates a mock outbound request "handler" for testing purposes.
///
/// Each time a request is expected, handle_next should be called.
pub fn create_outbound_service_mock(size: usize) -> (OutboundMessageRequester, OutboundServiceMock) {
    let (tx, rx) = mpsc::channel(size);
    (OutboundMessageRequester::new(tx), OutboundServiceMock::new(rx))
}

#[derive(Clone)]
pub struct OutboundServiceMockState {
    #[allow(clippy::type_complexity)]
    calls: Arc<Mutex<Vec<(FinalSendMessageParams, BytesMut)>>>,
    next_response: Arc<RwLock<Option<SendMessageResponse>>>,
    notif_sender: Arc<watch::Sender<()>>,
    notif_reciever: watch::Receiver<()>,
    behaviour: Arc<Mutex<MockBehaviour>>,
}

impl OutboundServiceMockState {
    pub fn new() -> Self {
        let (sender, receiver) = watch::channel(());
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
            next_response: Arc::new(RwLock::new(None)),
            notif_sender: Arc::new(sender),
            notif_reciever: receiver,
            behaviour: Arc::new(Mutex::new(MockBehaviour::default())),
        }
    }

    pub async fn call_count(&self) -> usize {
        self.calls.lock().await.len()
    }

    /// Wait for `num_calls` extra calls or timeout.
    ///
    /// An error will be returned if the timeout expires.
    pub async fn wait_call_count(&self, expected_calls: usize, timeout: Duration) -> Result<usize, String> {
        let mut rx = self.notif_reciever.clone();
        let start = Instant::now();
        let result = loop {
            let since = start.elapsed();
            if timeout.checked_sub(since).is_none() {
                break None;
            }
            if time::timeout(timeout - since, rx.changed()).await.is_err() {
                break None;
            }
            let calls = self.calls.lock().await;
            if calls.len() >= expected_calls {
                break Some(calls.len());
            }
        };

        match result {
            Some(n) => Ok(n),
            None => {
                let num_calls = self.call_count().await;
                Err(format!(
                    "wait_call_count timed out before before receiving the expected number of calls. (Expected = {}, \
                     Got = {})",
                    expected_calls, num_calls
                ))
            },
        }
    }

    pub async fn take_next_response(&self) -> Option<SendMessageResponse> {
        self.next_response.write().await.take()
    }

    async fn add_call(&self, req: (FinalSendMessageParams, BytesMut)) {
        self.calls.lock().await.push(req);
        let _r = self.notif_sender.send(());
    }

    pub async fn take_calls(&self) -> Vec<(FinalSendMessageParams, BytesMut)> {
        self.calls
            .lock()
            .await
            .drain(..)
            .map(|(p, mut b)| {
                if p.encryption.is_encrypt() {
                    // Remove prefix data
                    (p, b.split_off(mem::size_of::<u32>()))
                } else {
                    (p, b)
                }
            })
            .collect()
    }

    pub async fn pop_call(&self) -> Option<(FinalSendMessageParams, BytesMut)> {
        self.calls.lock().await.pop().map(|(p, mut b)| {
            if p.encryption.is_encrypt() {
                // Remove prefix data
                (p, b.split_off(mem::size_of::<u32>()))
            } else {
                (p, b)
            }
        })
    }

    pub async fn set_behaviour(&self, behaviour: MockBehaviour) {
        let mut lock = self.behaviour.lock().await;
        *lock = behaviour;
    }

    pub async fn get_behaviour(&self) -> MockBehaviour {
        let lock = self.behaviour.lock().await;
        (*lock).clone()
    }
}

impl Default for OutboundServiceMockState {
    fn default() -> Self {
        Self::new()
    }
}

pub struct OutboundServiceMock {
    receiver: mpsc::Receiver<DhtOutboundRequest>,
    mock_state: OutboundServiceMockState,
}

impl OutboundServiceMock {
    pub fn new(receiver: mpsc::Receiver<DhtOutboundRequest>) -> Self {
        Self {
            receiver,
            mock_state: OutboundServiceMockState::new(),
        }
    }

    pub fn get_state(&self) -> OutboundServiceMockState {
        self.mock_state.clone()
    }

    pub async fn run(mut self) {
        while let Some(req) = self.receiver.recv().await {
            match req {
                DhtOutboundRequest::SendMessage(params, body, reply_tx) => {
                    let behaviour = self.mock_state.get_behaviour().await;
                    trace!(
                        target: LOG_TARGET,
                        "Send message request received with length of {} bytes (behaviour = {:?})",
                        body.len(),
                        behaviour
                    );
                    match (*params).clone().broadcast_strategy {
                        BroadcastStrategy::DirectPublicKey(_) => {
                            match behaviour.direct {
                                ResponseType::Queued => {
                                    let (response, mut inner_reply_tx) = self.add_call((*params).clone(), body).await;
                                    let _ignore = reply_tx.send(response);
                                    inner_reply_tx.reply_success();
                                },
                                ResponseType::QueuedFail => {
                                    let (response, mut inner_reply_tx) = self.add_call((*params).clone(), body).await;
                                    let _ignore = reply_tx.send(response);
                                    inner_reply_tx.reply_fail(SendFailReason::PeerDialFailed);
                                },
                                ResponseType::QueuedSuccessDelay(delay) => {
                                    let (response, mut inner_reply_tx) = self.add_call((*params).clone(), body).await;
                                    let _ignore = reply_tx.send(response);
                                    sleep(delay).await;
                                    inner_reply_tx.reply_success();
                                },
                                resp => {
                                    let _ignore = reply_tx.send(SendMessageResponse::Failed(SendFailure::General(
                                        format!("Unexpected mock response {:?}", resp),
                                    )));
                                },
                            };
                        },
                        BroadcastStrategy::ClosestNodes(_) => {
                            if behaviour.broadcast == ResponseType::Queued {
                                let (response, mut inner_reply_tx) = self.add_call((*params).clone(), body).await;
                                let _ignore = reply_tx.send(response);
                                inner_reply_tx.reply_success();
                            } else {
                                reply_tx
                                    .send(SendMessageResponse::Failed(SendFailure::General(
                                        "Mock broadcast behaviour was not set to Queued".to_string(),
                                    )))
                                    .expect("Reply channel cancelled");
                            }
                        },
                        _ => {
                            let (response, mut inner_reply_tx) = self.add_call((*params).clone(), body).await;
                            let _ignore = reply_tx.send(response);
                            inner_reply_tx.reply_success();
                        },
                    }
                },
            }
        }
    }

    async fn add_call(
        &mut self,
        params: FinalSendMessageParams,
        body: BytesMut,
    ) -> (SendMessageResponse, MessagingReplyTx) {
        self.mock_state.add_call((params, body)).await;
        let (inner_reply_tx, inner_reply_rx) = oneshot::channel();
        let response = self
            .mock_state
            .take_next_response()
            .await
            .or_else(|| {
                Some(SendMessageResponse::Queued(
                    vec![MessageSendState::new(MessageTag::new(), inner_reply_rx)].into(),
                ))
            })
            .expect("never none");
        (response, inner_reply_tx.into())
    }
}

/// Define the three response options the mock can respond with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResponseType {
    Queued,
    QueuedFail,
    QueuedSuccessDelay(Duration),
    Failed,
    PendingDiscovery,
}

/// Define how the mock service will response to various broadcast strategies
#[derive(Debug, Clone)]
pub struct MockBehaviour {
    pub direct: ResponseType,
    pub broadcast: ResponseType,
}

impl Default for MockBehaviour {
    fn default() -> Self {
        Self {
            direct: ResponseType::Queued,
            broadcast: ResponseType::Queued,
        }
    }
}
