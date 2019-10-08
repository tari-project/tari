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

//! # Comms Middleware
//!
//! Comms Middleware contains the middleware layers that can be composed when processing
//! inbound and outbound comms messages.
//!
//! For example, should you want your messages to be encrypted, you'll add the EncryptionLayer to
//! the outbound middleware stack and the DecryptionLayer to the inbound stack.
//! TODO: This needs to be implemented
//!
//! Middlewares use `tower_layer` and `tower_service`. A Middleware is simply any service which
//! is `Service<InboundMessage, Response = (), Error = MiddlewareError>`. This service will usually
//! be composed of other services by using the `tower_util::ServiceBuilder`.
//!
//! TODO: This code doesnt work
//! ```nocompile
//! let inbound_middleware = ServiceBuilder::new()
//!   .layer(DecryptionLayer::new(..))
//!   .layer(DhtLayer::new(..))
//!   .service(DomainPubsubService::new(..));
//!
//! // IdentityService does nothing
//! let outbound_middleware = IdentityService::new();
//!
//! CommsBuilder::new()
//!   .inbound_middleware(inbound_middleware)
//!   .outbound_middleware(outbound_middleware)
//!   // (...)
//!   .build();
//! ```
// Needed to make futures::select! work
#![recursion_limit = "256"]
#![feature(type_alias_impl_trait)]

pub mod error;
pub mod message;
pub mod pipeline;
pub mod sink;

#[cfg(test)]
mod test_utils;

pub use error::MiddlewareError;
