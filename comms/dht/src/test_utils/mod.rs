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

//! Provides a number of DHT mocks and other testing-related utilities.

macro_rules! unwrap_oms_send_msg {
    ($var:expr, reply_value=$reply_value:expr) => {
        match $var {
            crate::outbound::DhtOutboundRequest::SendMessage(boxed, body, reply_tx) => {
                let _result = reply_tx.send($reply_value);
                (*boxed, body)
            },
        }
    };
    ($var:expr) => {
        unwrap_oms_send_msg!(
            $var,
            reply_value = $crate::outbound::SendMessageResponse::Queued(vec![].into())
        )
    };
}

mod dht_actor_mock;
pub use dht_actor_mock::{create_dht_actor_mock, DhtMockState};

mod dht_discovery_mock;
pub use dht_discovery_mock::{create_dht_discovery_mock, DhtDiscoveryMockState};

#[cfg(test)]
mod makers;
#[cfg(test)]
pub use makers::*;

mod service;
pub use service::service_spy;

mod store_and_forward_mock;
pub use store_and_forward_mock::create_store_and_forward_mock;

pub fn assert_send_static_service<T, S>(_: &S)
where
    S: tower::Service<T> + Send + 'static,
    S::Future: Send,
{
}
