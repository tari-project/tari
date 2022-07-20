// Copyright 2020. The Tari Project
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

mod error;
mod op_codes;
mod script;
mod script_commitment;
mod script_context;
mod serde;
mod stack;

pub use error::ScriptError;
pub use op_codes::{slice_to_boxed_hash, slice_to_hash, HashValue, Opcode};
pub use script::TariScript;
pub use script_commitment::{ScriptCommitment, ScriptCommitmentError, ScriptCommitmentFactory};
pub use script_context::ScriptContext;
pub use stack::{ExecutionStack, StackItem};
use tari_common::hashing_domain::HashingDomain;

/// The TariScript domain separated hashing domain
/// Usage:
///   let hash = tari_script_hash_domain().digest::<Blake256>(b"my secret");
///   etc.
pub fn tari_script_hash_domain() -> HashingDomain {
    HashingDomain::new("infrastructure.tari_script")
}

// As hex: 6dc4e4d80f1221fcf6a7389369760b122895f69576af03f1e123f1f3559cfc7b
pub const DEFAULT_SCRIPT_HASH: HashValue = [
    0x6d, 0xc4, 0xe4, 0xd8, 0x0f, 0x12, 0x21, 0xfc, 0xf6, 0xa7, 0x38, 0x93, 0x69, 0x76, 0x0b, 0x12, 0x28, 0x95, 0xf6,
    0x95, 0x76, 0xaf, 0x03, 0xf1, 0xe1, 0x23, 0xf1, 0xf3, 0x55, 0x9c, 0xfc, 0x7b,
];
