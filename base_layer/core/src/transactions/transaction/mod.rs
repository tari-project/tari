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

pub use asset_output_features::AssetOutputFeatures;
use blake2::Digest;
pub use error::TransactionError;
pub use full_rewind_result::FullRewindResult;
pub use kernel_builder::KernelBuilder;
pub use kernel_features::KernelFeatures;
pub use kernel_sum::KernelSum;
pub use mint_non_fungible_features::MintNonFungibleFeatures;
pub use output_features::OutputFeatures;
pub use output_flags::OutputFlags;
pub use rewind_result::RewindResult;
pub use side_chain_checkpoint_features::SideChainCheckpointFeatures;
use tari_common_types::types::{Commitment, HashDigest};
use tari_crypto::{script::TariScript, tari_utilities::ByteArray};
pub use template_parameter::TemplateParameter;
pub use transaction::Transaction;
pub use transaction_builder::TransactionBuilder;
pub use transaction_input::TransactionInput;
pub use transaction_kernel::TransactionKernel;
pub use transaction_output::TransactionOutput;
pub use unblinded_output::UnblindedOutput;
pub use unblinded_output_builder::UnblindedOutputBuilder;

use crate::{consensus::ToConsensusBytes, covenants::Covenant};

mod asset_output_features;
mod error;
mod full_rewind_result;
mod kernel_builder;
mod kernel_features;
mod kernel_sum;
mod mint_non_fungible_features;
mod output_features;
mod output_flags;
mod rewind_result;
mod side_chain_checkpoint_features;
mod template_parameter;
// TODO: in future, this module can be renamed
#[allow(clippy::module_inception)]
mod transaction;
mod transaction_builder;
mod transaction_input;
mod transaction_kernel;
mod transaction_output;
mod unblinded_output;
mod unblinded_output_builder;

#[cfg(test)]
mod test;

// Tx_weight(inputs(12,500), outputs(500), kernels(1)) = 126,510 still well enough below block weight of 127,795
pub const MAX_TRANSACTION_INPUTS: usize = 12_500;
pub const MAX_TRANSACTION_OUTPUTS: usize = 500;
pub const MAX_TRANSACTION_RECIPIENTS: usize = 15;

//----------------------------------------     Crate functions   ----------------------------------------------------//

/// Implement the canonical hashing function for TransactionOutput and UnblindedOutput for use in
/// ordering as well as for the output hash calculation for TransactionInput.
///
/// We can exclude the range proof from this hash. The rationale for this is:
/// a) It is a significant performance boost, since the RP is the biggest part of an output
/// b) Range proofs are committed to elsewhere and so we'd be hashing them twice (and as mentioned, this is slow)
/// c) TransactionInputs will now have the same hash as UTXOs, which makes locating STXOs easier when doing reorgs
pub(super) fn hash_output(
    features: &OutputFeatures,
    commitment: &Commitment,
    script: &TariScript,
    covenant: &Covenant,
) -> Vec<u8> {
    HashDigest::new()
        .chain(features.to_consensus_bytes())
        .chain(commitment.as_bytes())
        // .chain(range proof) // See docs as to why we exclude this
        .chain(script.as_bytes())
        .chain(covenant.to_consensus_bytes())
        .finalize()
        .to_vec()
}

//-----------------------------------------       Tests           ----------------------------------------------------//

#[cfg(test)]
mod test {
    use rand::{self, rngs::OsRng};
    use tari_common_types::types::{BlindingFactor, ComSignature, PrivateKey, PublicKey, RangeProof, Signature};
    use tari_crypto::{
        commitment::HomomorphicCommitmentFactory,
        keys::{PublicKey as PublicKeyTrait, SecretKey as SecretKeyTrait},
        range_proof::{RangeProofError, RangeProofService},
        ristretto::pedersen::PedersenCommitmentFactory,
        script,
        script::{ExecutionStack, StackItem},
        tari_utilities::{hex::Hex, Hashable},
    };

    use super::*;
    use crate::{
        transactions::{
            tari_amount::{MicroTari, T},
            test_helpers,
            test_helpers::{TestParams, UtxoTestParams},
            transaction::OutputFeatures,
            transaction_protocol::RewindData,
            CryptoFactories,
        },
        txn_schema,
    };

