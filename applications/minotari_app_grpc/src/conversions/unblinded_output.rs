// Copyright 2021. The Tari Project
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

use std::{
    convert::{TryFrom, TryInto},
    fmt,
};

use borsh::{BorshDeserialize, BorshSerialize};
use tari_common_types::types::{CompressedPublicKey, PrivateKey, RangeProof};
use tari_script::{ExecutionStack, TariScript};
use tari_transaction_components::{
    MicroMinotari,
    transaction_components::{EncryptedData, TransactionOutputVersion, UnblindedOutput},
};
use tari_utilities::{ByteArray, ByteArrayError};
use zeroize::Zeroize;

use crate::tari_rpc as grpc;

// Hand-written `Debug` implementations for protobuf messages that carry private key bytes.
// Keep the field lists in sync with `applications/minotari_app_grpc/proto/transaction.proto`
// (`UnblindedOutput`) and `applications/minotari_app_grpc/proto/wallet.proto`
// (`ImportUtxosRequest`, `CreateBurnTransactionRequest`, `RegisterValidatorNodeRequest`,
// `SubmitValidatorNodeExitRequest`).
impl fmt::Debug for grpc::UnblindedOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UnblindedOutput")
            .field("value", &self.value)
            .field("spending_key", &"<redacted>")
            .field("features", &self.features)
            .field("script", &self.script)
            .field("input_data", &self.input_data)
            .field("script_private_key", &"<redacted>")
            .field("sender_offset_public_key", &self.sender_offset_public_key)
            .field("metadata_signature", &self.metadata_signature)
            .field("script_lock_height", &self.script_lock_height)
            .field("covenant", &self.covenant)
            .field("encrypted_data", &self.encrypted_data)
            .field("minimum_value_promise", &self.minimum_value_promise)
            .field("range_proof", &self.range_proof)
            .finish()
    }
}

impl fmt::Debug for grpc::ImportUtxosRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImportUtxosRequest")
            .field("outputs", &self.outputs)
            .field("payment_id", &self.payment_id)
            .finish()
    }
}

impl fmt::Debug for grpc::CreateBurnTransactionRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CreateBurnTransactionRequest")
            .field("amount", &self.amount)
            .field("fee_per_gram", &self.fee_per_gram)
            .field("claim_public_key", &self.claim_public_key)
            .field("payment_id", &self.payment_id)
            .field("sidechain_deployment_key", &"<redacted>")
            .finish()
    }
}

impl fmt::Debug for grpc::RegisterValidatorNodeRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RegisterValidatorNodeRequest")
            .field("validator_node_public_key", &self.validator_node_public_key)
            .field("validator_node_signature", &self.validator_node_signature)
            .field("validator_node_claim_public_key", &self.validator_node_claim_public_key)
            .field("max_epoch", &self.max_epoch)
            .field("fee_per_gram", &self.fee_per_gram)
            .field("payment_id", &self.payment_id)
            .field("sidechain_deployment_key", &"<redacted>")
            .finish()
    }
}

impl fmt::Debug for grpc::SubmitValidatorNodeExitRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SubmitValidatorNodeExitRequest")
            .field("validator_node_public_key", &self.validator_node_public_key)
            .field("validator_node_signature", &self.validator_node_signature)
            .field("max_epoch", &self.max_epoch)
            .field("fee_per_gram", &self.fee_per_gram)
            .field("message", &self.message)
            .field("sidechain_deployment_key", &"<redacted>")
            .field("activation_epoch", &self.activation_epoch)
            .finish()
    }
}

fn parse_and_zeroize_optional_private_key(buf: &mut Vec<u8>) -> Result<Option<PrivateKey>, ByteArrayError> {
    let result = if buf.is_empty() {
        Ok(None)
    } else {
        PrivateKey::from_canonical_bytes(buf.as_slice()).map(Some)
    };
    buf.as_mut_slice().zeroize();
    result
}

impl grpc::CreateBurnTransactionRequest {
    pub fn take_sidechain_deployment_key(&mut self) -> Result<Option<PrivateKey>, ByteArrayError> {
        parse_and_zeroize_optional_private_key(&mut self.sidechain_deployment_key)
    }
}

impl grpc::RegisterValidatorNodeRequest {
    pub fn take_sidechain_deployment_key(&mut self) -> Result<Option<PrivateKey>, ByteArrayError> {
        parse_and_zeroize_optional_private_key(&mut self.sidechain_deployment_key)
    }
}

