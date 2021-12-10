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
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE
//
// Portions of this file were originally copyrighted (c) 2018 The Grin Developers, issued under the Apache License,
// Version 2.0, available at http://www.apache.org/licenses/LICENSE-2.0.

use tari_common_types::types::BlindingFactor;
use tari_crypto::range_proof::{FullRewindResult as CryptoFullRewindResult, REWIND_USER_MESSAGE_LENGTH};

use crate::transactions::tari_amount::MicroTari;

/// A wrapper struct to hold the result of a successful range proof full rewinding to reveal the committed value, proof
/// message and blinding factor
#[derive(Debug, PartialEq)]
pub struct FullRewindResult {
    pub committed_value: MicroTari,
    pub proof_message: [u8; REWIND_USER_MESSAGE_LENGTH],
    pub blinding_factor: BlindingFactor,
}

impl FullRewindResult {
    pub fn new(
        committed_value: MicroTari,
        proof_message: [u8; REWIND_USER_MESSAGE_LENGTH],
        blinding_factor: BlindingFactor,
    ) -> Self {
        Self {
            committed_value,
            proof_message,
            blinding_factor,
        }
    }
}

impl From<CryptoFullRewindResult<BlindingFactor>> for FullRewindResult {
    fn from(crr: CryptoFullRewindResult<BlindingFactor>) -> Self {
        Self {
            committed_value: crr.committed_value.into(),
            proof_message: crr.proof_message,
            blinding_factor: crr.blinding_factor,
        }
    }
}
