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

use tari_crypto::ristretto::pedersen::PedersenCommitment;

use crate::HashValue;

/// Contextual data for use in Taiji scripts. The context will typically be unambiguously and deterministically
/// populated by nodes that are executing the script.
#[derive(Debug, Clone, Default)]
pub struct ScriptContext {
    /// The height of the chain where the UTXO is being _spent_; not the height the UTXO was created in
    block_height: u64,
    /// The hash of the previous block's hash
    prev_block_hash: HashValue,
    /// The commitment of the UTXO that is attached to this script
    commitment: PedersenCommitment,
}

impl ScriptContext {
    pub fn new(height: u64, prev_hash: &HashValue, com: &PedersenCommitment) -> Self {
        ScriptContext {
            block_height: height,
            prev_block_hash: *prev_hash,
            commitment: com.clone(),
        }
    }

    pub fn block_height(&self) -> u64 {
        self.block_height
    }

    pub fn prev_block_hash(&self) -> &HashValue {
        &self.prev_block_hash
    }

    pub fn commitment(&self) -> &PedersenCommitment {
        &self.commitment
    }
}
