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

use rand::rngs::OsRng;
use tari_common_types::types::{PrivateKey, Signature};
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    keys::SecretKey as SecretKeyTrait,
    range_proof::RangeProofService,
    tari_utilities::hex::Hex,
};
use tari_p2p::Network;
use tari_script::{inputs, script, ExecutionStack, StackItem};
use tari_test_utils::unpack_enum;

use super::*;
use crate::{
    consensus::ConsensusManager,
    transactions::{
        aggregated_body::AggregateBody,
        key_manager::{
            create_memory_db_key_manager,
            create_memory_db_key_manager_with_range_proof_size,
            TransactionKeyManagerInterface,
        },
        tari_amount::{uT, T},
        test_helpers,
        test_helpers::{TestParams, UtxoTestParams},
        transaction_components::{
            encrypted_data::PaymentId,
            transaction_output::batch_verify_range_proofs,
            OutputFeatures,
        },
        transaction_protocol::TransactionProtocolError,
        CryptoFactories,
    },
    txn_schema,
    validation::{transaction::TransactionInternalConsistencyValidator, ValidationError},
};

#[tokio::test]
async fn input_and_output_and_wallet_output_hash_match() {
    let key_manager = create_memory_db_key_manager().unwrap();
    let test_params = TestParams::new(&key_manager).await;

    let i = test_params
        .create_output(Default::default(), &key_manager)
        .await
        .unwrap();
    let output = i.to_transaction_output(&key_manager).await.unwrap();
    let input = i.to_transaction_input(&key_manager).await.unwrap();
    assert_eq!(output.hash(), input.output_hash());
    assert_eq!(output.hash(), i.hash(&key_manager).await.unwrap());
}

#[test]
fn test_smt_hashes() {
    let input = TransactionInput::default();
    let output = TransactionOutput::default();
    let input_hash = input.smt_hash(10);
    let output_hash = output.smt_hash(10);
    assert_eq!(input_hash, output_hash);
}

#[tokio::test]
async fn key_manager_input() {
    let key_manager = create_memory_db_key_manager().unwrap();
    let test_params = TestParams::new(&key_manager).await;

    let i = test_params
        .create_output(Default::default(), &key_manager)
        .await
        .unwrap();
    let input = i
        .to_transaction_input(&key_manager)
        .await
        .expect("Should be able to create transaction input");

    let output = i
        .to_transaction_output(&key_manager)
        .await
        .expect("Should be able to create transaction output");

    assert_eq!(*input.features().unwrap(), OutputFeatures::default());
    let (_, value, _) = key_manager.try_output_key_recovery(&output, None).await.unwrap();
    assert_eq!(value, i.value);
}

#[tokio::test]
async fn range_proof_verification() {
    let factories = CryptoFactories::new(32);
    let key_manager = create_memory_db_key_manager_with_range_proof_size(32).unwrap();
    // Directly test the tx_output verification
    let test_params_1 = TestParams::new(&key_manager).await;
    let test_params_2 = TestParams::new(&key_manager).await;

    // For testing the max range has been limited to 2^32 so this value is too large.
    let wallet_output1 = test_params_1
        .create_output(
            UtxoTestParams {
                value: (2u64.pow(32) - 1u64).into(),
                ..Default::default()
            },
            &key_manager,
        )
        .await
        .unwrap();
    let tx_output1 = wallet_output1.to_transaction_output(&key_manager).await.unwrap();
    tx_output1.verify_range_proof(&factories.range_proof).unwrap();
    let input_data = inputs!(test_params_2.script_key_pk.clone());
    let wallet_output2 = WalletOutputBuilder::new(
        (2u64.pow(32) + 1u64).into(),
        test_params_2.commitment_mask_key_id.clone(),
    )
    .with_features(OutputFeatures::default())
    .with_script(script![Nop])
    .encrypt_data_for_recovery(&key_manager, None, PaymentId::Empty)
    .await
    .unwrap()
    .with_input_data(input_data)
    .with_covenant(Covenant::default())
    .with_version(TransactionOutputVersion::get_current_version())
    .with_sender_offset_public_key(test_params_2.sender_offset_key_pk.clone())
    .with_script_key(test_params_2.script_key_id.clone())
    .sign_as_sender_and_receiver(&key_manager, &test_params_2.sender_offset_key_id)
    .await
    .unwrap()
    .try_build(&key_manager)
    .await;

    match wallet_output2 {
        Ok(_) => panic!("Range proof should have failed to verify"),
        Err(e) => {
            unpack_enum!(TransactionError::BuilderError(s) = e);
            assert_eq!(s, "Value provided is outside the range allowed by the range proof");
        },
    }
    let key = PrivateKey::random(&mut OsRng);

    // Test that proofs with values encroaching on the bit length cannot be constructed
    if factories.range_proof.construct_proof(&key, 2u64.pow(32) - 1).is_err() {
        panic!("Range proof construction should have succeeded")
    };
    if factories.range_proof.construct_proof(&key, 2u64.pow(32)).is_ok() {
        panic!("Range proof construction should have failed")
    };
    if factories.range_proof.construct_proof(&key, 2u64.pow(32) + 1).is_ok() {
        panic!("Range proof construction should have failed")
    };
}

