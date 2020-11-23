// Copyright 2018 The Tari Project
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

// Needed to make futures::select! work
#![recursion_limit = "512"]
// Used to eliminate the need for boxing futures in many cases.
// Tracking issue: https://github.com/rust-lang/rust/issues/63063
#![feature(type_alias_impl_trait)]
#![feature(shrink_to)]
#![cfg_attr(not(debug_assertions), deny(unused_variables))]
#![cfg_attr(not(debug_assertions), deny(unused_imports))]
#![cfg_attr(not(debug_assertions), deny(dead_code))]
#![cfg_attr(not(debug_assertions), deny(unused_extern_crates))]
#![deny(unused_must_use)]
#![deny(unreachable_patterns)]
#![deny(unknown_lints)]

#[macro_use]
extern crate bitflags;

#[cfg(feature = "base_node")]
pub mod blocks;
#[cfg(feature = "base_node")]
pub mod chain_storage;
#[cfg(any(feature = "base_node", feature = "transactions"))]
pub mod consensus;
#[cfg(feature = "base_node")]
pub mod iterators;
#[cfg(feature = "base_node")]
pub mod mining;
#[cfg(any(feature = "base_node", feature = "transactions"))]
pub mod proof_of_work;
#[cfg(feature = "base_node")]
pub mod validation;

#[cfg(any(test, feature = "base_node"))]
pub mod test_helpers;

#[cfg(any(feature = "base_node", feature = "base_node_proto"))]
pub mod base_node;
#[cfg(any(feature = "base_node", feature = "base_node_proto"))]
pub mod proto;

#[cfg(any(feature = "base_node", feature = "mempool_proto"))]
pub mod mempool;

#[cfg(feature = "transactions")]
pub mod transactions;

// Re-export the crypto crate to make exposing traits etc easier for clients of this crate
pub use crypto::tari_utilities;
pub use tari_crypto as crypto;

#[allow(clippy::ptr_offset_with_cast)]
#[allow(clippy::assign_op_pattern)]
pub mod large_ints {
    uint::construct_uint! {
        /// 256-bit unsigned integer.
        pub struct U256(4);
    }

    uint::construct_uint! {
        /// 512-bit unsigned integer.
        pub struct U512(8);
    }
}
pub use large_ints::{U256, U512};
