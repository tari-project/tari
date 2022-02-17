//  Copyright 2022, The Tari Project
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

use digest::Digest;
use rand::{self, rngs::OsRng};
use tari_common_types::types::{BlindingFactor, ComSignature, PrivateKey, PublicKey, RangeProof, Signature};
use tari_comms::types::Challenge;
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    keys::{PublicKey as PublicKeyTrait, SecretKey as SecretKeyTrait},
    range_proof::{RangeProofError, RangeProofService},
    ristretto::pedersen::PedersenCommitmentFactory,
    script,
    script::{ExecutionStack, StackItem},
    tari_utilities::{hex::Hex, Hashable},
};
use tari_test_utils::unpack_enum;
use tari_utilities::ByteArray;

use super::*;
use crate::{
    transactions::{
        tari_amount::{uT, MicroTari, T},
        test_helpers,
        test_helpers::{create_sender_transaction_protocol_with, create_unblinded_txos, TestParams, UtxoTestParams},
        transaction_components::OutputFeatures,
        transaction_protocol::{RewindData, TransactionProtocolError},
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
    assert_eq!(*input.features().unwrap(), OutputFeatures::default());
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
        Err(e) => {
            unpack_enum!(TransactionError::ValidationError(s) = e);
            assert_eq!(s, "Value provided is outside the range allowed by the range proof");
        },
    }

    let value = 2u64.pow(32) + 1;
    let v = PrivateKey::from(value);
    let c = factories.commitment.commit(&test_params_2.spend_key, &v);
    let proof = factories
        .range_proof
        .construct_proof(&test_params_2.spend_key, 2u64.pow(32) + 1)
        .unwrap();

    let tx_output3 = TransactionOutput::new_current_version(
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
            &Covenant::default(),
        )
        .unwrap(),
        Covenant::default(),
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
        "ce54718b33405e8fc96ed68044af21febc84c7a74c2aa9d792947f2571c7a61b"
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
        "db1522441628687beb21d4d8279e107e733aec9c8b7d513ef3c35b05c1e0150c"
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
        TransactionInputVersion::get_current_version(),
        OutputFeatures::default(),
        c,
        script,
        input_data,
        script_signature,
        offset_pub_key,
        Covenant::default(),
    );

    let mut kernel = test_helpers::create_test_kernel(0.into(), 0);
    let mut tx = Transaction::new(Vec::new(), Vec::new(), Vec::new(), 0.into(), 0.into());

    // lets add time locks
    input.set_maturity(5).unwrap();
    kernel.lock_height = 2;
    tx.body.add_input(input.clone());
    tx.body.add_kernel(kernel.clone());
    assert!(matches!(
        tx.body.check_stxo_rules(1),
        Err(TransactionError::InputMaturity)
    ));
    tx.body.check_stxo_rules(5).unwrap();

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
    let features = OutputFeatures {
        unique_id: Some(b"abc".to_vec()),
        ..Default::default()
    };
    let (tx, _, _) = test_helpers::create_tx(5000.into(), 3.into(), 1, 2, 1, 4, features);

    let factories = CryptoFactories::default();
    assert!(tx
        .validate_internal_consistency(false, &factories, None, None, u64::MAX)
        .is_ok());
}

#[test]
#[allow(clippy::identity_op)]
fn check_cut_through() {
    let (tx, _, outputs) = test_helpers::create_tx(50000000.into(), 3.into(), 1, 2, 1, 2, Default::default());

    assert_eq!(tx.body.inputs().len(), 2);
    assert_eq!(tx.body.outputs().len(), 2);
    assert_eq!(tx.body.kernels().len(), 1);

    let factories = CryptoFactories::default();
    tx.validate_internal_consistency(false, &factories, None, None, u64::MAX)
        .unwrap();

    let schema = txn_schema!(from: vec![outputs[1].clone()], to: vec![1 * T, 2 * T]);
    let (tx2, _outputs) = test_helpers::spend_utxos(schema);

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
    tx3.validate_internal_consistency(false, &factories, None, None, u64::MAX)
        .unwrap();

    // tx3_cut_through has manual cut-through, it should not be possible so this should fail
    tx3_cut_through
        .validate_internal_consistency(false, &factories, None, None, u64::MAX)
        .unwrap_err();
}

