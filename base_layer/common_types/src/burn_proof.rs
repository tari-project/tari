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
    types::{BlockHash, CompressedCommitment, CompressedPublicKey, CompressedSignature, FixedHash},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartialBurnClaimProof {
    /// The L2 account public key (`P`) the burn is intended for. Lets the L2 wallet route the
    /// claim to the right account and derive the stealth claim key `C = H(R·p)·G + P` against
    /// which `ownership_proof` is signed (`R = sender_offset_public_key`, `p` = the L2 account
    /// secret). `C` itself is not carried on the wire — both L1 and L2 can compute it from
    /// `(R, P, p)` and the on-chain `ConfidentialOutputData.claim_public_key` echoes it.
    pub claim_public_key: CompressedPublicKey,
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

/// An L1 kernel MMR inclusion proof in a self-describing form, so that consumers can verify it without decoding the
/// base node's bincode encoding of `tari_mmr::MerkleProof`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KernelMerkleProof {
    /// The hash of the block the kernel was mined in
    #[serde(with = "serializers::hex")]
    pub block_hash: BlockHash,
    /// The index of the kernel leaf in the kernel MMR
    pub leaf_index: u64,
    /// The size of the kernel MMR at the time the proof was created
    pub mmr_size: u64,
    /// The sibling path from the leaf up to its local peak
    #[serde(with = "serializers::hex_seq")]
    pub path: Vec<FixedHash>,
    /// The MMR peaks, excluding the local peak of the leaf
    #[serde(with = "serializers::hex_seq")]
    pub peaks: Vec<FixedHash>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kernel_merkle_proof_json_round_trips_as_hex() {
        let proof = KernelMerkleProof {
            block_hash: FixedHash::from([1u8; 32]),
            leaf_index: 7,
            mmr_size: 11,
            path: vec![FixedHash::from([2u8; 32]), FixedHash::from([3u8; 32])],
            peaks: vec![FixedHash::from([4u8; 32])],
        };
        let json = serde_json::to_value(&proof).unwrap();
        assert_eq!(json["block_hash"], "01".repeat(32));
        assert_eq!(json["path"][1], "03".repeat(32));
        assert_eq!(json["peaks"][0], "04".repeat(32));
        assert_eq!(serde_json::from_value::<KernelMerkleProof>(json).unwrap(), proof);
    }

    #[test]
    fn kernel_merkle_proof_rejects_wrong_hash_length() {
        let json = serde_json::json!({
            "block_hash": "01".repeat(32),
            "leaf_index": 0,
            "mmr_size": 1,
            "path": ["0102"],
            "peaks": [],
        });
        serde_json::from_value::<KernelMerkleProof>(json).unwrap_err();
    }
}
