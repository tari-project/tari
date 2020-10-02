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
use bytes::Bytes;
use futures::{
    channel::{mpsc, oneshot},
    stream::Fuse,
    StreamExt,
};
use log::*;
use std::{
    sync::{Arc, Condvar, Mutex, RwLock},
    time::Duration,
};
use tari_comms::{
    message::{MessageTag, MessagingReplyTx},
    protocol::messaging::SendFailReason,
};
use tokio::time::delay_for;

const LOG_TARGET: &str = "mock::outbound_requester";

/// Creates a mock outbound request "handler" for testing purposes.
///
/// Each time a request is expected, handle_next should be called.
pub fn create_outbound_service_mock(size: usize) -> (OutboundMessageRequester, OutboundServiceMock) {
    let (tx, rx) = mpsc::channel(size);
    (OutboundMessageRequester::new(tx), OutboundServiceMock::new(rx.fuse()))
}

#[derive(Clone, Default)]
pub struct OutboundServiceMockState {
    #[allow(clippy::type_complexity)]
    calls: Arc<Mutex<Vec<(FinalSendMessageParams, Bytes)>>>,
    next_response: Arc<RwLock<Option<SendMessageResponse>>>,
    call_count_cond_var: Arc<Condvar>,
    behaviour: Arc<Mutex<MockBehaviour>>,
}

impl OutboundServiceMockState {
    pub fn new() -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
            next_response: Arc::new(RwLock::new(None)),
            call_count_cond_var: Arc::new(Condvar::new()),
            behaviour: Arc::new(Mutex::new(MockBehaviour::default())),
        }
    }

    pub fn call_count(&self) -> usize {
        acquire_lock!(self.calls).len()
    }

    /// Wait for `num_calls` extra calls or timeout.
    ///
    /// An error will be returned if the timeout expires.
    pub fn wait_call_count(&self, expected_calls: usize, timeout: Duration) -> Result<usize, String> {
        let call_guard = acquire_lock!(self.calls);
        let (call_guard, is_timeout) =
            condvar_shim::wait_timeout_until(&self.call_count_cond_var, call_guard, timeout, |calls| {
                calls.len() >= expected_calls
            })
            .expect("CondVar must never be poisoned");

        if is_timeout {
            Err(format!(
                "wait_call_count timed out before before receiving the expected number of calls. (Expected = {}, Got \
                 = {})",
                expected_calls,
                call_guard.len()
            ))
        } else {
            Ok(call_guard.len())
        }
    }

    /// Wait for a call to be added or timeout.
    ///
    /// An error will be returned if the timeout expires.
    pub fn wait_pop_call(&self, timeout: Duration) -> Result<(FinalSendMessageParams, Bytes), String> {
        let call_guard = acquire_lock!(self.calls);
        let (mut call_guard, timeout) = self
            .call_count_cond_var
            .wait_timeout(call_guard, timeout)
            .expect("CondVar must never be poisoned");

        if timeout.timed_out() {
            Err("wait_pop_call timed out before before receiving a call.".to_string())
        } else {
            Ok(call_guard.pop().expect("calls.len() must be greater than 1"))
        }
    }

    pub fn take_next_response(&self) -> Option<SendMessageResponse> {
        self.next_response.write().unwrap().take()
    }

    pub fn add_call(&self, req: (FinalSendMessageParams, Bytes)) {
        acquire_lock!(self.calls).push(req);
        self.call_count_cond_var.notify_all();
    }

    pub fn take_calls(&self) -> Vec<(FinalSendMessageParams, Bytes)> {
        acquire_lock!(self.calls).drain(..).collect()
    }

    pub fn pop_call(&self) -> Option<(FinalSendMessageParams, Bytes)> {
        acquire_lock!(self.calls).pop()
    }

    pub fn set_behaviour(&self, behaviour: MockBehaviour) {
        let mut lock = acquire_lock!(self.behaviour);
        *lock = behaviour;
    }

    pub fn get_behaviour(&self) -> MockBehaviour {
        let lock = acquire_lock!(self.behaviour);
        (*lock).clone()
    }
}

