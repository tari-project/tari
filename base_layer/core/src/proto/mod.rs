// Copyright 2019, The Tari Project
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

//! Imports of code generated from protobuf files

pub mod transaction;
mod types_impls;

#[allow(clippy::large_enum_variant)]
pub mod base_node {
    include!(concat!(env!("OUT_DIR"), "/tari.base_node.rs"));
}

pub mod core {
    include!(concat!(env!("OUT_DIR"), "/tari.core.rs"));
}

pub mod mempool {
    include!(concat!(env!("OUT_DIR"), "/tari.mempool.rs"));
}

#[allow(clippy::large_enum_variant)]
pub mod transaction_protocol {
    include!(concat!(env!("OUT_DIR"), "/tari.transaction_protocol.rs"));
}

pub mod types {
    include!(concat!(env!("OUT_DIR"), "/tari.types.rs"));
}

#[cfg(feature = "base_node")]
mod block;
#[cfg(any(feature = "base_node", feature = "base_node_proto"))]
mod block_header;
#[cfg(any(feature = "base_node", feature = "base_node_proto"))]
mod sidechain_features;
#[cfg(any(feature = "base_node", feature = "base_node_proto"))]
mod utils;
