//  Copyright 2023. The Tari Project
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

use serde::{Deserialize, Serialize};

use crate::{
    serializers,
    types::{BlockHash, CompressedCommitment, CompressedPublicKey, CompressedSignature},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartialBurnClaimProof {
    /// The L2 account public key (`P`) that the burn is intended for. User-facing identifier;
    /// for stealth-shaped proofs the on-wire claim key used in `ownership_proof` is the
    /// stealth address `C` (see `stealth_claim_public_key`), not `P` directly.
    pub claim_public_key: CompressedPublicKey,
    /// For stealth-shaped proofs, this is the stealth address `C = H(r·P)·G + P` that the
    /// `ownership_proof` Schnorr signature commits to (as `H(commitment ‖ C)`), and that the
    /// L2 wallet must derive the spend secret `s = H(R·p) + p` against in order to claim.
    /// `None` for legacy-shaped proofs where `ownership_proof` commits to `claim_public_key`
    /// (= `P`) directly and the L2 wallet spends with `p`.
    #[serde(default)]
    pub stealth_claim_public_key: Option<CompressedPublicKey>,
    pub commitment: CompressedCommitment,
    pub ownership_proof: CompressedSignature,
    #[serde(with = "serializers::base64")]
    pub kernel_excess: Vec<u8>,
    #[serde(with = "serializers::base64")]
    pub kernel_excess_nonce: Vec<u8>,
    #[serde(with = "serializers::base64")]
    pub kernel_excess_signature: Vec<u8>,
    pub sender_offset_public_key: CompressedPublicKey,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodedMerkleProof {
    #[serde(with = "serializers::base64")]
    pub block_hash: BlockHash,
    #[serde(with = "serializers::base64")]
    pub encoded_merkle_proof: Vec<u8>,
    pub leaf_index: u64,
}