impl grpc::SubmitValidatorNodeExitRequest {
    pub fn take_sidechain_deployment_key(&mut self) -> Result<Option<PrivateKey>, ByteArrayError> {
        parse_and_zeroize_optional_private_key(&mut self.sidechain_deployment_key)
    }
}

struct ZeroizeOutputKeys<'a>(&'a mut grpc::UnblindedOutput);

impl Drop for ZeroizeOutputKeys<'_> {
    fn drop(&mut self) {
        self.0.spending_key.as_mut_slice().zeroize();
        self.0.script_private_key.as_mut_slice().zeroize();
    }
}

impl TryFrom<UnblindedOutput> for grpc::UnblindedOutput {
    type Error = String;

    fn try_from(output: UnblindedOutput) -> Result<Self, Self::Error> {
        let mut covenant = Vec::new();
        BorshSerialize::serialize(&output.covenant, &mut covenant).map_err(|err| err.to_string())?;
        Ok(grpc::UnblindedOutput {
            value: u64::from(output.value),
            spending_key: output.commitment_mask_key.as_bytes().to_vec(),
            features: Some(output.features.into()),
            script: output.script.to_bytes(),
            input_data: output.input_data.to_bytes(),
            script_private_key: output.script_private_key.as_bytes().to_vec(),
            sender_offset_public_key: output.sender_offset_public_key.as_bytes().to_vec(),
            metadata_signature: Some(grpc::ComAndPubSignature {
                ephemeral_commitment: Vec::from(output.metadata_signature.ephemeral_commitment().as_bytes()),
                ephemeral_pubkey: Vec::from(output.metadata_signature.ephemeral_pubkey().as_bytes()),
                u_a: Vec::from(output.metadata_signature.u_a().as_bytes()),
                u_x: Vec::from(output.metadata_signature.u_x().as_bytes()),
                u_y: Vec::from(output.metadata_signature.u_y().as_bytes()),
            }),
            script_lock_height: output.script_lock_height,
            covenant,
            encrypted_data: output.encrypted_data.to_byte_vec(),
            minimum_value_promise: output.minimum_value_promise.into(),
            range_proof: {
                let proof_bytes = if let Some(proof) = output.range_proof {
                    proof.to_vec()
                } else {
                    vec![]
                };
                Some(grpc::RangeProof { proof_bytes })
            },
        })
    }
}

impl TryFrom<grpc::UnblindedOutput> for UnblindedOutput {
    type Error = String;

    fn try_from(mut output: grpc::UnblindedOutput) -> Result<Self, Self::Error> {
        convert_unblinded_output(&mut output)
    }
}

fn convert_unblinded_output(output: &mut grpc::UnblindedOutput) -> Result<UnblindedOutput, String> {
    let guard = ZeroizeOutputKeys(output);
    let output = &mut *guard.0;

    let spending_key =
        PrivateKey::from_canonical_bytes(output.spending_key.as_bytes()).map_err(|e| format!("spending_key: {e:?}"))?;

    let features = output
        .features
        .take()
        .map(TryInto::try_into)
        .ok_or_else(|| "output features not provided".to_string())??;

    let script = TariScript::from_bytes(output.script.as_bytes()).map_err(|e| format!("script: {e:?}"))?;

    let input_data =
        ExecutionStack::from_bytes(output.input_data.as_bytes()).map_err(|e| format!("input_data: {e:?}"))?;

    let script_private_key = PrivateKey::from_canonical_bytes(output.script_private_key.as_bytes())
        .map_err(|e| format!("script_private_key: {e:?}"))?;

    let sender_offset_public_key =
        CompressedPublicKey::from_canonical_bytes(output.sender_offset_public_key.as_bytes())
            .map_err(|err| format!("sender_offset_public_key {err:?}"))?;

    let metadata_signature = output
        .metadata_signature
        .take()
        .ok_or_else(|| "Metadata signature not provided".to_string())?
        .try_into()
        .map_err(|_| "Metadata signature could not be converted".to_string())?;

    let mut buffer = output.covenant.as_bytes();
    let covenant = BorshDeserialize::deserialize(&mut buffer).map_err(|err| err.to_string())?;

    let encrypted_data = EncryptedData::from_bytes(&output.encrypted_data).map_err(|err| err.to_string())?;

    let minimum_value_promise = MicroMinotari::from(output.minimum_value_promise);

    let range_proof = if let Some(proof) = output.range_proof.as_ref() {
        if proof.proof_bytes.is_empty() {
            None
        } else {
            let proof = RangeProof::from_vec(&proof.proof_bytes).map_err(|e| format!("Range proof is invalid: {e}"))?;
            Some(proof)
        }
    } else {
        return Err("Range proof not provided".to_string());
    };

    Ok(UnblindedOutput::new(
        TransactionOutputVersion::try_from(0u8)?,
        MicroMinotari::from(output.value),
        spending_key,
        features,
        script,
        input_data,
        script_private_key,
        sender_offset_public_key,
        metadata_signature,
        output.script_lock_height,
        covenant,
        encrypted_data,
        minimum_value_promise,
        range_proof,
    ))
}

