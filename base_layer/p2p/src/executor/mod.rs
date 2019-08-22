// Copyright 2019 The Tari Project
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

//! P2P Service execution
//!
//! This module contains the building blocks for async p2p services.
//!
//! It consists of the following modules:
//! - `builder`: contains the `MakeServicePair` trait which should be implemented by a service builder and the
//!   `StackBuilder` struct which is responsible for building the service and making service _handles_ available to all
//!   the other services. Handles are any object which is able to control a service in some way. Most commonly the
//!   handle will be a `transport::Requester<MyServiceRequest>`.
//! - `handles`: struct for collecting named handles for services. The `StackBuilder` uses this to make all handles
//!   available to services.
//! - `transport`: This allows messages to be reliably send/received to/from services. A `Requester`/`Responder` pair is
//!   created using the `transport::channel` function which takes an impl of `tower_service::Service` as it's first
//!   parameter. A `Requester` implements `tower_service::Service` and is used to send requests which return a Future
//!   which resolves to a response. The `Requester` uses a `oneshot` channel allow responses to be sent back. A
//!   `Responder` receives a `(request, oneshot::Sender)` tuple, calls the given tower service with that request and
//!   sends the result on the `oneshot::Sender`. The `Responder` handles many requests simultaneously.

pub mod handles;
mod stack;
pub mod transport;

pub use self::stack::{ServiceInitializationError, ServiceInitializer, StackBuilder};