    #[test]
    fn input_and_output_and_unblinded_output_hash_match() {
        let test_params = TestParams::new();
        let factory = PedersenCommitmentFactory::default();

        let i = test_params.create_unblinded_output(Default::default());
        let output = i.as_transaction_output(&CryptoFactories::default()).unwrap();
        let input = i.as_transaction_input(&factory).unwrap();
        assert_eq!(output.hash(), input.output_hash());
        assert_eq!(output.hash(), i.hash(&CryptoFactories::default()));
    }

    #[test]
    fn unblinded_input() {
        let test_params = TestParams::new();
        let factory = PedersenCommitmentFactory::default();

        let i = test_params.create_unblinded_output(Default::default());
        let input = i
            .as_transaction_input(&factory)
            .expect("Should be able to create transaction input");
        assert!(input.opened_by(&i, &factory).unwrap());
    }

    #[test]
    fn with_maturity() {
        let features = OutputFeatures::with_maturity(42);
        assert_eq!(features.maturity, 42);
        assert_eq!(features.flags, OutputFlags::empty());
    }

    #[test]
    fn range_proof_verification() {
        let factories = CryptoFactories::new(32);
        // Directly test the tx_output verification
        let test_params_1 = TestParams::new();
        let test_params_2 = TestParams::new();
        let output_features = OutputFeatures::default();

        // For testing the max range has been limited to 2^32 so this value is too large.
        let unblinded_output1 = test_params_1.create_unblinded_output(UtxoTestParams {
            value: (2u64.pow(32) - 1u64).into(),
            ..Default::default()
        });
        let script = unblinded_output1.script.clone();
        let tx_output1 = unblinded_output1.as_transaction_output(&factories).unwrap();
        tx_output1.verify_range_proof(&factories.range_proof).unwrap();

        let unblinded_output2 = test_params_2.create_unblinded_output(UtxoTestParams {
            value: (2u64.pow(32) + 1u64).into(),
            ..Default::default()
        });
        let tx_output2 = unblinded_output2.as_transaction_output(&factories);
        match tx_output2 {
            Ok(_) => panic!("Range proof should have failed to verify"),
            Err(e) => assert_eq!(
                e,
                TransactionError::ValidationError(
                    "Value provided is outside the range allowed by the range proof".to_string()
                )
            ),
        }

        let value = 2u64.pow(32) + 1;
        let v = PrivateKey::from(value);
        let c = factories.commitment.commit(&test_params_2.spend_key, &v);
        let proof = factories
            .range_proof
            .construct_proof(&test_params_2.spend_key, 2u64.pow(32) + 1)
            .unwrap();

        let tx_output3 = TransactionOutput::new(
            output_features.clone(),
            c,
            RangeProof::from_bytes(&proof).unwrap(),
            script.clone(),
            test_params_2.sender_offset_public_key,
            TransactionOutput::create_final_metadata_signature(
                &value.into(),
                &test_params_2.spend_key,
                &script,
                &output_features,
                &test_params_2.sender_offset_private_key,
            )
            .unwrap(),
        );
        tx_output3.verify_range_proof(&factories.range_proof).unwrap_err();
    }

    #[test]
    fn sender_signature_verification() {
        let test_params = TestParams::new();
        let factories = CryptoFactories::new(32);
        let unblinded_output = test_params.create_unblinded_output(Default::default());

        let mut tx_output = unblinded_output.as_transaction_output(&factories).unwrap();
        assert!(tx_output.verify_metadata_signature().is_ok());
        tx_output.script = TariScript::default();
        assert!(tx_output.verify_metadata_signature().is_err());

        tx_output = unblinded_output.as_transaction_output(&factories).unwrap();
        assert!(tx_output.verify_metadata_signature().is_ok());
        tx_output.features = OutputFeatures::create_coinbase(0);
        assert!(tx_output.verify_metadata_signature().is_err());

        tx_output = unblinded_output.as_transaction_output(&factories).unwrap();
        assert!(tx_output.verify_metadata_signature().is_ok());
        tx_output.sender_offset_public_key = PublicKey::default();
        assert!(tx_output.verify_metadata_signature().is_err());
    }

