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

use crate::outbound::{
    message::SendMessageResponse,
    message_params::FinalSendMessageParams,
    DhtOutboundRequest,
    OutboundMessageRequester,
};
use futures::{channel::mpsc, stream::Fuse, StreamExt};
use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};
use tokio::future::FutureExt as TokioFutureExt;

/// Creates a mock outbound request "handler" for testing purposes.
///
/// Each time a request is expected, handle_next should be called.
pub fn create_mock_outbound_service(size: usize) -> (OutboundMessageRequester, MockOutboundService) {
    let (tx, rx) = mpsc::channel(size);
    (OutboundMessageRequester::new(tx), MockOutboundService::new(rx.fuse()))
}

pub struct MockOutboundService {
    receiver: Fuse<mpsc::Receiver<DhtOutboundRequest>>,
    request_count: AtomicUsize,
}

impl MockOutboundService {
    pub fn new(receiver: Fuse<mpsc::Receiver<DhtOutboundRequest>>) -> Self {
        Self {
            receiver,
            request_count: AtomicUsize::new(0),
        }
    }

    /// Return the number of requests that have been made
    pub fn request_count(&self) -> usize {
        self.request_count.load(Ordering::SeqCst)
    }

    /// Receive and handle the next request. A panic will occur if the request times out, the receiver has closed, or a
    /// reply cannot be sent back.
    pub async fn handle_next(
        &mut self,
        timeout: Duration,
        response: SendMessageResponse,
    ) -> (FinalSendMessageParams, Vec<u8>)
    {
        let req = self.receiver.next().timeout(timeout).await.unwrap().unwrap();
        self.request_count.fetch_add(1, Ordering::AcqRel);
        match req {
            DhtOutboundRequest::SendMsg(params, body, reply_tx) => {
                reply_tx.send(response).expect("Reply channel cancelled");
                (*params, body)
            },
            msg => panic!("Unexpected {} message", msg),
        }
    }

    /// Handle `count` messages and return a vector of the results.
    pub async fn handle_many(
        mut self,
        count: usize,
        timeout: Duration,
        num_sent: usize,
    ) -> Vec<(FinalSendMessageParams, Vec<u8>)>
    {
        let mut results = Vec::with_capacity(count);
        for _ in 0..count {
            results.push(
                self.handle_next(timeout.clone(), SendMessageResponse::Ok(num_sent))
                    .await,
            );
        }
        results
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::domain_message::OutboundDomainMessage;
    use futures::{future::join, FutureExt};
    use tari_test_utils::runtime;

    #[test]
    fn handle_next() {
        runtime::test_async(|rt| {
            let (mut requester, mut service) = create_mock_outbound_service(1);

            let (_, result) = rt.block_on(join(
                service.handle_next(Duration::from_millis(100), SendMessageResponse::Ok(123)),
                requester.send_direct_neighbours(
                    Default::default(),
                    Default::default(),
                    OutboundDomainMessage::new(0, b"".to_vec()),
                ),
            ));

            assert_eq!(service.request_count(), 1);
            match result.unwrap() {
                SendMessageResponse::Ok(123) => {},
                _ => panic!("Unexpected SendMessageResponse"),
            }
        })
    }

    #[test]
    fn handle_many() {
        runtime::test_async(|rt| {
            let (mut requester, service) = create_mock_outbound_service(1);
            rt.spawn(service.handle_many(2, Duration::from_millis(100), 123).map(|_| ()));

            let result = rt.block_on(requester.send_direct_neighbours(
                Default::default(),
                Default::default(),
                OutboundDomainMessage::new(0, b"".to_vec()),
            ));
            match result.unwrap() {
                SendMessageResponse::Ok(123) => {},
                _ => panic!("Unexpected SendMessageResponse"),
            }

            let result = rt.block_on(requester.send_direct_neighbours(
                Default::default(),
                Default::default(),
                OutboundDomainMessage::new(0, b"".to_vec()),
            ));

            match result.unwrap() {
                SendMessageResponse::Ok(123) => {},
                _ => panic!("Unexpected SendMessageResponse"),
            }
        })
    }
}
