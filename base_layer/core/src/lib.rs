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
#![recursion_limit = "1024"]
// Used to eliminate the need for boxing futures in many cases.
// Tracking issue: https://github.com/rust-lang/rust/issues/63063
#![feature(type_alias_impl_trait)]
// Enable usage of Vec::shrink_to
#![feature(shrink_to)]

#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate cfg_if;

cfg_if! {
    if #[cfg(feature = "base_node")] {
        pub mod blocks;
        pub mod chain_storage;
        pub mod consensus;
        pub mod helpers;
        pub mod mining;
        pub mod proof_of_work;
        pub mod validation;
    }
}

cfg_if! {
    if #[cfg(any(feature = "base_node", feature = "base_node_proto"))] {
        pub mod base_node;
        pub mod proto;
    }
}

#[cfg(any(feature = "base_node", feature = "mempool_proto"))]
pub mod mempool;

#[cfg(feature = "transactions")]
pub mod transactions;

// Re-export the crypto crate to make exposing traits etc easier for clients of this crate
pub use crypto::tari_utilities;
pub use tari_crypto as crypto;