#[test]
fn check_duplicate_inputs_outputs() {
    let (tx, _, _outputs) = test_helpers::create_tx(50000000.into(), 3.into(), 1, 2, 1, 2, Default::default());
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
#[ignore = "TODO: fix script error"]
fn inputs_not_malleable() {
    let (inputs, outputs) = test_helpers::create_unblinded_txos(
        5000.into(),
        1,
        1,
        2,
        15.into(),
        Default::default(),
        script![Drop],
        Default::default(),
    );
    let script_pk = PublicKey::from_secret_key(&outputs[0].0.script_private_key);
    let mut stack = inputs[0].input_data.clone();
    let mut tx = test_helpers::create_transaction_with(1, 15.into(), inputs, outputs);

    stack
        .push(StackItem::Hash(*b"Pls put this on tha tari network"))
        .unwrap();
    stack.push(StackItem::PublicKey(script_pk)).unwrap();

    tx.body.inputs_mut()[0].input_data = stack;

    let factories = CryptoFactories::default();
    let err = tx
        .validate_internal_consistency(false, &factories, None, None, u64::MAX)
        .unwrap_err();
    unpack_enum!(TransactionError::InvalidSignatureError(_a) = err);
    // assert!(matches!(err, TransactionError::InvalidSignatureError(_)));
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
        .as_rewindable_transaction_output(&factories, &rewind_data, None)
        .unwrap();

    assert!(matches!(
        output.rewind_range_proof_value_only(&factories.range_proof, &public_random_key, &rewind_blinding_public_key),
        Err(TransactionError::RangeProofError(RangeProofError::InvalidRewind))
    ));
    assert!(matches!(
        output.rewind_range_proof_value_only(&factories.range_proof, &rewind_public_key, &public_random_key),
        Err(TransactionError::RangeProofError(RangeProofError::InvalidRewind))
    ));

    let rewind_result = output
        .rewind_range_proof_value_only(&factories.range_proof, &rewind_public_key, &rewind_blinding_public_key)
        .unwrap();

    assert_eq!(rewind_result.committed_value, v);
    assert_eq!(&rewind_result.proof_message, proof_message);

    assert!(matches!(
        output.full_rewind_range_proof(&factories.range_proof, &random_key, &rewind_blinding_key),
        Err(TransactionError::RangeProofError(RangeProofError::InvalidRewind))
    ));
    assert!(matches!(
        output.full_rewind_range_proof(&factories.range_proof, &rewind_key, &random_key),
        Err(TransactionError::RangeProofError(RangeProofError::InvalidRewind))
    ));

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
    #[allow(clippy::field_reassign_with_default)]
    fn consensus_encode_minimal() {
        let mut features = OutputFeatures::default();
        features.version = OutputFeaturesVersion::V0;
        let mut buf = Vec::new();
        let written = features.consensus_encode(&mut buf).unwrap();
        assert_eq!(buf.len(), 9);
        assert_eq!(written, 9);

        let mut features = OutputFeatures::default();
        features.version = OutputFeaturesVersion::V1;
        let mut buf = Vec::new();
        let written = features.consensus_encode(&mut buf).unwrap();
        assert_eq!(buf.len(), 10);
        assert_eq!(written, 10);
    }

    #[test]
    fn consensus_encode_decode() {
        let mut features = OutputFeatures::create_coinbase(u64::MAX);
        features.version = OutputFeaturesVersion::V0;
        let known_size = features.consensus_encode_exact_size();
        let mut buf = Vec::with_capacity(known_size);
        assert_eq!(known_size, 18);
        let written = features.consensus_encode(&mut buf).unwrap();
        assert_eq!(buf.len(), 18);
        assert_eq!(written, 18);
        let decoded_features = OutputFeatures::consensus_decode(&mut &buf[..]).unwrap();
        assert_eq!(features, decoded_features);

        let mut features = OutputFeatures::create_coinbase(u64::MAX);
        features.version = OutputFeaturesVersion::V1;
        let known_size = features.consensus_encode_exact_size();
        let mut buf = Vec::with_capacity(known_size);
        assert_eq!(known_size, 19);
        let written = features.consensus_encode(&mut buf).unwrap();
        assert_eq!(buf.len(), 19);
        assert_eq!(written, 19);
        let decoded_features = OutputFeatures::consensus_decode(&mut &buf[..]).unwrap();
        assert_eq!(features, decoded_features);
    }

    #[test]
    fn consensus_decode_bad_flags() {
        let data = [0x00u8, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
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

mod validate_internal_consistency {
    use super::*;
    use crate::consensus::ToConsensusBytes;

    fn test_case(
        input_params: &UtxoTestParams,
        utxo_params: &UtxoTestParams,
        height: u64,
    ) -> Result<(), TransactionProtocolError> {
        let (mut inputs, outputs) = create_unblinded_txos(
            100 * T,
            1,
            0,
            1,
            5 * uT,
            utxo_params.features.clone(),
            utxo_params.script.clone(),
            utxo_params.covenant.clone(),
        );
        inputs[0].features = input_params.features.clone();
        inputs[0].covenant = input_params.covenant.clone();
        inputs[0].script = input_params.script.clone();
        // SenderTransactionProtocol::finalize() calls validate_internal_consistency
        let stx_protocol = create_sender_transaction_protocol_with(0, 5 * uT, inputs, outputs)?;
        // Otherwise if this passes check again with the height
        let tx = stx_protocol.take_transaction().unwrap();
        tx.validate_internal_consistency(false, &CryptoFactories::default(), None, None, height)?;
        Ok(())
    }

    #[test]
    fn it_validates_that_the_covenant_is_honoured() {
        //---------------------------------- Case1 - PASS --------------------------------------------//
        let covenant = covenant!(fields_preserved(@fields(@field::features_unique_id, @field::covenant)));
        let unique_id = b"dank-meme-nft".to_vec();
        let mut features = OutputFeatures {
            unique_id: Some(unique_id.clone()),
            ..Default::default()
        };
        test_case(
            &UtxoTestParams {
                features: features.clone(),
                covenant: covenant.clone(),
                ..Default::default()
            },
            &UtxoTestParams {
                features: features.clone(),
                covenant,
                ..Default::default()
            },
            0,
        )
        .unwrap();

        //---------------------------------- Case2 - PASS --------------------------------------------//
        features.parent_public_key = Some(PublicKey::default());
        let hash = Challenge::new()
            .chain(Some(PublicKey::default()).to_consensus_bytes())
            .chain(Some(unique_id.clone()).to_consensus_bytes())
            .finalize();

        let covenant = covenant!(fields_hashed_eq(@fields(@field::features_parent_public_key, @field::features_unique_id), @hash(hash.into())));

        test_case(
            &UtxoTestParams {
                covenant,
                ..Default::default()
            },
            &UtxoTestParams {
                features,
                ..Default::default()
            },
            0,
        )
        .unwrap();

        //---------------------------------- Case3 - FAIL --------------------------------------------//
        let covenant = covenant!(or(absolute_height(@uint(100),), field_eq(@field::features_unique_id, @bytes(unique_id.clone()))));

        let err = test_case(
            &UtxoTestParams {
                covenant: covenant.clone(),
                ..Default::default()
            },
            &UtxoTestParams::default(),
            0,
        )
        .unwrap_err();

        unpack_enum!(TransactionProtocolError::TransactionBuildError(err) = err);
        unpack_enum!(TransactionError::CovenantError(_s) = err);

        //---------------------------------- Case4 - PASS --------------------------------------------//
        // Pass because unique_id is set
        test_case(
            &UtxoTestParams {
                covenant: covenant.clone(),
                ..Default::default()
            },
            &UtxoTestParams {
                features: OutputFeatures {
                    unique_id: Some(unique_id),
                    ..Default::default()
                },
                ..Default::default()
            },
            0,
        )
        .unwrap();

        //---------------------------------- Case5 - PASS --------------------------------------------//
        // Pass because height == 100
        test_case(
            &UtxoTestParams {
                covenant,
                ..Default::default()
            },
            &UtxoTestParams::default(),
            100,
        )
        .unwrap();
    }
}
