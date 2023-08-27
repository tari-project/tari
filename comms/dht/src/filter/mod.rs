// Copyright 2022 The Taiji Project
// SPDX-License-Identifier: BSD-3-Clause

// #![allow(elided_lifetimes_in_paths)]

//! Conditionally dispatch requests to the inner service based on the result of
//! a predicate.

pub mod future;
mod layer;
mod predicate;

use std::task::{Context, Poll};

use future::ResponseFuture;
use futures::ready;
pub use layer::FilterLayer;
pub use predicate::Predicate;
use taiji_comms::pipeline::PipelineError;
use tower::Service;

/// Conditionally dispatch requests to the inner service based on a predicate.
#[derive(Clone, Debug)]
pub struct Filter<T, U> {
    inner: T,
    predicate: U,
}

impl<T, U> Filter<T, U> {
    #[allow(missing_docs)]
    pub fn new(inner: T, predicate: U) -> Self {
        Filter { inner, predicate }
    }
}

impl<T, U, Request> Service<Request> for Filter<T, U>
where
    T: Service<Request, Response = (), Error = PipelineError> + Clone,
    U: Predicate<Request>,
{
    type Error = PipelineError;
    type Future = ResponseFuture<T, Request>;
    type Response = T::Response;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(ready!(self.inner.poll_ready(cx)))
    }

    fn call(&mut self, request: Request) -> Self::Future {
        use std::mem;

        let inner = self.inner.clone();
        let inner = mem::replace(&mut self.inner, inner);

        // Check the request
        let check = self.predicate.check(&request);

        ResponseFuture::new(request, check, inner)
    }
}