#[cfg(test)]
mod test {
    use borsh::BorshSerialize;
    use tari_common_types::types::{CompressedPublicKey, MAX_RANGE_PROOF_BYTES, PrivateKey};
    use tari_transaction_components::transaction_components::{EncryptedData, covenants::Covenant};
    use tari_utilities::ByteArray;

    use super::convert_unblinded_output;
    use crate::tari_rpc as grpc;

    fn sample_grpc_unblinded_output() -> grpc::UnblindedOutput {
        let spending_key = PrivateKey::from_canonical_bytes(&[1u8; 32]).unwrap();
        let script_private_key = PrivateKey::from_canonical_bytes(&[2u8; 32]).unwrap();
        let sender_offset_public_key =
            CompressedPublicKey::from_secret_key(&PrivateKey::from_canonical_bytes(&[3u8; 32]).unwrap());
        let u_a = PrivateKey::from_canonical_bytes(&[4u8; 32]).unwrap();
        let u_x = PrivateKey::from_canonical_bytes(&[5u8; 32]).unwrap();
        let u_y = PrivateKey::from_canonical_bytes(&[6u8; 32]).unwrap();
        let ephemeral_pubkey =
            CompressedPublicKey::from_secret_key(&PrivateKey::from_canonical_bytes(&[7u8; 32]).unwrap());

        let mut covenant = Vec::new();
        Covenant::default().serialize(&mut covenant).unwrap();

        grpc::UnblindedOutput {
            value: 1_000,
            spending_key: spending_key.as_bytes().to_vec(),
            features: Some(grpc::OutputFeatures::default()),
            script: Vec::new(),
            input_data: Vec::new(),
            script_private_key: script_private_key.as_bytes().to_vec(),
            sender_offset_public_key: sender_offset_public_key.as_bytes().to_vec(),
            metadata_signature: Some(grpc::ComAndPubSignature {
                ephemeral_commitment: vec![0u8; 32],
                ephemeral_pubkey: ephemeral_pubkey.as_bytes().to_vec(),
                u_a: u_a.as_bytes().to_vec(),
                u_x: u_x.as_bytes().to_vec(),
                u_y: u_y.as_bytes().to_vec(),
            }),
            script_lock_height: 0,
            covenant,
            encrypted_data: EncryptedData::default().to_byte_vec(),
            minimum_value_promise: 0,
            range_proof: Some(grpc::RangeProof {
                proof_bytes: Vec::new(),
            }),
        }
    }

    #[test]
    fn try_from_zeroizes_keys_when_range_proof_fails() {
        let mut output = sample_grpc_unblinded_output();
        assert!(output.spending_key.iter().any(|&b| b != 0));
        assert!(output.script_private_key.iter().any(|&b| b != 0));

        // Corrupt the range proof so conversion fails at the final step after both keys were parsed.
        output.range_proof = Some(grpc::RangeProof {
            proof_bytes: vec![0xAB; MAX_RANGE_PROOF_BYTES + 1],
        });

        let err = convert_unblinded_output(&mut output).unwrap_err();
        assert!(err.starts_with("Range proof is invalid:"), "unexpected error: {err}");
        assert_eq!(output.spending_key, vec![0u8; 32]);
        assert_eq!(output.script_private_key, vec![0u8; 32]);

        let mut output_missing_proof = sample_grpc_unblinded_output();
        output_missing_proof.range_proof = None;
        let err = convert_unblinded_output(&mut output_missing_proof).unwrap_err();
        assert_eq!(err, "Range proof not provided");
        assert_eq!(output_missing_proof.spending_key, vec![0u8; 32]);
        assert_eq!(output_missing_proof.script_private_key, vec![0u8; 32]);
    }

