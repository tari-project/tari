// Copyright 2022 The Taiji Project
// SPDX-License-Identifier: BSD-3-Clause

//! Future types

use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use futures::ready;
use pin_project::pin_project;
use taiji_comms::pipeline::PipelineError;
use tower::Service;

/// Filtered response future
#[pin_project]
#[derive(Debug)]
pub struct ResponseFuture<S, Request>
where S: Service<Request>
{
    #[pin]
    /// Response future state
    state: State<Request, S::Future>,

    /// Predicate result
    check: bool,

    /// Inner service
    service: S,
}

#[pin_project(project = StateProj)]
#[derive(Debug)]
enum State<Request, U> {
    Check(Option<Request>),
    WaitResponse(#[pin] U),
}

impl<S, Request> ResponseFuture<S, Request>
where S: Service<Request, Error = PipelineError>
{
    pub(crate) fn new(request: Request, check: bool, service: S) -> Self {
        ResponseFuture {
            state: State::Check(Some(request)),
            check,
            service,
        }
    }
}

impl<S, Request> Future for ResponseFuture<S, Request>
where S: Service<Request, Response = (), Error = PipelineError>
{
    type Output = Result<S::Response, PipelineError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.project();

        loop {
            match this.state.as_mut().project() {
                StateProj::Check(request) => {
                    let request = request
                        .take()
                        .expect("we either give it back or leave State::Check once we take");

                    match this.check {
                        true => {
                            let response = this.service.call(request);
                            this.state.set(State::WaitResponse(response));
                        },
                        false => {
                            return Poll::Ready(Ok(()));
                        },
                    }
                },
                StateProj::WaitResponse(response) => {
                    return Poll::Ready(ready!(response.poll(cx)));
                },
            }
        }
    }
}
