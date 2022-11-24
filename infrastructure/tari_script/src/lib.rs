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
pub use op_codes::{slice_to_boxed_hash, slice_to_hash, HashValue, Message, Opcode, OpcodeVersion, ScalarValue};
pub use script::TariScript;
pub use script_context::ScriptContext;
pub use stack::{ExecutionStack, StackItem};

// As hex: c5a1ea6d3e0a6a0d650c99489bcd563e37a06221fd04b8f3a842a982b2813907
pub const DEFAULT_SCRIPT_HASH: HashValue = [
    0x26, 0x2b, 0x84, 0x98, 0x22, 0x71, 0x5f, 0x63, 0xa8, 0x8a, 0x6d, 0x0d, 0xd8, 0x70, 0x37, 0x76, 0x56, 0x47, 0x25,
    0x60, 0xff, 0x9e, 0x98, 0x3b, 0x31, 0xd3, 0xab, 0x6c, 0x75, 0x45, 0x97, 0x8b,
];