#[tokio::test]
async fn range_proof_verification_batch() {
    let factories = CryptoFactories::new(64);
    let key_manager = create_memory_db_key_manager().unwrap();
    let wallet_output1 = TestParams::new(&key_manager)
        .await
        .create_output(
            UtxoTestParams {
                value: (1u64).into(),
                ..Default::default()
            },
            &key_manager,
        )
        .await
        .unwrap();
    let tx_output1 = wallet_output1.to_transaction_output(&key_manager).await.unwrap();
    assert!(tx_output1.verify_range_proof(&factories.range_proof).is_ok());

    let wallet_output2 = TestParams::new(&key_manager)
        .await
        .create_output(
            UtxoTestParams {
                value: (2u64).into(),
                ..Default::default()
            },
            &key_manager,
        )
        .await
        .unwrap();
    let tx_output2 = wallet_output2.to_transaction_output(&key_manager).await.unwrap();
    assert!(tx_output2.verify_range_proof(&factories.range_proof).is_ok());

    let wallet_output3 = TestParams::new(&key_manager)
        .await
        .create_output(
            UtxoTestParams {
                value: (3u64).into(),
                ..Default::default()
            },
            &key_manager,
        )
        .await
        .unwrap();
    let tx_output3 = wallet_output3.to_transaction_output(&key_manager).await.unwrap();
    assert!(tx_output3.verify_range_proof(&factories.range_proof).is_ok());

    let wallet_output4 = TestParams::new(&key_manager)
        .await
        .create_output(
            UtxoTestParams {
                value: (4u64).into(),
                ..Default::default()
            },
            &key_manager,
        )
        .await
        .unwrap();
    let tx_output4 = wallet_output4.to_transaction_output(&key_manager).await.unwrap();
    assert!(tx_output4.verify_range_proof(&factories.range_proof).is_ok());

    let wallet_output5 = TestParams::new(&key_manager)
        .await
        .create_output(
            UtxoTestParams {
                value: (5u64).into(),
                ..Default::default()
            },
            &key_manager,
        )
        .await
        .unwrap();
    let mut tx_output5 = wallet_output5.to_transaction_output(&key_manager).await.unwrap();
    assert!(tx_output5.verify_range_proof(&factories.range_proof).is_ok());

    // The batch should pass
    let outputs = vec![
        tx_output1.clone(),
        tx_output2.clone(),
        tx_output3.clone(),
        tx_output4.clone(),
        tx_output5.clone(),
    ];
    let outputs = outputs.iter().collect::<Vec<_>>();
    assert!(batch_verify_range_proofs(&factories.range_proof, &outputs).is_ok());

    // The batch should fail after tampering with a single proof
    tx_output5.proof = tx_output4.proof.clone();
    let outputs = vec![tx_output1, tx_output2, tx_output3, tx_output4, tx_output5];
    let outputs = outputs.iter().collect::<Vec<_>>();
    assert!(batch_verify_range_proofs(&factories.range_proof, &outputs).is_err());
}

