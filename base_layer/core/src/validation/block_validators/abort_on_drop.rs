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
    ops::Deref,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::task;

/// A task JoinHandle that aborts the inner task associated with this handle if it is dropped.
/// Awaiting a cancelled task might complete as usual if the task was already completed before the time
/// it was cancelled, otherwise it will complete with a Err(JoinError::Cancelled).
pub struct AbortOnDropJoinHandle<T> {
    inner: task::JoinHandle<T>,
}

impl<T> From<task::JoinHandle<T>> for AbortOnDropJoinHandle<T> {
    fn from(handle: task::JoinHandle<T>) -> Self {
        Self { inner: handle }
    }
}

impl<T> Deref for AbortOnDropJoinHandle<T> {
    type Target = task::JoinHandle<T>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> Drop for AbortOnDropJoinHandle<T> {
    fn drop(&mut self) {
        self.inner.abort();
    }
}

impl<T> Future for AbortOnDropJoinHandle<T> {
    type Output = Result<T, task::JoinError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.inner).poll(cx)
    }
}
