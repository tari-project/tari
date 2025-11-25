// Copyright 2025. The Tari Project
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
pub mod marshal_output_pair;
pub mod models;
pub mod offline_signer;
pub mod one_sided_signer;

pub use models::PaymentRecipient;
pub use offline_signer::{
    prepare_deposit_multisig_transaction,
    prepare_one_sided_transaction_for_signing,
    prepare_withdraw_multisig_transaction,
    sign_locked_deposit_multisig_transaction,
    sign_locked_transaction,
    sign_locked_withdraw_multisig_transaction,
};

#[cfg(test)]
mod test {
    #![allow(clippy::indexing_slicing)]
    use argon2::password_hash::rand_core::OsRng;
    use rand::RngCore;
    use tari_common::configuration::Network;
    use tari_common_types::{
        tari_address::{TariAddress, TariAddressFeatures},
        transaction::TxId,
    };
    use tari_script::{
        push_pubkey_script,
        CompressedCheckSigSchnorrSignature,
        ExecutionStack,
        Opcode,
        StackItem,
        TariScript,
    };

    use crate::{
        crypto_factories::CryptoFactories,
        fee::Fee,
        helpers::borsh::SerializedSize,
        key_manager::{
            error::KeyManagerError,
            wallet_types::{ViewWallet, WalletType},
            KeyManager,
            SerializedKeyString,
            TariKeyId,
            TransactionKeyManagerInterface,
        },
        multisig::script::derive_multisig_ephemeral_pubkeys,
        offline_signing::{
            offline_signer::sign_locked_transaction,
            prepare_deposit_multisig_transaction,
            prepare_one_sided_transaction_for_signing,
            prepare_withdraw_multisig_transaction,
            sign_locked_deposit_multisig_transaction,
            sign_locked_withdraw_multisig_transaction,
            PaymentRecipient,
        },
        test_helpers::{create_consensus_manager, create_test_input},
        transaction_components::{
            covenants::Covenant,
            one_sided::public_key_to_output_encryption_key,
            EncryptedData,
            MemoField,
            OutputFeatures,
            WalletOutputBuilder,
        },
        validation::transaction::TransactionInternalConsistencyValidator,
        MicroMinotari,
        TransactionBuilder,
    };

    fn create_view_key_manager(view_wallet: ViewWallet) -> Result<KeyManager, KeyManagerError> {
        let wallet = WalletType::ViewWallet(view_wallet);
        KeyManager::new(wallet)
    }