#[tokio::test]
async fn sender_signature_verification() {
    let key_manager = create_memory_db_key_manager().unwrap();
    let test_params = TestParams::new(&key_manager).await;
    let wallet_output = test_params
        .create_output(Default::default(), &key_manager)
        .await
        .unwrap();

    let mut tx_output = wallet_output.to_transaction_output(&key_manager).await.unwrap();
    assert!(tx_output.verify_metadata_signature().is_ok());
    tx_output.script = TariScript::default();
    assert!(tx_output.verify_metadata_signature().is_err());

    tx_output = wallet_output.to_transaction_output(&key_manager).await.unwrap();
    assert!(tx_output.verify_metadata_signature().is_ok());
    tx_output.features = OutputFeatures::create_coinbase(0, None, RangeProofType::BulletProofPlus);
    assert!(tx_output.verify_metadata_signature().is_err());

    tx_output = wallet_output.to_transaction_output(&key_manager).await.unwrap();
    assert!(tx_output.verify_metadata_signature().is_ok());
    tx_output.sender_offset_public_key = PublicKey::default();
    assert!(tx_output.verify_metadata_signature().is_err());
}

#[test]
fn kernel_hash() {
    #[cfg(tari_target_network_mainnet)]
    if let Network::MainNet = Network::get_current_or_user_setting_or_default() {
        eprintln!("This test is configured for stagenet only");
        return;
    }
    let s = PrivateKey::from_hex("6c6eebc5a9c02e1f3c16a69ba4331f9f63d0718401dea10adc4f9d3b879a2c09").unwrap();
    let r = PublicKey::from_hex("28e8efe4e5576aac931d358d0f6ace43c55fa9d4186d1d259d1436caa876d43b").unwrap();
    let sig = Signature::new(r, s);
    let excess = Commitment::from_hex("9017be5092b85856ce71061cadeb20c2d1fabdf664c4b3f082bf44cf5065e650").unwrap();
    let k = KernelBuilder::new()
        .with_signature(sig)
        .with_fee(100.into())
        .with_excess(&excess)
        .with_lock_height(500)
        .build()
        .unwrap();
    #[cfg(tari_target_network_nextnet)]
    assert_eq!(
        &k.hash().to_hex(),
        "c1f6174935d08358809fcf244a9a1edb078b74a1ae18ab4c7dd501b0294a2a94"
    );
    #[cfg(tari_target_network_mainnet)]
    assert_eq!(
        &k.hash().to_hex(),
        "b94992cb59695ebad3786e9f51a220e91c627f8b38f51bcf6c87297325d1b410"
    );
    #[cfg(tari_target_network_testnet)]
    assert_eq!(
        &k.hash().to_hex(),
        "38b03d013f941e86c027969fbbc190ca2a28fa2d7ac075d50dbfb6232deee646"
    );
}

#[test]
fn kernel_metadata() {
    let s = PrivateKey::from_hex("df9a004360b1cf6488d8ff7fb625bc5877f4b013f9b2b20d84932172e605b207").unwrap();
    let r = PublicKey::from_hex("5c6bfaceaa1c83fa4482a816b5f82ca3975cb9b61b6e8be4ee8f01c5f1bee561").unwrap();
    let sig = Signature::new(r, s);
    let excess = Commitment::from_hex("e0bd3f743b566272277c357075b0584fc840d79efac49e9b3b6dbaa8a351bc0c").unwrap();
    let k = KernelBuilder::new()
        .with_signature(sig)
        .with_fee(100.into())
        .with_excess(&excess)
        .with_lock_height(500)
        .build()
        .unwrap();
    #[cfg(tari_target_network_mainnet)]
    match Network::get_current_or_user_setting_or_default() {
        Network::MainNet => {
            eprintln!("This test is configured for stagenet only");
        },
        Network::StageNet => assert_eq!(
            &k.hash().to_hex(),
            "75a357c2769098b19a6aedc7e46f6be305f4f1a1831556cd380b0b0f20bfdf12"
        ),
        n => panic!("Only mainnet networks should target mainnet. Network was {}", n),
    }

    #[cfg(tari_target_network_nextnet)]
    assert_eq!(
        &k.hash().to_hex(),
        "22e39392dfeae9653c73437880be71e99f4b8a2b23289d54f57b8931deebfeed"
    );
    #[cfg(tari_target_network_testnet)]
    assert_eq!(
        &k.hash().to_hex(),
        "ebc852fbac798c25ce497b416f69ec11a97e186aacaa10e2bb4ca5f5a0f197f2"
    )
}

