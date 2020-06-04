//  Copyright 2020, The Tari Project
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

//! Contains `Either` and related types and functions.
//!
//! See `Either` documentation for more details.
//!
//! Tari changes:
//! 1. Instead of defining a new private boxed error that all users of Either must "comply" with, this implementation
//!    requires that `A` and `B` both have the same error type. This suits our implementation as all errors are
//!    `RpcStatus` and removes the need for (often inelegant) error conversions.

use futures::ready;
use pin_project::pin_project;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tower::Service;

/// Combine two different service types into a single type.
///
/// Both services must be of the same request, response, and error types.
/// `Either` is useful for handling conditional branching in service middleware
/// to different inner service types.
#[pin_project(project = EitherProj)]
#[derive(Clone, Debug)]
pub enum Either<A, B> {
    /// One type of backing `Service`.
    A(#[pin] A),
    /// The other type of backing `Service`.
    B(#[pin] B),
}

impl<A, B, Request> Service<Request> for Either<A, B>
where
    A: Service<Request>,
    B: Service<Request, Response = A::Response, Error = A::Error>,
{
    type Error = A::Error;
    type Future = Either<A::Future, B::Future>;
    type Response = A::Response;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        use self::Either::*;

        match self {
            A(service) => Poll::Ready(Ok(ready!(service.poll_ready(cx))?)),
            B(service) => Poll::Ready(Ok(ready!(service.poll_ready(cx))?)),
        }
    }

    fn call(&mut self, request: Request) -> Self::Future {
        use self::Either::*;

        match self {
            A(service) => A(service.call(request)),
            B(service) => B(service.call(request)),
        }
    }
}

impl<A, B, T, AE> Future for Either<A, B>
where
    A: Future<Output = Result<T, AE>>,
    B: Future<Output = Result<T, AE>>,
{
    type Output = Result<T, AE>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.project() {
            EitherProj::A(fut) => Poll::Ready(Ok(ready!(fut.poll(cx))?)),
            EitherProj::B(fut) => Poll::Ready(Ok(ready!(fut.poll(cx))?)),
        }
    }
}
