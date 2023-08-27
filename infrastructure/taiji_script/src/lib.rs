// Copyright 2020. The Taiji Project
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
mod script_context;
mod serde;
mod stack;

pub use error::ScriptError;
pub use op_codes::{slice_to_boxed_hash, slice_to_hash, HashValue, Message, Opcode, OpcodeVersion, ScalarValue};
pub use script::TaijiScript;
pub use script_context::ScriptContext;
pub use stack::{ExecutionStack, StackItem};
use tari_crypto::ristretto::RistrettoPublicKey;

// As hex: c5a1ea6d3e0a6a0d650c99489bcd563e37a06221fd04b8f3a842a982b2813907
pub const DEFAULT_SCRIPT_HASH: HashValue = [
    0xc5, 0xa1, 0xea, 0x6d, 0x3e, 0x0a, 0x6a, 0x0d, 0x65, 0x0c, 0x99, 0x48, 0x9b, 0xcd, 0x56, 0x3e, 0x37, 0xa0, 0x62,
    0x21, 0xfd, 0x04, 0xb8, 0xf3, 0xa8, 0x42, 0xa9, 0x82, 0xb2, 0x81, 0x39, 0x07,
];

/// The standard payment script to be used for one-sided payment to stealth addresses
pub fn stealth_payment_script(
    nonce_public_key: &RistrettoPublicKey,
    script_spending_key: &RistrettoPublicKey,
) -> TaijiScript {
    script!(PushPubKey(Box::new(nonce_public_key.clone())) Drop PushPubKey(Box::new(script_spending_key.clone())))
}

/// The standard payment script to be used for one-sided payment to public addresses
pub fn one_sided_payment_script(destination_public_key: &RistrettoPublicKey) -> TaijiScript {
    script!(PushPubKey(Box::new(destination_public_key.clone())))
}
