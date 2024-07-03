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
mod script_context;
mod serde;
mod stack;

pub use error::ScriptError;
pub use op_codes::{
    slice_to_boxed_hash,
    slice_to_boxed_message,
    slice_to_hash,
    HashValue,
    Message,
    Opcode,
    OpcodeVersion,
    ScalarValue,
};
pub use script::TariScript;
pub use script_context::ScriptContext;
pub use stack::{ExecutionStack, StackItem};
use tari_crypto::{
    hash_domain,
    ristretto::{RistrettoPublicKey, RistrettoSecretKey},
    signatures::SchnorrSignature,
};

hash_domain!(CheckSigHashDomain, "com.tari.script.check_sig", 1);

/// The type used for `CheckSig`, `CheckMultiSig`, and related opcodes' signatures
pub type CheckSigSchnorrSignature = SchnorrSignature<RistrettoPublicKey, RistrettoSecretKey, CheckSigHashDomain>;

/// The standard payment script to be used for one-sided payment to public addresses
pub fn push_pubkey_script(destination_public_key: &RistrettoPublicKey) -> TariScript {
    script!(PushPubKey(Box::new(destination_public_key.clone())))
}
