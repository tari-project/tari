//  Copyright 2022. The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use chrono::NaiveDateTime;
use serde::Serialize;
use tari_common_types::burn_proof::{BurnClaimProof, EncodedMerkleProof};
use tari_transaction_components::transaction_components::TransactionKernel;

#[derive(Debug, Clone)]
pub struct UiBurnProof {
    pub id: i32,
    pub proof: BurnClaimProof,
    pub encoded_merkle_proof: Option<EncodedMerkleProof>,
    pub kernel: TransactionKernel,
    pub burned_at: NaiveDateTime,
}

impl UiBurnProof {
    pub fn to_confirmed_proof(&self) -> Option<ConfirmedBurnClaimProof<'_>> {
        self.encoded_merkle_proof.as_ref().map(|mp| ConfirmedBurnClaimProof {
            claim_proof: &self.proof,
            merkle_proof: mp,
            kernel: &self.kernel,
        })
    }
}

/// Used to save the proof to a file
#[derive(Debug, Clone, Serialize)]
pub struct ConfirmedBurnClaimProof<'a> {
    pub claim_proof: &'a BurnClaimProof,
    pub merkle_proof: &'a EncodedMerkleProof,
    pub kernel: &'a TransactionKernel,
}
