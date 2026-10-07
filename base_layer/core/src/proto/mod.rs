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

/// The decode budget (see `tari_comms::decode_budget`) of the RPC methods that carry a whole block body or
/// transaction: sync_blocks, mempool get_state and submit_transaction, and the wallet's submit_transaction. The
/// per-network test in `decode_budget_tests` checks a max-weight block fits it on every network.
pub const BODY_MAX_DECODE_ITEMS: usize = 262_144;

/// The decode budget for transactions, blocks and base node messages received over messaging: the same as the RPC
/// block-body budget. A messaging frame is up to 8 MiB, so without it a single frame of empty inputs decodes into about
/// 1 GB.
pub const MESSAGE_MAX_DECODE_ITEMS: usize = BODY_MAX_DECODE_ITEMS;

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

pub mod types {
    include!(concat!(env!("OUT_DIR"), "/tari.types.rs"));
}

mod block;
mod block_header;
mod sidechain_feature;

#[cfg(test)]
mod decode_budget_tests;