    #[test]
    fn try_from_zeroizes_both_keys_on_early_return_before_script_private_key() {
        let mut output = sample_grpc_unblinded_output();
        assert!(output.spending_key.iter().any(|&b| b != 0));
        assert!(output.script_private_key.iter().any(|&b| b != 0));

        // Fail before `script_private_key` is even read.
        output.features = None;

        let err = convert_unblinded_output(&mut output).unwrap_err();
        assert_eq!(err, "output features not provided");
        assert_eq!(output.spending_key, vec![0u8; 32]);
        assert_eq!(output.script_private_key, vec![0u8; 32]);
    }

    #[test]
    fn debug_redacts_spending_and_script_private_keys() {
        let mut output = sample_grpc_unblinded_output();
        let spending_marker = vec![0xAA; 32];
        let script_key_marker = vec![0xBB; 32];
        output.spending_key = spending_marker.clone();
        output.script_private_key = script_key_marker.clone();

        let output_debug = format!("{output:?}");
        assert!(output_debug.contains("spending_key: \"<redacted>\""));
        assert!(output_debug.contains("script_private_key: \"<redacted>\""));
        assert!(!output_debug.contains(&format!("{spending_marker:?}")));
        assert!(!output_debug.contains(&format!("{script_key_marker:?}")));

        let request = grpc::ImportUtxosRequest {
            outputs: vec![output],
            payment_id: vec![0x11, 0x22],
        };
        let request_debug = format!("{request:?}");
        assert!(request_debug.contains("<redacted>"));
        assert!(!request_debug.contains(&format!("{spending_marker:?}")));
        assert!(!request_debug.contains(&format!("{script_key_marker:?}")));
        assert!(!request_debug.contains("170, 170"));
        assert!(!request_debug.contains("187, 187"));
    }

    #[test]
    fn debug_redacts_and_zeroizes_sidechain_deployment_key() {
        let valid_key = PrivateKey::from_canonical_bytes(&[9u8; 32]).unwrap();
        let key_bytes = valid_key.as_bytes().to_vec();

        let mut burn_req = grpc::CreateBurnTransactionRequest {
            amount: 100,
            fee_per_gram: 5,
            claim_public_key: vec![1u8; 32],
            payment_id: vec![2u8; 8],
            sidechain_deployment_key: key_bytes.clone(),
        };
        let burn_debug = format!("{burn_req:?}");
        assert!(burn_debug.contains("sidechain_deployment_key: \"<redacted>\""));
        assert!(!burn_debug.contains(&format!("{key_bytes:?}")));
        assert_eq!(
            burn_req.take_sidechain_deployment_key().unwrap(),
            Some(valid_key.clone())
        );
        assert_eq!(burn_req.sidechain_deployment_key, vec![0u8; 32]);

        let mut reg_req = grpc::RegisterValidatorNodeRequest {
            validator_node_public_key: vec![1u8; 32],
            validator_node_signature: None,
            validator_node_claim_public_key: vec![2u8; 32],
            max_epoch: 10,
            fee_per_gram: 5,
            payment_id: Vec::new(),
            sidechain_deployment_key: vec![0xFF; 32],
        };
        let reg_debug = format!("{reg_req:?}");
        assert!(reg_debug.contains("sidechain_deployment_key: \"<redacted>\""));
        assert!(!reg_debug.contains("255, 255"));
        assert!(reg_req.take_sidechain_deployment_key().is_err());
        assert_eq!(reg_req.sidechain_deployment_key, vec![0u8; 32]);

        let mut exit_req = grpc::SubmitValidatorNodeExitRequest {
            validator_node_public_key: vec![1u8; 32],
            validator_node_signature: None,
            max_epoch: 10,
            fee_per_gram: 5,
            message: Vec::new(),
            sidechain_deployment_key: key_bytes.clone(),
            activation_epoch: 1,
        };
        let exit_debug = format!("{exit_req:?}");
        assert!(exit_debug.contains("sidechain_deployment_key: \"<redacted>\""));
        assert!(!exit_debug.contains(&format!("{key_bytes:?}")));
        assert_eq!(exit_req.take_sidechain_deployment_key().unwrap(), Some(valid_key));
        assert_eq!(exit_req.sidechain_deployment_key, vec![0u8; 32]);
    }
}