    #[test]
    fn kernel_hash() {
        let s = PrivateKey::from_hex("6c6eebc5a9c02e1f3c16a69ba4331f9f63d0718401dea10adc4f9d3b879a2c09").unwrap();
        let r = PublicKey::from_hex("28e8efe4e5576aac931d358d0f6ace43c55fa9d4186d1d259d1436caa876d43b").unwrap();
        let sig = Signature::new(r, s);
        let excess = Commitment::from_hex("9017be5092b85856ce71061cadeb20c2d1fabdf664c4b3f082bf44cf5065e650").unwrap();
        let k = KernelBuilder::new()
            .with_signature(&sig)
            .with_fee(100.into())
            .with_excess(&excess)
            .with_lock_height(500)
            .build()
            .unwrap();
        assert_eq!(
            &k.hash().to_hex(),
            "fe25e4e961d5efec889c489d43e40a1334bf9b4408be4c2e8035a523f231a732"
        );
    }

    #[test]
    fn kernel_metadata() {
        let s = PrivateKey::from_hex("df9a004360b1cf6488d8ff7fb625bc5877f4b013f9b2b20d84932172e605b207").unwrap();
        let r = PublicKey::from_hex("5c6bfaceaa1c83fa4482a816b5f82ca3975cb9b61b6e8be4ee8f01c5f1bee561").unwrap();
        let sig = Signature::new(r, s);
        let excess = Commitment::from_hex("e0bd3f743b566272277c357075b0584fc840d79efac49e9b3b6dbaa8a351bc0c").unwrap();
        let k = KernelBuilder::new()
            .with_signature(&sig)
            .with_fee(100.into())
            .with_excess(&excess)
            .with_lock_height(500)
            .build()
            .unwrap();
        assert_eq!(
            &k.hash().to_hex(),
            "f1e7348b0952d8afbec6bfaa07a1cbc9c45e51e022242d3faeb0f190e2a9dd07"
        )
    }

    #[test]
    fn check_timelocks() {
        let factories = CryptoFactories::new(32);
        let k = BlindingFactor::random(&mut OsRng);
        let v = PrivateKey::from(2u64.pow(32) + 1);
        let c = factories.commitment.commit(&k, &v);

        let script = TariScript::default();
        let input_data = ExecutionStack::default();
        let script_signature = ComSignature::default();
        let offset_pub_key = PublicKey::default();
        let mut input = TransactionInput::new_with_output_data(
            OutputFeatures::with_maturity(5),
            c,
            script,
            input_data,
            script_signature,
            offset_pub_key,
        );

        let mut kernel = test_helpers::create_test_kernel(0.into(), 0);
        let mut tx = Transaction::new(Vec::new(), Vec::new(), Vec::new(), 0.into(), 0.into());

        // lets add time locks
        input.set_maturity(5).unwrap();
        kernel.lock_height = 2;
        tx.body.add_input(input.clone());
        tx.body.add_kernel(kernel.clone());
        assert_eq!(tx.body.check_stxo_rules(1), Err(TransactionError::InputMaturity));
        assert_eq!(tx.body.check_stxo_rules(5), Ok(()));

        assert_eq!(tx.max_input_maturity(), 5);
        assert_eq!(tx.max_kernel_timelock(), 2);
        assert_eq!(tx.min_spendable_height(), 5);

        input.set_maturity(4).unwrap();
        kernel.lock_height = 3;
        tx.body.add_input(input.clone());
        tx.body.add_kernel(kernel.clone());

        assert_eq!(tx.max_input_maturity(), 5);
        assert_eq!(tx.max_kernel_timelock(), 3);
        assert_eq!(tx.min_spendable_height(), 5);

        input.set_maturity(2).unwrap();
        kernel.lock_height = 10;
        tx.body.add_input(input);
        tx.body.add_kernel(kernel);

        assert_eq!(tx.max_input_maturity(), 5);
        assert_eq!(tx.max_kernel_timelock(), 10);
        assert_eq!(tx.min_spendable_height(), 10);
    }

