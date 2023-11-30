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

use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};

use futures::{
    channel::{oneshot, oneshot::Receiver},
    future::{FusedFuture, Shared},
    FutureExt,
};

pub fn channel<T: Clone>() -> OneshotTrigger<T> {
    OneshotTrigger::new()
}

#[derive(Clone, Debug)]
pub struct OneshotTrigger<T> {
    sender: Arc<Mutex<Option<oneshot::Sender<T>>>>,
    signal: OneshotSignal<T>,
}

impl<T: Clone> OneshotTrigger<T> {
    pub fn new() -> Self {
        let (tx, rx) = oneshot::channel();
        Self {
            sender: Arc::new(Mutex::new(Some(tx))),
            signal: rx.shared().into(),
        }
    }

    pub fn to_signal(&self) -> OneshotSignal<T> {
        self.signal.clone()
    }

    pub fn broadcast(&mut self, item: T) {
        let mut x = self.sender.lock().unwrap();
        if let Some(tx) = (*x).take() {
            let _result = tx.send(item);
        }
    }

    pub fn is_used(&self) -> bool {
        self.sender.lock().unwrap().is_none()
    }
}

impl<T: Clone> Default for OneshotTrigger<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct OneshotSignal<T> {
    inner: Shared<oneshot::Receiver<T>>,
}

impl<T: Clone> From<Shared<oneshot::Receiver<T>>> for OneshotSignal<T> {
    fn from(inner: Shared<Receiver<T>>) -> Self {
        Self { inner }
    }
}

impl<T: Clone> Future for OneshotSignal<T> {
    type Output = Option<T>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.is_terminated() {
            return Poll::Ready(None);
        }

        match Pin::new(&mut self.inner).poll(cx) {
            Poll::Ready(Ok(v)) => Poll::Ready(Some(v)),
            // Channel canceled
            Poll::Ready(Err(_)) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<T: Clone> FusedFuture for OneshotSignal<T> {
    fn is_terminated(&self) -> bool {
        self.inner.is_terminated()
    }
}
