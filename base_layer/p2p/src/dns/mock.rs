//  Copyright 2021, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{pin::Pin, sync::Arc};

use futures::{future, stream, Future};
use hickory_client::{
    op::{Message, Query},
    proto::{
        error::ProtoError,
        xfer::{DnsHandle, DnsRequest, DnsResponse},
    },
    rr::Record,
};

#[derive(Clone)]
pub struct MockClientHandle<O: OnSend> {
    messages: Arc<Vec<Result<DnsResponse, ProtoError>>>,
    _on_send: O,
}

impl MockClientHandle<DefaultOnSend> {
    /// constructs a new MockClient which returns each Message one after the other
    pub fn mock(messages: Vec<Result<DnsResponse, ProtoError>>) -> Self {
        println!("MockClientHandle::mock message count: {}", messages.len());

        MockClientHandle {
            messages: Arc::new(messages),
            _on_send: DefaultOnSend,
        }
    }
}

impl<O: OnSend + Unpin> DnsHandle for MockClientHandle<O> {
    type Response = stream::Once<future::Ready<Result<DnsResponse, ProtoError>>>;

    fn send<R: Into<DnsRequest>>(&self, _: R) -> Self::Response {
        let responses = (*self.messages)
            .clone()
            .into_iter()
            .try_fold(Message::new(), |mut msg, resp| {
                resp.map(move |resp| {
                    msg.add_answers(resp.answers().iter().cloned());
                    msg
                })
            })
            .and_then(DnsResponse::from_message);

        // let stream = stream::unfold(messages, |mut msgs| async move {
        //     let msg = msgs.pop()?;
        //     Some((msg, msgs))
        // });

        stream::once(future::ready(responses))
    }
}

pub fn message(query: Query, answers: Vec<Record>, name_servers: Vec<Record>, additionals: Vec<Record>) -> Message {
    let mut message = Message::new();
    message.add_query(query);
    message.insert_answers(answers);
    message.insert_name_servers(name_servers);
    message.insert_additionals(additionals);
    message
}

pub trait OnSend: Clone + Send + Sync + 'static {
    #[allow(dead_code)]
    fn on_send<E>(
        &mut self,
        response: Result<DnsResponse, E>,
    ) -> Pin<Box<dyn Future<Output = Result<DnsResponse, E>> + Send>>
    where
        E: From<ProtoError> + Send + 'static,
    {
        Box::pin(future::ready(response))
    }
}

#[derive(Clone)]
pub struct DefaultOnSend;

impl OnSend for DefaultOnSend {}
