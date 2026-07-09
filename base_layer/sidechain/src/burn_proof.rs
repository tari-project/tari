//  Copyright 2026. The Tari Project
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

use tari_common_types::{
    burn_proof::EncodedMerkleProof,
    serializers,
    types::{CompressedCommitment, CompressedPublicKey},
};
use tari_crypto::ristretto::CompressedRistrettoSchnorr;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct CompleteClaimBurnProof {
    pub claim_proof: BurnClaimProof,
    #[serde(with = "serializers::base64")]
    pub encrypted_data: Vec<u8>,
    /// The L1 epoch the burn was mined in (`block_height / vn_epoch_length`). Lets an L2 claimant defer the
    /// claim until L2 has synced past this epoch, avoiding a premature, permanently-rejected submission.
    /// `Option` (via `serde(default)`) so proof files written before this field, or by wallets connected to a
    /// base node that did not report the height, still deserialize (as `None`) and older readers ignore it.
    #[serde(default)]
    pub mined_in_epoch: Option<u64>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct BurnClaimProof {
    /// This is typically the public nonce that the UTXO was burnt with
    pub burn_public_key: CompressedPublicKey,
    pub commitment: CompressedCommitment,
    pub ownership_proof: CompressedRistrettoSchnorr,
    pub encoded_merkle_proof: EncodedMerkleProof,
    pub kernel: AbridgedTransactionKernel,
    pub value: u64,
    pub sender_offset_public_key: CompressedPublicKey,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AbridgedTransactionKernel {
    pub version: u8,
    pub fee: u64,
    pub lock_height: u64,
    pub excess: CompressedCommitment,
    pub excess_sig: CompressedRistrettoSchnorr,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_proof(mined_in_epoch: Option<u64>) -> CompleteClaimBurnProof {
        CompleteClaimBurnProof {
            claim_proof: BurnClaimProof {
                burn_public_key: Default::default(),
                commitment: Default::default(),
                ownership_proof: Default::default(),
                encoded_merkle_proof: EncodedMerkleProof {
                    block_hash: Default::default(),
                    encoded_merkle_proof: vec![1, 2, 3],
                    leaf_index: 7,
                },
                kernel: AbridgedTransactionKernel {
                    version: 0,
                    fee: 100,
                    lock_height: 0,
                    excess: Default::default(),
                    excess_sig: Default::default(),
                },
                value: 12_345,
                sender_offset_public_key: Default::default(),
            },
            encrypted_data: vec![9, 9, 9],
            mined_in_epoch,
        }
    }

    #[test]
    fn mined_in_epoch_round_trips() {
        let proof = sample_proof(Some(42));
        let json = serde_json::to_string(&proof).unwrap();
        assert!(
            json.contains("mined_in_epoch"),
            "serialized proof should contain the field: {json}"
        );
        let parsed: CompleteClaimBurnProof = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.mined_in_epoch, Some(42));
    }

    #[test]
    fn absent_mined_in_epoch_defaults_to_none() {
        // Simulate a proof file written before the field existed by dropping the key entirely, so the reader must
        // rely on `#[serde(default)]` rather than a `null` value being present.
        let mut value = serde_json::to_value(sample_proof(Some(42))).unwrap();
        value.as_object_mut().unwrap().remove("mined_in_epoch");
        assert!(value.get("mined_in_epoch").is_none());

        let parsed: CompleteClaimBurnProof = serde_json::from_value(value).unwrap();
        assert_eq!(parsed.mined_in_epoch, None, "absent field must deserialize as None");
    }
}
