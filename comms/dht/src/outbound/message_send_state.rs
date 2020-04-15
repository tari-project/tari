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

use futures::{stream::FuturesUnordered, Future, StreamExt};
use std::ops::Index;
use tari_comms::message::{MessageTag, MessagingReplyRx};

#[derive(Debug)]
pub struct MessageSendState {
    pub tag: MessageTag,
    reply_rx: MessagingReplyRx,
}
impl MessageSendState {
    pub fn new(tag: MessageTag, reply_rx: MessagingReplyRx) -> Self {
        Self { tag, reply_rx }
    }
}

#[derive(Debug)]
pub struct MessageSendStates {
    inner: Vec<MessageSendState>,
}

impl MessageSendStates {
    /// The number of `MessageSendState`s held in this container
    #[inline]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns true if there are no send states held in this container, otherwise false
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Wait for all send results to return. The return value contains the successful messages sent and the failed
    /// messages respectively
    pub async fn wait_all(self) -> (Vec<MessageTag>, Vec<MessageTag>) {
        let mut succeeded = Vec::new();
        let mut failed = Vec::new();
        let mut unordered = self.into_futures_unordered();
        while let Some((tag, result)) = unordered.next().await {
            match result {
                Ok(_) => {
                    succeeded.push(tag);
                },
                Err(_) => {
                    failed.push(tag);
                },
            }
        }

        (succeeded, failed)
    }

    /// Wait for a certain percentage of successful sends
    pub async fn wait_percentage_success(self, threshold_perc: f32) -> (Vec<MessageTag>, Vec<MessageTag>) {
        if self.is_empty() {
            return (Vec::new(), Vec::new());
        }
        let total = self.len();
        let mut count = 0;

        let mut unordered = self.into_futures_unordered();
        let mut succeeded = Vec::new();
        let mut failed = Vec::new();
        loop {
            match unordered.next().await {
                Some((tag, result)) => {
                    match result {
                        Ok(_) => {
                            count += 1;
                            succeeded.push(tag);
                        },
                        Err(_) => {
                            failed.push(tag);
                        },
                    }
                    if (count as f32) / (total as f32) >= threshold_perc {
                        break;
                    }
                },
                None => {
                    break;
                },
            }
        }

        (succeeded, failed)
    }

    /// Wait for the result of a single send. This should not be used when this container contains multiple send states.
    ///
    /// ## Panics
    ///
    /// This function expects there to be exactly one MessageSendState contained in this object and will
    /// panic in debug mode if this expectation is not met. It will panic for release builds if called
    /// when empty.
    pub async fn wait_single(mut self) -> bool {
        let state = self
            .inner
            .pop()
            .expect("wait_single called when MessageSendStates::len() is 0");

        debug_assert!(
            self.is_empty(),
            "MessageSendStates::wait_single called with multiple message send states"
        );

        state
            .reply_rx
            .await
            .expect("oneshot should never be canceled before sending")
            .is_ok()
    }

    pub fn into_futures_unordered(self) -> FuturesUnordered<impl Future<Output = (MessageTag, Result<(), ()>)>> {
        let unordered = FuturesUnordered::new();
        self.inner.into_iter().for_each(|state| {
            unordered.push(async move {
                match state.reply_rx.await {
                    Ok(result) => (state.tag, result),
                    // Somewhere the reply sender was dropped without first sending a reply
                    // This should never happen because we if the wrapped oneshot is dropped it sends an Err(())
                    Err(_) => unreachable!(),
                }
            });
        });

        unordered
    }
}

impl From<Vec<MessageSendState>> for MessageSendStates {
    fn from(inner: Vec<MessageSendState>) -> Self {
        Self { inner }
    }
}

impl Index<usize> for MessageSendStates {
    type Output = MessageSendState;

    fn index(&self, index: usize) -> &Self::Output {
        &self.inner[index]
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use bitflags::_core::iter::repeat_with;
    use futures::channel::oneshot;
    use tari_comms::message::MessagingReplyTx;

    fn create_send_state() -> (MessageSendState, MessagingReplyTx) {
        let (reply_tx, reply_rx) = oneshot::channel();
        let state = MessageSendState::new(MessageTag::new(), reply_rx);
        (state, reply_tx)
    }

    #[test]
    fn is_empty() {
        let states = MessageSendStates::from(vec![]);
        assert!(states.is_empty());
        let (state, _) = create_send_state();
        let states = MessageSendStates::from(vec![state]);
        assert_eq!(states.is_empty(), false);
    }

    #[tokio_macros::test_basic]
    async fn wait_single() {
        let (state, reply_tx) = create_send_state();
        let states = MessageSendStates::from(vec![state]);
        reply_tx.send(Ok(())).unwrap();
        assert_eq!(states.len(), 1);
        assert_eq!(states.wait_single().await, true);

        let (state, reply_tx) = create_send_state();
        let states = MessageSendStates::from(vec![state]);
        reply_tx.send(Err(())).unwrap();
        assert_eq!(states.len(), 1);
        assert_eq!(states.wait_single().await, false);
    }

    #[tokio_macros::test_basic]
    async fn wait_percentage_success() {
        let states = repeat_with(|| create_send_state()).take(10).collect::<Vec<_>>();
        let (states, mut reply_txs) = states.into_iter().unzip::<_, _, Vec<_>, Vec<_>>();
        let states = MessageSendStates::from(states);
        reply_txs.drain(..4).for_each(|tx| tx.send(Err(())).unwrap());
        reply_txs.drain(..).for_each(|tx| tx.send(Ok(())).unwrap());

        let (success, failed) = states.wait_percentage_success(0.3).await;
        assert_eq!(success.len(), 3);
        assert_eq!(failed.len(), 4);
    }

    #[tokio_macros::test_basic]
    async fn wait_all() {
        let states = repeat_with(|| create_send_state()).take(10).collect::<Vec<_>>();
        let (states, mut reply_txs) = states.into_iter().unzip::<_, _, Vec<_>, Vec<_>>();
        let states = MessageSendStates::from(states);
        reply_txs.drain(..4).for_each(|tx| tx.send(Err(())).unwrap());
        reply_txs.drain(..).for_each(|tx| tx.send(Ok(())).unwrap());

        let (success, failed) = states.wait_all().await;
        assert_eq!(success.len(), 6);
        assert_eq!(failed.len(), 4);
    }
}
