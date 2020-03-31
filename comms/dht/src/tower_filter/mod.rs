#![doc(html_root_url = "https://docs.rs/tower-filter/0.3.0-alpha.2")]
// #![allow(elided_lifetimes_in_paths)]

//! Conditionally dispatch requests to the inner service based on the result of
//! a predicate.

pub mod future;
mod layer;
mod predicate;

pub use layer::FilterLayer;
pub use predicate::Predicate;

use future::ResponseFuture;
use futures::ready;
use std::task::{Context, Poll};
use tari_comms::pipeline::PipelineError;
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
    T: Service<Request, Error = PipelineError> + Clone,
    U: Predicate<Request>,
{
    type Error = PipelineError;
    type Future = ResponseFuture<U::Future, T, Request>;
    type Response = T::Response;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(ready!(self.inner.poll_ready(cx)).map_err(PipelineError::from_debug))
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