    #[tokio::test]
    async fn offline_sign_is_valid() {
        let rules = create_consensus_manager();
        let alice_key_manager = KeyManager::new_random().unwrap();
        let alice_keys = ViewWallet::new(
            alice_key_manager.get_spend_key().pub_key,
            alice_key_manager.get_private_view_key(),
            None,
        );
        let alice_view_key_manager = create_view_key_manager(alice_keys).unwrap();
        let bob_key_manager = KeyManager::new_random().unwrap();

        let input = create_test_input(MicroMinotari(10000), 0, &alice_view_key_manager, vec![], None);
        let input2 = create_test_input(MicroMinotari(2000), 0, &alice_view_key_manager, vec![], None);
        let input3 = create_test_input(MicroMinotari(15000), 0, &alice_view_key_manager, vec![], None);
        // this replicates the behaviour od the oms that selects the inputs and starts the build tx process.
        let mut tx_builder = TransactionBuilder::new(
            rules.consensus_constants(0).clone(),
            alice_view_key_manager.clone(),
            Network::LocalNet,
        )
        .unwrap();
        tx_builder
            .with_lock_height(0)
            .with_fee_per_gram(MicroMinotari(20))
            .with_input(input)
            .unwrap()
            .with_input(input2)
            .unwrap()
            .with_input(input3)
            .unwrap();

        // now we start the offline process
        let payment_id = MemoField::new_empty();
        let output_features = OutputFeatures::default();
        let amount = MicroMinotari(5000);

        let spend_key = bob_key_manager.get_spend_key().pub_key;
        let view_key = bob_key_manager.get_view_key().pub_key;
        let bob_address = TariAddress::new_dual_address(
            view_key,
            spend_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();
        let spend_key = alice_view_key_manager.get_spend_key().pub_key;
        let view_key = alice_view_key_manager.get_view_key().pub_key;
        let alice_address = TariAddress::new_dual_address(
            view_key,
            spend_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();

        let spend_key = alice_key_manager.get_spend_key().pub_key;
        let view_key = alice_key_manager.get_view_key().pub_key;
        let alice_address_s = TariAddress::new_dual_address(
            view_key,
            spend_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();

        assert_eq!(alice_address, alice_address_s);
        let recipients = [PaymentRecipient {
            amount,
            output_features: output_features.clone(),
            address: bob_address.clone(),
            payment_id: payment_id.clone(),
        }];

        let init = prepare_one_sided_transaction_for_signing(
            TxId::new_random(),
            tx_builder,
            &recipients,
            payment_id,
            alice_address,
        )
        .unwrap();

        assert_eq!(init.info.fee, MicroMinotari(0));
        assert_eq!(init.info.fee_per_gram, MicroMinotari(20));
        assert_eq!(init.info.inputs.len(), 3);
        assert_eq!(init.info.outputs.len(), 0);

        let signed = sign_locked_transaction(
            &alice_key_manager,
            rules.consensus_constants(0).clone(),
            Network::LocalNet,
            init,
        )
        .unwrap();
        assert!(signed.signed_transaction.change_output.is_some());
        assert_eq!(
            signed.signed_transaction.transaction.body.kernels()[0].fee,
            MicroMinotari(2960)
        );
        assert_eq!(signed.signed_transaction.transaction.body.inputs().len(), 3);
        assert_eq!(signed.signed_transaction.transaction.body.outputs().len(), 2);
        assert_eq!(signed.signed_transaction.sent_hashes.len(), 1);
        let tx = signed.signed_transaction.transaction.clone();

        let factories = CryptoFactories::default();
        let validator = TransactionInternalConsistencyValidator::new(false, rules, factories);
        assert!(validator.validate(&tx, None, None, u64::MAX).is_ok());
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn batch_offline_sign_is_valid() {
        let rules = create_consensus_manager();
        let alice_key_manager = KeyManager::new_random().unwrap();
        let alice_keys = ViewWallet::new(
            alice_key_manager.get_spend_key().pub_key,
            alice_key_manager.get_private_view_key(),
            None,
        );
        let alice_view_key_manager = create_view_key_manager(alice_keys).unwrap();
        let bob_key_manager = KeyManager::new_random().unwrap();
        let charlie_key_manager = KeyManager::new_random().unwrap();

        let input = create_test_input(MicroMinotari(10000), 0, &alice_view_key_manager, vec![], None);
        let input2 = create_test_input(MicroMinotari(2000), 0, &alice_view_key_manager, vec![], None);
        let input3 = create_test_input(MicroMinotari(15000), 0, &alice_view_key_manager, vec![], None);
        // this replicates the behaviour od the oms that selects the inputs and starts the build tx process.
        let mut tx_builder = TransactionBuilder::new(
            rules.consensus_constants(0).clone(),
            alice_view_key_manager.clone(),
            Network::LocalNet,
        )
        .unwrap();
        tx_builder
            .with_lock_height(0)
            .with_fee_per_gram(MicroMinotari(20))
            .with_input(input)
            .unwrap()
            .with_input(input2)
            .unwrap()
            .with_input(input3)
            .unwrap();

        // now we start the offline process
        let payment_id = MemoField::new_empty();
        let output_features = OutputFeatures::default();
        let amount = MicroMinotari(5000);

        let spend_key = bob_key_manager.get_spend_key().pub_key;
        let view_key = bob_key_manager.get_view_key().pub_key;
        let bob_address = TariAddress::new_dual_address(
            view_key,
            spend_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();

        let spend_key = charlie_key_manager.get_spend_key().pub_key;
        let view_key = charlie_key_manager.get_view_key().pub_key;
        let charlie_address = TariAddress::new_dual_address(
            view_key,
            spend_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();

        let spend_key = alice_view_key_manager.get_spend_key().pub_key;
        let view_key = alice_view_key_manager.get_view_key().pub_key;
        let alice_address = TariAddress::new_dual_address(
            view_key,
            spend_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();

        let spend_key = alice_key_manager.get_spend_key().pub_key;
        let view_key = alice_key_manager.get_view_key().pub_key;
        let alice_address_s = TariAddress::new_dual_address(
            view_key,
            spend_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();

        assert_eq!(alice_address, alice_address_s);
        let recipients = [
            PaymentRecipient {
                amount,
                output_features: output_features.clone(),
                address: bob_address.clone(),
                payment_id: payment_id.clone(),
            },
            PaymentRecipient {
                amount,
                output_features: output_features.clone(),
                address: charlie_address.clone(),
                payment_id: payment_id.clone(),
            },
        ];

        let init = prepare_one_sided_transaction_for_signing(
            TxId::new_random(),
            tx_builder,
            &recipients,
            payment_id,
            alice_address,
        )
        .unwrap();

        assert_eq!(init.info.fee, MicroMinotari(0));
        assert_eq!(init.info.fee_per_gram, MicroMinotari(20));
        assert_eq!(init.info.inputs.len(), 3);
        assert_eq!(init.info.outputs.len(), 0);

        let signed = sign_locked_transaction(
            &alice_key_manager,
            rules.consensus_constants(0).clone(),
            Network::LocalNet,
            init,
        )
        .unwrap();
        assert!(signed.signed_transaction.change_output.is_some());
        assert_eq!(
            signed.signed_transaction.transaction.body.kernels()[0].fee,
            MicroMinotari(4100)
        );
        assert_eq!(signed.signed_transaction.transaction.body.inputs().len(), 3);
        assert_eq!(signed.signed_transaction.transaction.body.outputs().len(), 3);
        assert_eq!(signed.signed_transaction.sent_hashes.len(), 2);
        let tx = signed.signed_transaction.transaction.clone();

        let factories = CryptoFactories::default();
        let validator = TransactionInternalConsistencyValidator::new(false, rules, factories);
        assert!(validator.validate(&tx, None, None, u64::MAX).is_ok());
    }

    #[tokio::test]
    async fn large_batch_offline_sign_is_valid() {
        let rules = create_consensus_manager();
        let alice_key_manager = KeyManager::new_random().unwrap();
        let alice_keys = ViewWallet::new(
            alice_key_manager.get_spend_key().pub_key,
            alice_key_manager.get_private_view_key(),
            None,
        );
        let alice_view_key_manager = create_view_key_manager(alice_keys).unwrap();
        let mut recipients = Vec::new();
        let amount = 100;
        let payment_id = MemoField::new_empty();
        let output_features = OutputFeatures::default();
        for _i in 0..amount {
            let key_manager = KeyManager::new_random().unwrap();
            let spend_key = key_manager.get_spend_key().pub_key;
            let view_key = key_manager.get_view_key().pub_key;
            let address = TariAddress::new_dual_address(
                view_key,
                spend_key,
                Network::LocalNet,
                TariAddressFeatures::create_one_sided_only(),
                None,
            )
            .unwrap();
            let amount = 5000.into();
            let recipient = PaymentRecipient {
                amount,
                output_features: output_features.clone(),
                address,
                payment_id: payment_id.clone(),
            };
            recipients.push(recipient);
        }

        let input = create_test_input(MicroMinotari(1000000000), 0, &alice_view_key_manager, vec![], None);
        // this replicates the behaviour od the oms that selects the inputs and starts the build tx process.
        let mut tx_builder = TransactionBuilder::new(
            rules.consensus_constants(0).clone(),
            alice_view_key_manager.clone(),
            Network::LocalNet,
        )
        .unwrap();
        tx_builder
            .with_lock_height(0)
            .with_fee_per_gram(MicroMinotari(20))
            .with_input(input)
            .unwrap();

        let spend_key = alice_view_key_manager.get_spend_key().pub_key;
        let view_key = alice_view_key_manager.get_view_key().pub_key;
        let alice_address = TariAddress::new_dual_address(
            view_key,
            spend_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();

        let init = prepare_one_sided_transaction_for_signing(
            TxId::new_random(),
            tx_builder,
            &recipients,
            payment_id,
            alice_address,
        )
        .unwrap();

        assert_eq!(init.info.fee, MicroMinotari(0));
        assert_eq!(init.info.fee_per_gram, MicroMinotari(20));
        assert_eq!(init.info.inputs.len(), 1);
        assert_eq!(init.info.outputs.len(), 0);

        let signed = sign_locked_transaction(
            &alice_key_manager,
            rules.consensus_constants(0).clone(),
            Network::LocalNet,
            init,
        )
        .unwrap();
        assert!(signed.signed_transaction.change_output.is_some());
        assert_eq!(
            signed.signed_transaction.transaction.body.kernels()[0].fee,
            MicroMinotari(115500)
        );
        assert_eq!(signed.signed_transaction.transaction.body.inputs().len(), 1);
        assert_eq!(signed.signed_transaction.transaction.body.outputs().len(), 101);
        assert_eq!(signed.signed_transaction.sent_hashes.len(), 100);
        let tx = signed.signed_transaction.transaction.clone();

        let factories = CryptoFactories::default();
        let validator = TransactionInternalConsistencyValidator::new(false, rules, factories);
        assert!(validator.validate(&tx, None, None, u64::MAX).is_ok());
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn offline_deposit_multisign_is_valid() {
        let rules = create_consensus_manager();
        let charlie_key_manager = KeyManager::new_random().unwrap();
        let bob_key_manager = KeyManager::new_random().unwrap();

        let alice_key_manager = KeyManager::new_random().unwrap();
        let alice_keys = ViewWallet::new(
            alice_key_manager.get_spend_key().pub_key,
            alice_key_manager.get_private_view_key(),
            None,
        );
        let alice_view_key_manager = create_view_key_manager(alice_keys).unwrap();

        let bob_spend_key = bob_key_manager.get_spend_key().pub_key;
        let bob_view_key = bob_key_manager.get_view_key().pub_key;
        let bob_address = TariAddress::new_dual_address(
            bob_view_key,
            bob_spend_key.clone(),
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();

        let spend_key = alice_view_key_manager.get_spend_key().pub_key;
        let view_key = alice_view_key_manager.get_view_key().pub_key;
        let alice_address = TariAddress::new_dual_address(
            view_key,
            spend_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();

        let party_number = 2;

        let multisignature_participiants = vec![
            charlie_key_manager.get_spend_key().pub_key,
            alice_key_manager.get_spend_key().pub_key,
            bob_spend_key.clone(),
        ];

        let input = create_test_input(MicroMinotari(10000), 0, &alice_view_key_manager, vec![], None);
        let input2 = create_test_input(MicroMinotari(2000), 0, &alice_view_key_manager, vec![], None);
        let input3 = create_test_input(MicroMinotari(15000), 0, &alice_view_key_manager, vec![], None);
        // this replicates the behaviour od the oms that selects the inputs and starts the build tx process.
        let mut tx_builder = TransactionBuilder::new(
            rules.consensus_constants(0).clone(),
            alice_view_key_manager.clone(),
            Network::LocalNet,
        )
        .unwrap();
        tx_builder
            .with_lock_height(0)
            .with_fee_per_gram(MicroMinotari(20))
            .with_input(input)
            .unwrap()
            .with_input(input2)
            .unwrap()
            .with_input(input3)
            .unwrap();

        // now we start the offline process
        let payment_id = MemoField::new_empty();
        let output_features = OutputFeatures::default();
        let amount = MicroMinotari(5000);

        let spend_key = alice_key_manager.get_spend_key().pub_key;
        let view_key = alice_key_manager.get_view_key().pub_key;
        let alice_address_s = TariAddress::new_dual_address(
            view_key,
            spend_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();

        assert_eq!(alice_address, alice_address_s);
        let init = prepare_deposit_multisig_transaction(
            TxId::new_random(),
            tx_builder,
            amount,
            payment_id,
            output_features,
            party_number,
            multisignature_participiants,
            alice_address,
            bob_address,
        )
        .unwrap();

        assert_eq!(init.info.inputs.len(), 3);
        assert_eq!(init.info.outputs.len(), 0);

        let signed = sign_locked_deposit_multisig_transaction(
            &alice_key_manager,
            rules.consensus_constants(0).clone(),
            Network::LocalNet,
            init,
        )
        .unwrap();

        assert!(signed.signed_transaction.change_output.is_some());
        assert_eq!(
            signed.signed_transaction.transaction.body.kernels()[0].fee,
            MicroMinotari(3120)
        );
        assert_eq!(signed.signed_transaction.transaction.body.inputs().len(), 3);
        assert_eq!(signed.signed_transaction.transaction.body.outputs().len(), 2);
        assert_eq!(signed.signed_transaction.sent_hashes.len(), 1);
        let tx = signed.signed_transaction.transaction.clone();
        let factories = CryptoFactories::default();
        let validator = TransactionInternalConsistencyValidator::new(false, rules, factories);
        assert!(validator.validate(&tx, None, None, u64::MAX).is_ok());
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn offline_withdraw_multisign_is_valid() {
        let rules = create_consensus_manager();
        let alice_key_manager = KeyManager::new_random().unwrap();
        let alice_keys = ViewWallet::new(
            alice_key_manager.get_spend_key().pub_key,
            alice_key_manager.get_private_view_key(),
            None,
        );
        let alice_view_key_manager = create_view_key_manager(alice_keys).unwrap();

        let charlie_key_manager = KeyManager::new_random().unwrap();
        let bob_key_manager = KeyManager::new_random().unwrap();
        let alice_spend_key = alice_key_manager.get_spend_key();

        let bob_spend_key = bob_key_manager.get_spend_key().pub_key;
        let bob_view_key = bob_key_manager.get_view_key().pub_key;
        let bob_address = TariAddress::new_dual_address(
            bob_view_key,
            bob_spend_key.clone(),
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();

        let spend_key = alice_key_manager.get_spend_key().pub_key;
        let view_key = alice_key_manager.get_view_key().pub_key;
        let alice_address_s = TariAddress::new_dual_address(
            view_key,
            spend_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();

        let spend_key = alice_view_key_manager.get_spend_key().pub_key;
        let view_key = alice_view_key_manager.get_view_key().pub_key;
        let alice_address = TariAddress::new_dual_address(
            view_key,
            spend_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();

        let party_number = 2;

        let multisignature_participiants = vec![
            charlie_key_manager.get_spend_key().pub_key,
            alice_key_manager.get_spend_key().pub_key,
            bob_spend_key.clone(),
        ];

        let sender_offset_key = alice_view_key_manager.get_random_key(None, false).unwrap();

        let mut message = Box::new([0u8; 32]);
        OsRng.fill_bytes(message.as_mut());

        let mut signatures: Vec<CompressedCheckSigSchnorrSignature> = Vec::new();

        let ephemeral_pubkeys = derive_multisig_ephemeral_pubkeys(
            &alice_view_key_manager,
            &multisignature_participiants,
            &sender_offset_key.key_id,
        )
        .unwrap();

        signatures.push(
            charlie_key_manager
                .sign_script_message_with_spend_key(&message[..], Some(&sender_offset_key.pub_key))
                .unwrap(),
        );
        signatures.push(
            bob_key_manager
                .sign_script_message_with_spend_key(&message[..], Some(&sender_offset_key.pub_key))
                .unwrap(),
        );

        let mut script_opcodes = vec![Opcode::CheckMultiSigVerify(
            party_number,
            u8::try_from(ephemeral_pubkeys.len()).unwrap(),
            ephemeral_pubkeys.clone(),
            message,
        )];

        let commitment_mask_private_key = TariKeyId::DHCommitmentMask {
            private_key: sender_offset_key.key_id.clone().into(),
            public_key: alice_spend_key.pub_key.clone(),
        };

        let script_pubkey = alice_key_manager
            .stealth_address_script_spending_key(&commitment_mask_private_key, &alice_spend_key.pub_key)
            .unwrap();

        script_opcodes.push(Opcode::PushPubKey(Box::new(script_pubkey.clone())));

        let full_script = TariScript::new(script_opcodes).unwrap();
        let amount = MicroMinotari(5000);
        let payment_id = MemoField::new_empty();
        let (commitment_mask_key, _script_key) = alice_view_key_manager
            .get_next_commitment_mask_and_script_key()
            .unwrap();

        let mut input_stack = ExecutionStack::default();

        for sig in signatures.clone() {
            input_stack.push(StackItem::Signature(sig)).unwrap();
        }

        let output_features = OutputFeatures::default();

        let script_key = TariKeyId::Derived {
            key: SerializedKeyString::from(commitment_mask_private_key.to_string()),
        };

        let input = WalletOutputBuilder::new(amount, commitment_mask_key.key_id.clone())
            .with_script(full_script.clone())
            .with_features(output_features.clone())
            .with_input_data(input_stack)
            .with_script_key(script_key.clone())
            .encrypt_data_for_recovery(&alice_view_key_manager, None, payment_id.clone())
            .unwrap()
            .with_sender_offset_public_key(sender_offset_key.pub_key.clone())
            .sign_metadata_signature_user_verified(
                &alice_view_key_manager,
                &sender_offset_key.key_id,
                &Default::default(),
            )
            .unwrap()
            .try_build(&alice_view_key_manager)
            .unwrap();

        let consensus_constants = rules.consensus_constants(0);

        let mut tx_builder = TransactionBuilder::new(
            consensus_constants.clone(),
            alice_view_key_manager.clone(),
            Network::LocalNet,
        )
        .unwrap();

        let fee_per_gram = MicroMinotari(2);

        tx_builder
            .with_lock_height(0)
            .with_fee_per_gram(fee_per_gram)
            .with_input(input)
            .unwrap();

        // now we start the offline process

        let fee_calculator = Fee::new(*consensus_constants.transaction_weight_params());
        let script = push_pubkey_script(&Default::default());

        let features_and_scripts_byte_size = consensus_constants
            .transaction_weight_params()
            .round_up_features_and_scripts_size(
                output_features.get_serialized_size().unwrap() +
                    script.get_serialized_size().unwrap() +
                    Covenant::default().get_serialized_size().unwrap(),
            );

        let fee: MicroMinotari = fee_calculator.calculate(fee_per_gram, 1, 1, 1, features_and_scripts_byte_size);

        let total_amount = amount.checked_sub(fee).unwrap();
        assert_eq!(alice_address, alice_address_s);
        let init = prepare_withdraw_multisig_transaction(
            TxId::new_random(),
            tx_builder,
            total_amount,
            payment_id,
            output_features,
            alice_address,
            bob_address,
        )
        .unwrap();
        assert_eq!(init.info.inputs.len(), 1);
        assert_eq!(init.info.outputs.len(), 0);
        let signed = sign_locked_withdraw_multisig_transaction(
            &alice_key_manager,
            rules.consensus_constants(0).clone(),
            Network::LocalNet,
            init,
        )
        .unwrap();
        assert_eq!(signed.signed_transaction.transaction.body.kernels()[0].fee, fee,);
        assert_eq!(signed.signed_transaction.transaction.body.inputs().len(), 1);
        assert_eq!(signed.signed_transaction.transaction.body.outputs().len(), 1);
        assert_eq!(signed.signed_transaction.sent_hashes.len(), 1);
        let tx = signed.signed_transaction.transaction.clone();
        let factories = CryptoFactories::default();
        let validator = TransactionInternalConsistencyValidator::new(false, rules, factories);
        assert!(validator.validate(&tx, None, None, u64::MAX).is_ok());
    }

    #[tokio::test]
    async fn offline_sign_can_be_claimed() {
        let rules = create_consensus_manager();
        let alice_key_manager = KeyManager::new_random().unwrap();
        let alice_keys = ViewWallet::new(
            alice_key_manager.get_spend_key().pub_key,
            alice_key_manager.get_private_view_key(),
            None,
        );
        let alice_view_key_manager = create_view_key_manager(alice_keys).unwrap();
        let bob_key_manager = KeyManager::new_random().unwrap();

        let input = create_test_input(MicroMinotari(100000), 0, &alice_view_key_manager, vec![], None);
        // this replicates the behaviour od the oms that selects the inputs and starts the build tx process.
        let mut tx_builder = TransactionBuilder::new(
            rules.consensus_constants(0).clone(),
            alice_view_key_manager.clone(),
            Network::LocalNet,
        )
        .unwrap();
        tx_builder
            .with_lock_height(0)
            .with_fee_per_gram(MicroMinotari(20))
            .with_input(input)
            .unwrap();

        // now we start the offline process
        let spend_key = bob_key_manager.get_spend_key().pub_key;
        let view_key = bob_key_manager.get_view_key().pub_key;
        let bob_address = TariAddress::new_dual_address(
            view_key,
            spend_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();
        let spend_key = alice_view_key_manager.get_spend_key().pub_key;
        let view_key = alice_view_key_manager.get_view_key().pub_key;
        let alice_address = TariAddress::new_dual_address(
            view_key,
            spend_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();
        let recipients = [PaymentRecipient {
            amount: MicroMinotari(5000),
            output_features: OutputFeatures::default(),
            address: bob_address.clone(),
            payment_id: MemoField::new_empty(),
        }];
        let init = prepare_one_sided_transaction_for_signing(
            TxId::new_random(),
            tx_builder,
            &recipients,
            MemoField::new_empty(),
            alice_address,
        )
        .unwrap();

        let signed = sign_locked_transaction(
            &alice_key_manager,
            rules.consensus_constants(0).clone(),
            Network::LocalNet,
            init,
        )
        .unwrap();
        let tx = signed.signed_transaction.transaction.clone();

        let factories = CryptoFactories::default();
        let validator = TransactionInternalConsistencyValidator::new(false, rules, factories);
        assert!(validator.validate(&tx, None, None, u64::MAX).is_ok());

        let outputs = signed.signed_transaction.transaction.body.outputs();
        let mut sent_index = 99;
        for (i, output) in outputs.iter().enumerate() {
            if output.hash() == signed.signed_transaction.sent_hashes[0] {
                sent_index = i;
            }
        }
        let change_index = if sent_index == 0 { 1 } else { 0 };

        let change_output = &signed.signed_transaction.transaction.body.outputs()[change_index].clone();

        // let see if alice's view wallet can claim the change:
        assert!(alice_view_key_manager
            .is_this_output_ours(&change_output.commitment, &change_output.encrypted_data, None,)
            .unwrap());
        // lets test the hot wallet
        assert!(alice_key_manager
            .is_this_output_ours(&change_output.commitment, &change_output.encrypted_data, None,)
            .unwrap());

        // lets see if bob's wallet can claim the sent:
        let sent_output = &signed.signed_transaction.transaction.body.outputs()[sent_index].clone();
        let view_key = bob_key_manager.get_view_key();
        let shared_secret = bob_key_manager
            .get_diffie_hellman_shared_secret(&view_key.key_id, &sent_output.sender_offset_public_key)
            .unwrap();

        let recovery_key = public_key_to_output_encryption_key(&shared_secret).unwrap();
        let res =
            EncryptedData::decrypt_data(&recovery_key, &sent_output.commitment, &sent_output.encrypted_data).unwrap();
        assert_eq!(res.0, MicroMinotari(5000));
    }

    #[tokio::test]
    async fn view_only_cannot_sign_offline() {
        let rules = create_consensus_manager();
        let alice_key_manager = KeyManager::new_random().unwrap();
        let alice_keys = ViewWallet::new(
            alice_key_manager.get_spend_key().pub_key,
            alice_key_manager.get_private_view_key(),
            None,
        );
        let alice_view_key_manager = create_view_key_manager(alice_keys).unwrap();

        let bob_key_manager = KeyManager::new_random().unwrap();

        let input = create_test_input(MicroMinotari(10000), 0, &alice_view_key_manager, vec![], None);
        let input2 = create_test_input(MicroMinotari(2000), 0, &alice_view_key_manager, vec![], None);
        let input3 = create_test_input(MicroMinotari(15000), 0, &alice_view_key_manager, vec![], None);
        // this replicates the behaviour od the oms that selects the inputs and starts the build tx process.
        let mut tx_builder = TransactionBuilder::new(
            rules.consensus_constants(0).clone(),
            alice_view_key_manager.clone(),
            Network::LocalNet,
        )
        .unwrap();
        tx_builder
            .with_lock_height(0)
            .with_fee_per_gram(MicroMinotari(20))
            .with_input(input)
            .unwrap()
            .with_input(input2)
            .unwrap()
            .with_input(input3)
            .unwrap();

        // now we start the offline process
        let payment_id = MemoField::new_empty();
        let output_features = OutputFeatures::default();
        let amount = MicroMinotari(5000);

        let spend_key = bob_key_manager.get_spend_key().pub_key;
        let view_key = bob_key_manager.get_view_key().pub_key;
        let bob_address = TariAddress::new_dual_address(
            view_key,
            spend_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();
        let spend_key = alice_view_key_manager.get_spend_key().pub_key;
        let view_key = alice_view_key_manager.get_view_key().pub_key;
        let alice_address = TariAddress::new_dual_address(
            view_key,
            spend_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();

        let spend_key = alice_key_manager.get_spend_key().pub_key;
        let view_key = alice_key_manager.get_view_key().pub_key;
        let alice_address_s = TariAddress::new_dual_address(
            view_key,
            spend_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();

        assert_eq!(alice_address, alice_address_s);
        let recipients = [PaymentRecipient {
            amount,
            output_features: output_features.clone(),
            address: bob_address.clone(),
            payment_id: payment_id.clone(),
        }];

        let init = prepare_one_sided_transaction_for_signing(
            TxId::new_random(),
            tx_builder,
            &recipients,
            payment_id,
            alice_address,
        )
        .unwrap();

        assert_eq!(init.info.inputs.len(), 3);
        assert_eq!(init.info.outputs.len(), 0);

        assert!(sign_locked_transaction(
            &alice_view_key_manager,
            rules.consensus_constants(0).clone(),
            Network::LocalNet,
            init
        )
        .is_err());
    }
}