    #[test]
    fn test_validate_internal_consistency() {
        let (tx, _, _) = test_helpers::create_tx(5000.into(), 3.into(), 1, 2, 1, 4);

        let factories = CryptoFactories::default();
        assert!(tx
            .validate_internal_consistency(false, &factories, None, None, None)
            .is_ok());
    }

    #[test]
    #[allow(clippy::identity_op)]
    fn check_cut_through() {
        let (tx, _, outputs) = test_helpers::create_tx(50000000.into(), 3.into(), 1, 2, 1, 2);

        assert_eq!(tx.body.inputs().len(), 2);
        assert_eq!(tx.body.outputs().len(), 2);
        assert_eq!(tx.body.kernels().len(), 1);

        let factories = CryptoFactories::default();
        assert!(tx
            .validate_internal_consistency(false, &factories, None, None, None)
            .is_ok());

        let schema = txn_schema!(from: vec![outputs[1].clone()], to: vec![1 * T, 2 * T]);
        let (tx2, _outputs, _) = test_helpers::spend_utxos(schema);

        assert_eq!(tx2.body.inputs().len(), 1);
        assert_eq!(tx2.body.outputs().len(), 3);
        assert_eq!(tx2.body.kernels().len(), 1);

        let tx3 = tx + tx2;
        let mut tx3_cut_through = tx3.clone();
        // check that all inputs are as we expect them to be
        assert_eq!(tx3.body.inputs().len(), 3);
        assert_eq!(tx3.body.outputs().len(), 5);
        assert_eq!(tx3.body.kernels().len(), 2);

        // Do manual cut-through on tx3
        let double_inputs: Vec<TransactionInput> = tx3_cut_through
            .body
            .inputs()
            .clone()
            .iter()
            .filter(|input| tx3_cut_through.body.outputs_mut().iter().any(|o| o.is_equal_to(input)))
            .cloned()
            .collect();
        for input in double_inputs {
            tx3_cut_through.body.outputs_mut().retain(|x| !input.is_equal_to(x));
            tx3_cut_through.body.inputs_mut().retain(|x| *x != input);
        }

        // Validate basis transaction where cut-through has not been applied.
        assert!(tx3
            .validate_internal_consistency(false, &factories, None, None, Some(u64::MAX))
            .is_ok());

        // tx3_cut_through has manual cut-through, it should not be possible so this should fail
        assert!(tx3_cut_through
            .validate_internal_consistency(false, &factories, None, None, Some(u64::MAX))
            .is_err());
    }

    #[test]
    fn check_duplicate_inputs_outputs() {
        let (tx, _, _outputs) = test_helpers::create_tx(50000000.into(), 3.into(), 1, 2, 1, 2);
        assert!(!tx.body.contains_duplicated_outputs());
        assert!(!tx.body.contains_duplicated_inputs());

        let input = tx.body.inputs()[0].clone();
        let output = tx.body.outputs()[0].clone();

        let mut broken_tx_1 = tx.clone();
        let mut broken_tx_2 = tx;

        broken_tx_1.body.add_input(input);
        broken_tx_2.body.add_output(output);

        assert!(broken_tx_1.body.contains_duplicated_inputs());
        assert!(broken_tx_2.body.contains_duplicated_outputs());
    }

    #[test]
    fn inputs_not_malleable() {
        let (mut inputs, outputs) = test_helpers::create_unblinded_txos(5000.into(), 1, 1, 2, 15.into());
        let mut stack = inputs[0].input_data.clone();
        inputs[0].script = script!(Drop Nop);
        inputs[0].input_data.push(StackItem::Hash([0; 32])).unwrap();
        let mut tx = test_helpers::create_transaction_with(1, 15.into(), inputs, outputs);

        stack
            .push(StackItem::Hash(*b"Pls put this on tha tari network"))
            .unwrap();

        tx.body.inputs_mut()[0].input_data = stack;

        let factories = CryptoFactories::default();
        let err = tx
            .validate_internal_consistency(false, &factories, None, None, Some(u64::MAX))
            .unwrap_err();
        assert!(matches!(err, TransactionError::InvalidSignatureError(_)));
    }