#[test]
fn check_timelocks() {
    let factories = CryptoFactories::new(32);
    let k = PrivateKey::random(&mut OsRng);
    let v = PrivateKey::from(2u64.pow(32) + 1);
    let c = factories.commitment.commit(&k, &v);

    let script = TariScript::default();
    let input_data = ExecutionStack::default();
    let script_signature = ComAndPubSignature::default();
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
        EncryptedData::default(),
        Default::default(),
        Default::default(),
        MicroMinotari::zero(),
    );

    let mut kernel = test_helpers::create_test_kernel(0.into(), 0, KernelFeatures::empty());
    let mut tx = Transaction::new(Vec::new(), Vec::new(), Vec::new(), 0.into(), 0.into());

    // lets add time locks
    input.set_maturity(5).unwrap();
    kernel.lock_height = 2;
    tx.body.add_input(input.clone());
    tx.body.add_kernel(kernel.clone());
    assert!(matches!(
        tx.body.check_utxo_rules(1),
        Err(TransactionError::InputMaturity)
    ));
    tx.body.check_utxo_rules(5).unwrap();

    assert_eq!(tx.max_input_maturity().unwrap(), 5);
    assert_eq!(tx.max_kernel_timelock(), 2);
    assert_eq!(tx.min_spendable_height().unwrap(), 5);

    input.set_maturity(4).unwrap();
    kernel.lock_height = 3;
    tx.body.add_input(input.clone());
    tx.body.add_kernel(kernel.clone());

    assert_eq!(tx.max_input_maturity().unwrap(), 5);
    assert_eq!(tx.max_kernel_timelock(), 3);
    assert_eq!(tx.min_spendable_height().unwrap(), 5);

    input.set_maturity(2).unwrap();
    kernel.lock_height = 10;
    tx.body.add_input(input);
    tx.body.add_kernel(kernel);

    assert_eq!(tx.max_input_maturity().unwrap(), 5);
    assert_eq!(tx.max_kernel_timelock(), 10);
    assert_eq!(tx.min_spendable_height().unwrap(), 10);
}

#[tokio::test]
async fn test_validate_internal_consistency() {
    let features = OutputFeatures { ..Default::default() };
    let key_manager = create_memory_db_key_manager().unwrap();
    let (tx, _, _) = test_helpers::create_tx(5000.into(), 3.into(), 1, 2, 1, 4, features, &key_manager)
        .await
        .expect("Failed to create tx");
    let rules = ConsensusManager::builder(Network::LocalNet).build().unwrap();
    let factories = CryptoFactories::default();
    let validator = TransactionInternalConsistencyValidator::new(false, rules, factories);
    assert!(validator.validate(&tx, None, None, u64::MAX).is_ok());
}