pub struct OutboundServiceMock {
    receiver: Fuse<mpsc::Receiver<DhtOutboundRequest>>,
    mock_state: OutboundServiceMockState,
}

impl OutboundServiceMock {
    pub fn new(receiver: Fuse<mpsc::Receiver<DhtOutboundRequest>>) -> Self {
        Self {
            receiver,
            mock_state: OutboundServiceMockState::new(),
        }
    }

    pub fn get_state(&self) -> OutboundServiceMockState {
        self.mock_state.clone()
    }

    pub async fn run(mut self) {
        while let Some(req) = self.receiver.next().await {
            match req {
                DhtOutboundRequest::SendMessage(params, body, reply_tx) => {
                    let behaviour = self.mock_state.get_behaviour();
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
                                    let (response, mut inner_reply_tx) = self.add_call((*params).clone(), body);
                                    reply_tx.send(response).expect("Reply channel cancelled");
                                    inner_reply_tx.reply_success();
                                },
                                ResponseType::QueuedFail => {
                                    let (response, mut inner_reply_tx) = self.add_call((*params).clone(), body);
                                    reply_tx.send(response).expect("Reply channel cancelled");
                                    inner_reply_tx.reply_fail(SendFailReason::PeerDialFailed);
                                },
                                ResponseType::QueuedSuccessDelay(delay) => {
                                    let (response, mut inner_reply_tx) = self.add_call((*params).clone(), body);
                                    reply_tx.send(response).expect("Reply channel cancelled");
                                    delay_for(delay).await;
                                    inner_reply_tx.reply_success();
                                },
                                resp => {
                                    reply_tx
                                        .send(SendMessageResponse::Failed(SendFailure::General(format!(
                                            "Unexpected mock response {:?}",
                                            resp
                                        ))))
                                        .expect("Reply channel cancelled");
                                },
                            };
                        },
                        BroadcastStrategy::Closest(_) => {
                            if behaviour.broadcast == ResponseType::Queued {
                                let (response, mut inner_reply_tx) = self.add_call((*params).clone(), body);
                                reply_tx.send(response).expect("Reply channel cancelled");
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
                            let (response, mut inner_reply_tx) = self.add_call((*params).clone(), body);
                            reply_tx.send(response).expect("Reply channel cancelled");
                            inner_reply_tx.reply_success();
                        },
                    }
                },
            }
        }
    }

    fn add_call(&mut self, params: FinalSendMessageParams, body: Bytes) -> (SendMessageResponse, MessagingReplyTx) {
        self.mock_state.add_call((params, body));
        let (inner_reply_tx, inner_reply_rx) = oneshot::channel();
        let response = self
            .mock_state
            .take_next_response()
            .or_else(|| {
                Some(SendMessageResponse::Queued(
                    vec![MessageSendState::new(MessageTag::new(), inner_reply_rx)].into(),
                ))
            })
            .expect("never none");
        (response, inner_reply_tx.into())
    }
}

mod condvar_shim {
    use std::{
        sync::{Condvar, LockResult, MutexGuard, PoisonError},
        time::{Duration, Instant},
    };

    pub fn wait_timeout_until<'a, T, F>(
        condvar: &Condvar,
        mut guard: MutexGuard<'a, T>,
        dur: Duration,
        mut condition: F,
    ) -> LockResult<(MutexGuard<'a, T>, bool)>
    where
        F: FnMut(&mut T) -> bool,
    {
        let start = Instant::now();
        loop {
            if condition(&mut *guard) {
                return Ok((guard, false));
            }
            let timeout = match dur.checked_sub(start.elapsed()) {
                Some(timeout) => timeout,
                None => return Ok((guard, true)),
            };
            guard = condvar
                .wait_timeout(guard, timeout)
                .map(|(guard, timeout)| (guard, timeout.timed_out()))
                .map_err(|err| {
                    let (guard, timeout) = err.into_inner();
                    PoisonError::new((guard, timeout.timed_out()))
                })?
                .0;
        }
    }
}

/// Define the three response options the mock can respond with.
#[derive(Clone, Debug, PartialEq)]
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
