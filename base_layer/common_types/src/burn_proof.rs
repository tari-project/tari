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
    types::{BlockHash, ComAndPubSignature, CompressedCommitment, CompressedPublicKey, CompressedSignature, FixedHash},
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

/// An inclusion proof of a leaf in a Merkle mountain range, in a self-describing form so that consumers can verify it
/// without decoding the bincode encoding of `tari_mmr::MerkleProof`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MmrInclusionProof {
    /// The index of the leaf in the MMR
    pub leaf_index: u64,
    /// The size (node count) of the MMR the proof was created for
    pub mmr_size: u64,
    /// The sibling path from the leaf up to its local peak
    #[serde(with = "serializers::hex_seq")]
    pub path: Vec<FixedHash>,
    /// The MMR peaks, excluding the local peak of the leaf
    #[serde(with = "serializers::hex_seq")]
    pub peaks: Vec<FixedHash>,
}

/// The fields of a `TransactionOutput` that its hash commits to, in hash order. The range proof is carried only as its
/// hash, which is all the output hash commits to.
///
/// The fields with types that live in `tari_transaction_components` (`features`, `script`, `covenant` and
/// `encrypted_data`) are carried as their consensus (borsh) encoding, which is exactly what the output hash consumes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputHashPreimage {
    pub version: u8,
    /// Consensus encoded `OutputFeatures`
    #[serde(with = "serializers::hex")]
    pub features: Vec<u8>,
    pub commitment: CompressedCommitment,
    #[serde(with = "serializers::hex")]
    pub rangeproof_hash: FixedHash,
    /// Consensus encoded `TariScript`
    #[serde(with = "serializers::hex")]
    pub script: Vec<u8>,
    pub sender_offset_public_key: CompressedPublicKey,
    pub metadata_signature: ComAndPubSignature,
    /// Consensus encoded `Covenant`
    #[serde(with = "serializers::hex")]
    pub covenant: Vec<u8>,
    /// Consensus encoded `EncryptedData`
    #[serde(with = "serializers::hex")]
    pub encrypted_data: Vec<u8>,
    pub minimum_value_promise: u64,
}

/// Proves that a burn output was mined in a block, against that block's `block_output_mr`.
///
/// The block output MMR has the coinbase output hashes as leaves, followed by one last leaf: the root of the MMR of
/// every other output hash in the block (the normal output MMR). Burn outputs are never coinbases, so the proof is in
/// two levels: the output hash in the normal output MMR, then the normal output MMR root in the block output MMR.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BurnOutputProof {
    /// The hash of the block the output was mined in
    #[serde(with = "serializers::hex")]
    pub block_hash: BlockHash,
    /// The height of the block the output was mined in
    pub block_height: u64,
    /// The burn output
    pub output: OutputHashPreimage,
    /// The inclusion proof of the output hash in the normal output MMR
    pub normal_output_proof: MmrInclusionProof,
    /// The root of the normal output MMR
    #[serde(with = "serializers::hex")]
    pub normal_output_mr: FixedHash,
    /// The inclusion proof of `normal_output_mr` as the last leaf of the block output MMR
    pub block_output_proof: MmrInclusionProof,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mmr_inclusion_proof_json_round_trips_as_hex() {
        let proof = MmrInclusionProof {
            leaf_index: 7,
            mmr_size: 11,
            path: vec![FixedHash::from([2u8; 32]), FixedHash::from([3u8; 32])],
            peaks: vec![FixedHash::from([4u8; 32])],
        };
        let json = serde_json::to_value(&proof).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "leaf_index": 7,
                "mmr_size": 11,
                "path": ["02".repeat(32), "03".repeat(32)],
                "peaks": ["04".repeat(32)],
            })
        );
        assert_eq!(serde_json::from_value::<MmrInclusionProof>(json).unwrap(), proof);
    }

    #[test]
    fn mmr_inclusion_proof_rejects_wrong_hash_length() {
        let json = serde_json::json!({
            "leaf_index": 0,
            "mmr_size": 1,
            "path": ["0102"],
            "peaks": [],
        });
        serde_json::from_value::<MmrInclusionProof>(json).unwrap_err();
    }

    #[test]
    fn burn_output_proof_json_round_trips() {
        let proof = BurnOutputProof {
            block_hash: FixedHash::from([1u8; 32]),
            block_height: 123,
            output: OutputHashPreimage {
                version: 1,
                features: vec![1, 2, 3],
                commitment: Default::default(),
                rangeproof_hash: FixedHash::from([5u8; 32]),
                script: vec![4],
                sender_offset_public_key: Default::default(),
                metadata_signature: Default::default(),
                covenant: vec![0],
                encrypted_data: vec![6; 80],
                minimum_value_promise: 0,
            },
            normal_output_proof: MmrInclusionProof {
                leaf_index: 1,
                mmr_size: 3,
                path: vec![FixedHash::from([2u8; 32])],
                peaks: vec![],
            },
            normal_output_mr: FixedHash::from([7u8; 32]),
            block_output_proof: MmrInclusionProof {
                leaf_index: 1,
                mmr_size: 3,
                path: vec![FixedHash::from([3u8; 32])],
                peaks: vec![],
            },
        };
        let json = serde_json::to_value(&proof).unwrap();
        assert_eq!(json.pointer("/output/features").unwrap(), "010203");
        assert_eq!(serde_json::from_value::<BurnOutputProof>(json).unwrap(), proof);
    }
}
