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

use super::{error::Error, Filter};
use futures_util::{future::poll_fn, pin_mut};
use std::future::Future;
use tokio::runtime::Handle;
use tokio_test::task;
use tower::Service;
use tower_test::{assert_request_eq, mock};

#[tokio_macros::test]
async fn passthrough_sync() {
    let (mut service, handle) = new_service(|_| async { Ok(()) });

    let handle = Handle::current().spawn(async move {
        // Receive the requests and respond
        pin_mut!(handle);
        for i in 0..10 {
            assert_request_eq!(handle, format!("ping-{}", i)).send_response(format!("pong-{}", i));
        }
    });

    let mut responses = vec![];

    for i in 0usize..10 {
        let request = format!("ping-{}", i);
        poll_fn(|cx| service.poll_ready(cx)).await.unwrap();
        let exchange = service.call(request);
        let exchange = async move {
            let response = exchange.await.unwrap();
            let expect = format!("pong-{}", i);
            assert_eq!(response.as_str(), expect.as_str());
        };

        responses.push(exchange);
    }

    futures_util::future::join_all(responses).await;
    handle.await.unwrap();
}

#[test]
fn rejected_sync() {
    task::spawn(async {
        let (mut service, _handle) = new_service(|_| async { Err(Error::rejected()) });
        service.call("hello".into()).await.unwrap_err();
    });
}

type Mock = mock::Mock<String, String>;
type MockHandle = mock::Handle<String, String>;

fn new_service<F, U>(f: F) -> (Filter<Mock, F>, MockHandle)
where
    F: Fn(&String) -> U,
    U: Future<Output = Result<(), Error>>,
{
    let (service, handle) = mock::pair();
    let service = Filter::new(service, f);
    (service, handle)
}