    #[test]
    fn test_output_rewinding() {
        let test_params = TestParams::new();
        let factories = CryptoFactories::new(32);
        let v = MicroTari::from(42);
        let rewind_key = PrivateKey::random(&mut OsRng);
        let rewind_blinding_key = PrivateKey::random(&mut OsRng);
        let random_key = PrivateKey::random(&mut OsRng);
        let rewind_public_key = PublicKey::from_secret_key(&rewind_key);
        let rewind_blinding_public_key = PublicKey::from_secret_key(&rewind_blinding_key);
        let public_random_key = PublicKey::from_secret_key(&random_key);
        let proof_message = b"testing12345678910111";

        let rewind_data = RewindData {
            rewind_key: rewind_key.clone(),
            rewind_blinding_key: rewind_blinding_key.clone(),
            proof_message: proof_message.to_owned(),
        };

        let unblinded_output = test_params.create_unblinded_output(UtxoTestParams {
            value: v,
            ..Default::default()
        });
        let output = unblinded_output
            .as_rewindable_transaction_output(&factories, &rewind_data)
            .unwrap();

        assert_eq!(
            output.rewind_range_proof_value_only(
                &factories.range_proof,
                &public_random_key,
                &rewind_blinding_public_key
            ),
            Err(TransactionError::RangeProofError(RangeProofError::InvalidRewind))
        );
        assert_eq!(
            output.rewind_range_proof_value_only(&factories.range_proof, &rewind_public_key, &public_random_key),
            Err(TransactionError::RangeProofError(RangeProofError::InvalidRewind))
        );

        let rewind_result = output
            .rewind_range_proof_value_only(&factories.range_proof, &rewind_public_key, &rewind_blinding_public_key)
            .unwrap();

        assert_eq!(rewind_result.committed_value, v);
        assert_eq!(&rewind_result.proof_message, proof_message);

        assert_eq!(
            output.full_rewind_range_proof(&factories.range_proof, &random_key, &rewind_blinding_key),
            Err(TransactionError::RangeProofError(RangeProofError::InvalidRewind))
        );
        assert_eq!(
            output.full_rewind_range_proof(&factories.range_proof, &rewind_key, &random_key),
            Err(TransactionError::RangeProofError(RangeProofError::InvalidRewind))
        );

        let full_rewind_result = output
            .full_rewind_range_proof(&factories.range_proof, &rewind_key, &rewind_blinding_key)
            .unwrap();
        assert_eq!(full_rewind_result.committed_value, v);
        assert_eq!(&full_rewind_result.proof_message, proof_message);
        assert_eq!(full_rewind_result.blinding_factor, test_params.spend_key);
    }
    mod output_features {
        use std::io;

        use super::*;
        use crate::consensus::{ConsensusDecoding, ConsensusEncoding, ConsensusEncodingSized};

        #[test]
        fn consensus_encode_minimal() {
            let features = OutputFeatures::with_maturity(0);
            let mut buf = Vec::new();
            let written = features.consensus_encode(&mut buf).unwrap();
            assert_eq!(buf.len(), 3);
            assert_eq!(written, 3);
        }

        #[test]
        fn consensus_encode_decode() {
            let features = OutputFeatures::create_coinbase(u64::MAX);
            let known_size = features.consensus_encode_exact_size();
            let mut buf = Vec::with_capacity(known_size);
            assert_eq!(known_size, 12);
            let written = features.consensus_encode(&mut buf).unwrap();
            assert_eq!(buf.len(), 12);
            assert_eq!(written, 12);
            let decoded_features = OutputFeatures::consensus_decode(&mut &buf[..]).unwrap();
            assert_eq!(features, decoded_features);
        }

        #[test]
        fn consensus_decode_bad_flags() {
            let data = [0x00u8, 0x00, 0x02];
            let features = OutputFeatures::consensus_decode(&mut &data[..]).unwrap();
            // Assert the flag data is preserved
            assert_eq!(features.flags.bits() & 0x02, 0x02);
        }

        #[test]
        fn consensus_decode_bad_maturity() {
            let data = [0x00u8, 0xFF];
            let err = OutputFeatures::consensus_decode(&mut &data[..]).unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
        }

        #[test]
        fn consensus_decode_attempt_maturity_overflow() {
            let data = [0x00u8, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
            let err = OutputFeatures::consensus_decode(&mut &data[..]).unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        }
    }
}
