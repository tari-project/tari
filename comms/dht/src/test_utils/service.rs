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

use futures::future;
use std::{
    future::Future,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
        Mutex,
    },
};
use tower::Service;

pub fn service_spy<TReq>() -> ServiceSpy<TReq>
where TReq: 'static {
    ServiceSpy::new()
}

#[derive(Clone)]
pub struct ServiceSpy<TReq> {
    requests: Arc<Mutex<Vec<TReq>>>,
    call_count: Arc<AtomicUsize>,
}

impl<TReq> ServiceSpy<TReq>
where TReq: 'static
{
    pub fn new() -> Self {
        let requests = Arc::new(Mutex::new(Vec::new()));
        Self {
            requests,
            call_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    #[allow(dead_code)]
    pub fn reset(&self) {
        self.call_count.store(0, Ordering::SeqCst);
        self.requests.lock().unwrap().clear();
    }

    pub fn to_service<TErr>(
        &self,
    ) -> impl Service<TReq, Response = (), Error = TErr, Future = impl Future<Output = Result<(), TErr>>> + Clone {
        let req_inner = Arc::clone(&self.requests);
        let call_count = Arc::clone(&self.call_count);
        tower::service_fn(move |req: TReq| {
            req_inner.lock().unwrap().push(req);
            call_count.fetch_add(1, Ordering::SeqCst);
            future::ready(Result::<_, TErr>::Ok(()))
        })
    }

    #[allow(dead_code)]
    pub fn take_requests(&self) -> Vec<TReq> {
        self.requests.lock().unwrap().drain(..).collect()
    }

    pub fn pop_request(&self) -> Option<TReq> {
        self.requests.lock().unwrap().pop()
    }

    pub fn is_called(&self) -> bool {
        self.call_count() > 0
    }

    #[allow(dead_code)]
    pub fn call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }
}
