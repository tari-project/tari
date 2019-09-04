//! Service framework
//!
//! This module contains the building blocks for async services.
//!
//! It consists of the following modules:
//!
//! - `builder`: contains the `MakeServicePair` trait which should be implemented by a service builder and the
//!   `StackBuilder` struct which is responsible for building the service and making service _handles_ available to all
//!   the other services. Handles are any object which is able to control a service in some way. Most commonly the
//!   handle will be a `transport::Requester<MyServiceRequest>`.
//!
//! - `handles`: struct for collecting named handles for services. The `StackBuilder` uses this to make all handles
//!   available to services.
//!
//! - `transport`: This allows messages to be reliably send/received to/from services. A `Requester`/`Responder` pair is
//!   created using the `transport::channel` function which takes an impl of `tower_service::Service` as it's first
//!   parameter. A `Requester` implements `tower_service::Service` and is used to send requests which return a Future
//!   which resolves to a response. The `Requester` uses a `oneshot` channel allow responses to be sent back. A
//!   `Responder` receives a `(request, oneshot::Sender)` tuple, calls the given tower service with that request and
//!   sends the result on the `oneshot::Sender`. The `Responder` handles many requests simultaneously.

// Used to eliminate the need for boxing futures in many cases.
// Tracking issue: https://github.com/rust-lang/rust/issues/63063
#![feature(type_alias_impl_trait)]

#[macro_use]
extern crate futures;

pub mod handles;
mod stack;
pub mod transport;

pub use self::stack::{ServiceInitializationError, ServiceInitializer, StackBuilder};
