// Copyright 2019. The Tari Project
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

#[cfg(feature = "base_node")]
mod error;
#[cfg(feature = "base_node")]
mod inbound_handlers;
#[cfg(feature = "base_node")]
mod initializer;
#[cfg(feature = "base_node")]
mod local_service;
#[cfg(feature = "base_node")]
mod outbound_interface;
#[allow(clippy::module_inception)]
#[cfg(feature = "base_node")]
mod service;

// Public re-exports
#[cfg(feature = "base_node")]
pub use error::MempoolServiceError;
#[cfg(feature = "base_node")]
pub use initializer::MempoolServiceInitializer;
#[cfg(feature = "base_node")]
pub use local_service::LocalMempoolService;
#[cfg(feature = "base_node")]
pub use outbound_interface::OutboundMempoolServiceInterface;
#[cfg(feature = "base_node")]
pub use service::MempoolService;

mod request;
mod response;

pub use request::{MempoolRequest, MempoolServiceRequest};
pub use response::{MempoolResponse, MempoolServiceResponse};
