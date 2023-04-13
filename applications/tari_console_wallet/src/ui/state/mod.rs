// Copyright 2020. The Tari Project
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

mod app_state;
mod debouncer;
mod tasks;
mod wallet_event_monitor;

use serde::Serialize;
use tari_common_types::serde_with;

pub use self::app_state::*;

// ----------------------------------------------------------------------------
// TODO: re-implement in a clean way

#[derive(Serialize)]
pub struct CommitmentSignatureBase64 {
    #[serde(with = "serde_with::base64")]
    pub public_nonce: Vec<u8>,
    #[serde(with = "serde_with::base64")]
    pub u: Vec<u8>,
    #[serde(with = "serde_with::base64")]
    pub v: Vec<u8>,
}

#[derive(Serialize)]
pub struct BurntProofBase64 {
    #[serde(with = "serde_with::base64")]
    pub reciprocal_claim_public_key: Vec<u8>,
    #[serde(with = "serde_with::base64")]
    pub commitment: Vec<u8>,
    pub ownership_proof: Option<CommitmentSignatureBase64>,
    #[serde(with = "serde_with::base64")]
    pub range_proof: Vec<u8>,
}
