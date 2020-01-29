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

// Copyright (c) The Libra Core Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module provides a compatibility shim between traits in the `futures` and `tokio` crate.
use std::{
    io,
    pin::Pin,
    task::{self, Poll},
};

/// `IoCompat` provides a compatibility shim between the `AsyncRead`/`AsyncWrite` traits provided by
/// the `futures` library and those provided by the `tokio` library since they are different and
/// incompatible with one another.
#[derive(Copy, Clone, Debug)]
pub struct IoCompat<T> {
    inner: T,
}

impl<T> IoCompat<T> {
    pub fn new(inner: T) -> Self {
        IoCompat { inner }
    }
}

impl<T> tokio::io::AsyncRead for IoCompat<T>
where T: futures::io::AsyncRead + Unpin
{
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut task::Context, buf: &mut [u8]) -> Poll<io::Result<usize>> {
        futures::io::AsyncRead::poll_read(Pin::new(&mut self.inner), cx, buf)
    }
}

impl<T> futures::io::AsyncRead for IoCompat<T>
where T: tokio::io::AsyncRead + Unpin
{
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut task::Context, buf: &mut [u8]) -> Poll<io::Result<usize>> {
        tokio::io::AsyncRead::poll_read(Pin::new(&mut self.inner), cx, buf)
    }
}

impl<T> tokio::io::AsyncWrite for IoCompat<T>
where T: futures::io::AsyncWrite + Unpin
{
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut task::Context, buf: &[u8]) -> Poll<io::Result<usize>> {
        futures::io::AsyncWrite::poll_write(Pin::new(&mut self.inner), cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut task::Context) -> Poll<io::Result<()>> {
        futures::io::AsyncWrite::poll_flush(Pin::new(&mut self.inner), cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut task::Context) -> Poll<io::Result<()>> {
        futures::io::AsyncWrite::poll_close(Pin::new(&mut self.inner), cx)
    }
}

impl<T> futures::io::AsyncWrite for IoCompat<T>
where T: tokio::io::AsyncWrite + Unpin
{
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut task::Context, buf: &[u8]) -> Poll<io::Result<usize>> {
        tokio::io::AsyncWrite::poll_write(Pin::new(&mut self.inner), cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut task::Context) -> Poll<io::Result<()>> {
        tokio::io::AsyncWrite::poll_flush(Pin::new(&mut self.inner), cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut task::Context) -> Poll<io::Result<()>> {
        tokio::io::AsyncWrite::poll_shutdown(Pin::new(&mut self.inner), cx)
    }
}