#[tokio::test]
#[allow(clippy::identity_op)]
async fn check_cut_through() {
    let key_manager = create_memory_db_key_manager().unwrap();
    let (tx, _, outputs) =
        test_helpers::create_tx(50000000.into(), 3.into(), 1, 2, 1, 2, Default::default(), &key_manager)
            .await
            .expect("Failed to create tx");

    assert_eq!(tx.body.inputs().len(), 2);
    assert_eq!(tx.body.outputs().len(), 2);
    assert_eq!(tx.body.kernels().len(), 1);

    let rules = ConsensusManager::builder(Network::LocalNet).build().unwrap();
    let factories = CryptoFactories::default();
    let validator = TransactionInternalConsistencyValidator::new(false, rules, factories);
    validator.validate(&tx, None, None, u64::MAX).unwrap();

    let schema = txn_schema!(from: vec![outputs[1].clone()], to: vec![1 * T, 2 * T]);
    let (tx2, _outputs) = test_helpers::spend_utxos(schema, &key_manager).await;

    assert_eq!(tx2.body.inputs().len(), 1);
    assert_eq!(tx2.body.outputs().len(), 3);
    assert_eq!(tx2.body.kernels().len(), 1);

    let mut tx3 = tx + tx2;
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
        .filter(|input| tx3_cut_through.body.outputs().iter().any(|o| o.is_equal_to(input)))
        .cloned()
        .collect();
    let mut outputs = tx3_cut_through.body.outputs().clone();
    let mut inputs = tx3_cut_through.body.inputs().clone();
    for input in double_inputs {
        outputs.retain(|x| !input.is_equal_to(x));
        inputs.retain(|x| *x != input);
    }
    tx3_cut_through.body = AggregateBody::new(inputs, outputs, tx3_cut_through.body.kernels().clone());
    tx3.body.sort();
    tx3_cut_through.body.sort();

    // Validate basis transaction where cut-through has not been applied.
    validator.validate(&tx3, None, None, u64::MAX).unwrap();

    // tx3_cut_through has manual cut-through, it should not be possible so this should fail
    validator.validate(&tx3_cut_through, None, None, u64::MAX).unwrap_err();
}

#[tokio::test]
async fn check_duplicate_inputs_outputs() {
    let key_manager = create_memory_db_key_manager().unwrap();
    let (tx, _, _outputs) =
        test_helpers::create_tx(50000000.into(), 3.into(), 1, 2, 1, 2, Default::default(), &key_manager)
            .await
            .expect("Failed to create tx");
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

#[tokio::test]
async fn inputs_not_malleable() {
    let key_manager = create_memory_db_key_manager().unwrap();
    let (inputs, outputs) = test_helpers::create_wallet_outputs(
        5000.into(),
        1,
        1,
        2,
        15.into(),
        &Default::default(),
        &script![Nop],
        &Default::default(),
        &key_manager,
    )
    .await
    .expect("Failed to create wallet outputs");
    let mut stack = inputs[0].input_data.clone();
    let mut tx = test_helpers::create_transaction_with(1, 15.into(), inputs, outputs, &key_manager).await;

    stack
        .push(StackItem::Hash(*b"Pls put this on tha tari network"))
        .unwrap();

    let mut inputs = tx.body().inputs().clone();
    inputs[0].set_script(script![Drop]).unwrap();
    inputs[0].input_data = stack;
    tx.body = AggregateBody::new(inputs, tx.body.outputs().clone(), tx.body().kernels().clone());

    let rules = ConsensusManager::builder(Network::LocalNet).build().unwrap();
    let factories = CryptoFactories::default();
    let validator = TransactionInternalConsistencyValidator::new(false, rules, factories);
    let err = validator.validate(&tx, None, None, u64::MAX).unwrap_err();
    unpack_enum!(ValidationError::TransactionError(_a) = err);
}

#[tokio::test]
async fn test_output_recover_openings() {
    let key_manager = create_memory_db_key_manager().unwrap();
    let test_params = TestParams::new(&key_manager).await;
    let v = MicroMinotari::from(42);

    let wallet_output = test_params
        .create_output(
            UtxoTestParams {
                value: v,
                ..Default::default()
            },
            &key_manager,
        )
        .await
        .unwrap();
    let output = wallet_output.to_transaction_output(&key_manager).await.unwrap();

    let (mask, value, _) = key_manager.try_output_key_recovery(&output, None).await.unwrap();
    assert_eq!(value, wallet_output.value);
    assert_eq!(mask, test_params.commitment_mask_key_id);
}

mod validate_internal_consistency {

    use digest::Digest;
    use tari_crypto::hashing::DomainSeparation;

    use super::*;
    use crate::{
        covenants::{BaseLayerCovenantsDomain, COVENANTS_FIELD_HASHER_LABEL},
        transactions::{
            key_manager::MemoryDbKeyManager,
            test_helpers::{create_transaction_with, create_wallet_outputs},
        },
    };

    async fn test_case(
        input_params: &UtxoTestParams,
        utxo_params: &UtxoTestParams,
        height: u64,
        key_manager: &MemoryDbKeyManager,
    ) -> Result<(), TransactionProtocolError> {
        let (mut inputs, outputs) = create_wallet_outputs(
            100 * T,
            1,
            0,
            1,
            5 * uT,
            &utxo_params.features.clone(),
            &utxo_params.script.clone(),
            &utxo_params.covenant.clone(),
            key_manager,
        )
        .await
        .expect("Failed to create wallet outputs");
        inputs[0].features = input_params.features.clone();
        inputs[0].covenant = input_params.covenant.clone();
        inputs[0].script = input_params.script.clone();
        // SenderTransactionProtocol::finalize() calls validate_internal_consistency
        let tx = create_transaction_with(0, 5 * uT, inputs, outputs, key_manager).await;
        // Otherwise if this passes check again with the height
        let rules = ConsensusManager::builder(Network::LocalNet).build().unwrap();
        let validator = TransactionInternalConsistencyValidator::new(false, rules, CryptoFactories::default());
        validator
            .validate(&tx, None, None, height)
            .map_err(|err| TransactionError::BuilderError(err.to_string()))?;
        Ok(())
    }

    #[tokio::test]
    async fn it_validates_that_the_covenant_is_honoured() {
        //---------------------------------- Case1 - PASS --------------------------------------------//
        let covenant = covenant!(fields_preserved(@fields( @field::covenant)));
        let features = OutputFeatures { ..Default::default() };
        let key_manager = create_memory_db_key_manager().unwrap();
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
            &key_manager,
        )
        .await
        .unwrap();

        //---------------------------------- Case2 - PASS --------------------------------------------//
        let mut hasher = Blake2b::<U32>::default();
        BaseLayerCovenantsDomain::add_domain_separation_tag(&mut hasher, COVENANTS_FIELD_HASHER_LABEL);

        let hash = hasher
            .chain_update(borsh::to_vec(&features).unwrap())
            .finalize()
            .to_vec();

        let mut slice = [0u8; FixedHash::byte_size()];
        slice.copy_from_slice(hash.as_ref());
        let hash = FixedHash::from(slice);

        let covenant = covenant!(fields_hashed_eq(@fields(@field::features), @hash(hash)));

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
            &key_manager,
        )
        .await
        .unwrap();

        //---------------------------------- Case3 - FAIL --------------------------------------------//
        let covenant = covenant!(or(absolute_height(@uint(100),), field_eq(@field::features_maturity, @uint(42))));

        let err = test_case(
            &UtxoTestParams {
                covenant: covenant.clone(),
                ..Default::default()
            },
            &UtxoTestParams::default(),
            0,
            &key_manager,
        )
        .await
        .unwrap_err();

        unpack_enum!(TransactionProtocolError::TransactionBuildError(err) = err);
        unpack_enum!(TransactionError::BuilderError(_s) = err);

        //---------------------------------- Case4 - PASS --------------------------------------------//
        // Pass because maturity is set
        test_case(
            &UtxoTestParams {
                covenant: covenant.clone(),
                ..Default::default()
            },
            &UtxoTestParams {
                features: OutputFeatures {
                    maturity: 42,
                    ..Default::default()
                },
                ..Default::default()
            },
            0,
            &key_manager,
        )
        .await
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
            &key_manager,
        )
        .await
        .unwrap();
    }
}
