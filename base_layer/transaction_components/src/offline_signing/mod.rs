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
pub mod models;
pub mod offline_signer;
pub mod one_sided_signer;
pub mod payload_summary;

pub use models::PaymentRecipient;
pub use offline_signer::{
    prepare_deposit_multisig_transaction,
    prepare_one_sided_transaction_for_signing,
    prepare_withdraw_multisig_transaction,
    sign_locked_deposit_multisig_transaction,
    sign_locked_transaction,
    sign_locked_withdraw_multisig_transaction,
};
pub use payload_summary::{OutputSummary, PayloadSummary, RecipientSummary};

#[cfg(test)]
mod test {
    #![allow(clippy::indexing_slicing)]
    use rand::Rng;
    use tari_common::configuration::Network;
    use tari_common_types::{
        tari_address::{TariAddress, TariAddressFeatures},
        transaction::TxId,
    };
    use tari_script::{
        CompressedCheckSigSchnorrSignature,
        ExecutionStack,
        Opcode,
        StackItem,
        TariScript,
        push_pubkey_script,
    };

    use crate::{
        MicroMinotari,
        TransactionBuilder,
        TransactionBuilderError,
        crypto_factories::CryptoFactories,
        fee::{Fee, addressed_output_memo, recipient_output_features_and_scripts_size},
        key_manager::{
            KeyManager,
            SerializedKeyString,
            TariKeyId,
            TransactionKeyManagerInterface,
            error::KeyManagerError,
            wallet_types::{ViewWallet, WalletType},
        },
        multisig::script::derive_multisig_ephemeral_pubkeys,
        offline_signing::{
            PaymentRecipient,
            models::SignedOneSidedTransactionResult,
            offline_signer::sign_locked_transaction,
            one_sided_signer::{multisig_pending_output, withdraw_pending_output},
            prepare_deposit_multisig_transaction,
            prepare_one_sided_transaction_for_signing,
            prepare_withdraw_multisig_transaction,
            sign_locked_deposit_multisig_transaction,
            sign_locked_withdraw_multisig_transaction,
        },
        test_helpers::{create_consensus_manager, create_test_input},
        transaction_components::{
            EncryptedData,
            MemoField,
            OutputFeatures,
            WalletOutputBuilder,
            covenants::Covenant,
            memo_field::TxType,
            one_sided::public_key_to_output_encryption_key,
        },
        validation::transaction::TransactionInternalConsistencyValidator,
    };

    fn create_view_key_manager(view_wallet: ViewWallet) -> Result<KeyManager, KeyManagerError> {
        let wallet = WalletType::ViewWallet(view_wallet);
        KeyManager::new(wallet)
    }

    #[test]
    fn offline_sign_is_valid() {
        let rules = create_consensus_manager();
        let alice_key_manager = KeyManager::new_random().unwrap();
        let alice_keys = ViewWallet::new(
            alice_key_manager.get_spend_key().pub_key,
            alice_key_manager.get_private_view_key(),
            None,
        );
        let alice_view_key_manager = create_view_key_manager(alice_keys).unwrap();
        let bob_key_manager = KeyManager::new_random().unwrap();

        let input = create_test_input(MicroMinotari(10000), 0, &alice_key_manager, vec![], None);
        let input2 = create_test_input(MicroMinotari(2000), 0, &alice_key_manager, vec![], None);
        let input3 = create_test_input(MicroMinotari(15000), 0, &alice_key_manager, vec![], None);
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
            &alice_view_key_manager,
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
            MicroMinotari(3120)
        );
        assert_eq!(signed.signed_transaction.transaction.body.inputs().len(), 3);
        assert_eq!(signed.signed_transaction.transaction.body.outputs().len(), 2);
        assert_eq!(signed.signed_transaction.sent_hashes.len(), 1);
        assert_eq!(signed.signed_transaction.outputs.len(), 1);
        let tx = signed.signed_transaction.transaction.clone();

        let factories = CryptoFactories::default();
        let validator = TransactionInternalConsistencyValidator::new(false, rules, factories);
        assert!(validator.validate(&tx, None, None, u64::MAX).is_ok());
    }

    /// Prepare a one-sided payment of `amount` to `recipient` from a single input worth `input_value`, carrying
    /// `payment_id`, and sign it with the spend key.
    fn prepare_and_sign_one_sided(
        spend_key_manager: &KeyManager,
        view_key_manager: &KeyManager,
        recipient: &TariAddress,
        input_value: MicroMinotari,
        amount: MicroMinotari,
        payment_id: MemoField,
    ) -> Result<SignedOneSidedTransactionResult, TransactionBuilderError> {
        let rules = create_consensus_manager();
        let input = create_test_input(input_value, 0, spend_key_manager, vec![], None);
        let mut tx_builder = TransactionBuilder::new(
            rules.consensus_constants(0).clone(),
            view_key_manager.clone(),
            Network::LocalNet,
        )
        .unwrap();
        tx_builder
            .with_fee_per_gram(MicroMinotari(20))
            .with_input(input)
            .unwrap();
        let own_address = TariAddress::new_dual_address(
            view_key_manager.get_view_key().pub_key,
            view_key_manager.get_spend_key().pub_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();
        let recipients = [PaymentRecipient {
            amount,
            output_features: OutputFeatures::default(),
            address: recipient.clone(),
            payment_id: payment_id.clone(),
        }];
        let init = prepare_one_sided_transaction_for_signing(
            view_key_manager,
            TxId::new_random(),
            tx_builder,
            &recipients,
            payment_id,
            own_address,
        )
        .unwrap();
        sign_locked_transaction(
            spend_key_manager,
            rules.consensus_constants(0).clone(),
            Network::LocalNet,
            init,
        )
    }

    /// The signer must refuse any payload that would leave no change output, because the recipient outputs it
    /// returns would then give the view-key holder every term of the script offset but the spend key. A payload one
    /// microminotari over the boundary must sign, with that microminotari as change.
    #[test]
    fn offline_sign_requires_a_change_output() {
        let rules = create_consensus_manager();
        let alice_key_manager = KeyManager::new_random().unwrap();
        let alice_view_key_manager = create_view_key_manager(ViewWallet::new(
            alice_key_manager.get_spend_key().pub_key,
            alice_key_manager.get_private_view_key(),
            None,
        ))
        .unwrap();
        let bob_key_manager = KeyManager::new_random().unwrap();
        let bob_address = TariAddress::new_dual_address(
            bob_key_manager.get_view_key().pub_key,
            bob_key_manager.get_spend_key().pub_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();
        let amount = MicroMinotari(5000);

        // Measure the fee the payment carries without change, and what a change output would add to it, from a
        // builder in the state the signer's builder is in when it makes the change decision.
        let mut probe = TransactionBuilder::new(
            rules.consensus_constants(0).clone(),
            alice_key_manager.clone(),
            Network::LocalNet,
        )
        .unwrap();
        probe
            .with_fee_per_gram(MicroMinotari(20))
            .with_input(create_test_input(
                MicroMinotari(100_000),
                0,
                &alice_key_manager,
                vec![],
                None,
            ))
            .unwrap();
        probe
            .add_stealth_recipient(
                bob_address.clone(),
                amount,
                OutputFeatures::default(),
                MemoField::new_empty(),
            )
            .unwrap();
        probe.with_memo(MemoField::new_empty());
        let fee = probe.get_fee_estimate_without_change().unwrap();
        let change_fee = probe.get_change_output_fee(&[]).unwrap();
        assert!(change_fee > MicroMinotari::zero());

        // Inputs equal to amount plus fee: nothing is left for change.
        let err = prepare_and_sign_one_sided(
            &alice_key_manager,
            &alice_view_key_manager,
            &bob_address,
            amount + fee,
            amount,
            MemoField::new_empty(),
        )
        .unwrap_err();
        assert!(
            matches!(
                err,
                TransactionBuilderError::OfflineTransactionRequiresChange { remainder, change_fee: c }
                    if remainder == MicroMinotari::zero() && c == change_fee
            ),
            "expected the signer to refuse a payment without change, got {err:?}"
        );

        // A remainder that only covers the change output's own fee would leave a zero change, so it goes to the fee
        // and the payment is still refused.
        let err = prepare_and_sign_one_sided(
            &alice_key_manager,
            &alice_view_key_manager,
            &bob_address,
            amount + fee + change_fee,
            amount,
            MemoField::new_empty(),
        )
        .unwrap_err();
        assert!(
            matches!(
                err,
                TransactionBuilderError::OfflineTransactionRequiresChange { remainder, change_fee: c }
                    if remainder == change_fee && c == change_fee
            ),
            "expected the signer to refuse a payment without change, got {err:?}"
        );

        // One microminotari more and the payment signs with that as its change.
        let signed = prepare_and_sign_one_sided(
            &alice_key_manager,
            &alice_view_key_manager,
            &bob_address,
            amount + fee + change_fee + MicroMinotari(1),
            amount,
            MemoField::new_empty(),
        )
        .unwrap();
        let signed_tx = &signed.signed_transaction;
        assert_eq!(
            signed_tx.change_output.as_ref().map(|o| o.value()),
            Some(MicroMinotari(1))
        );
        assert_eq!(signed_tx.transaction.body.kernels()[0].fee, fee + change_fee);
        assert_eq!(signed_tx.transaction.body.outputs().len(), 2);
        let validator = TransactionInternalConsistencyValidator::new(false, rules, CryptoFactories::default());
        validator
            .validate(&signed_tx.transaction, None, None, u64::MAX)
            .unwrap();
    }

    /// A payment id that fits the recipient's memo but leaves no room for the change memo, which also carries an
    /// address, the amounts and the sent output hash, means no change output can be built. The signer must refuse
    /// it while deciding fee and change, rather than sign the recipient output and fail on the change.
    #[test]
    fn offline_sign_refuses_a_payment_id_the_change_memo_cannot_carry() {
        let alice_key_manager = KeyManager::new_random().unwrap();
        let alice_view_key_manager = create_view_key_manager(ViewWallet::new(
            alice_key_manager.get_spend_key().pub_key,
            alice_key_manager.get_private_view_key(),
            None,
        ))
        .unwrap();
        let bob_key_manager = KeyManager::new_random().unwrap();
        let bob_address = TariAddress::new_dual_address(
            bob_key_manager.get_view_key().pub_key,
            bob_key_manager.get_spend_key().pub_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();
        let payment_id = MemoField::new_open_from_string(&"x".repeat(200), TxType::PaymentToOther).unwrap();

        let err = prepare_and_sign_one_sided(
            &alice_key_manager,
            &alice_view_key_manager,
            &bob_address,
            MicroMinotari(100_000),
            MicroMinotari(5000),
            payment_id.clone(),
        )
        .unwrap_err();
        assert!(
            matches!(err, TransactionBuilderError::InvalidMemo(_)),
            "expected the signer to refuse the change memo, got {err:?}"
        );

        // The refusal comes from the fee and change decision itself, which the reservation makes before it mints any
        // sender offset key, and which the online wallet asks before it locks any input.
        let rules = create_consensus_manager();
        let builder_for = |input_value: MicroMinotari, amount: MicroMinotari| {
            let mut builder = TransactionBuilder::new(
                rules.consensus_constants(0).clone(),
                alice_key_manager.clone(),
                Network::LocalNet,
            )
            .unwrap();
            builder
                .with_fee_per_gram(MicroMinotari(20))
                .with_input(create_test_input(input_value, 0, &alice_key_manager, vec![], None))
                .unwrap();
            builder
                .add_stealth_recipient(
                    bob_address.clone(),
                    amount,
                    OutputFeatures::default(),
                    payment_id.clone(),
                )
                .unwrap();
            builder.with_memo(payment_id.clone());
            builder
        };
        let err = builder_for(MicroMinotari(100_000), MicroMinotari(5000))
            .check_change_output(&[])
            .unwrap_err();
        assert!(matches!(err, TransactionBuilderError::InvalidMemo(_)), "got {err:?}");

        // A payment that leaves nothing for change never needs the change memo, so it is not refused for it.
        let fee = builder_for(MicroMinotari(100_000), MicroMinotari(5000))
            .get_fee_estimate_without_change()
            .unwrap();
        let err = builder_for(MicroMinotari(5000) + fee, MicroMinotari(5000))
            .check_change_output(&[])
            .unwrap_err();
        assert!(
            matches!(err, TransactionBuilderError::OfflineTransactionRequiresChange { .. }),
            "got {err:?}"
        );
    }

    /// A payload carrying a directly-specified output must sign into a *valid* transaction.
    ///
    /// The output arrives with a metadata signature made against a sender offset key the signer does not hold, so the
    /// signer replaces both the key and the signature. Without that, the output travels into the body still signed
    /// against the discarded key and the whole transaction is unbroadcastable. The output here is built the way a
    /// wallet builds one — a genuine signature, not a placeholder — because a placeholder takes a different branch in
    /// the builder and would hide the defect.
    #[test]
    #[allow(clippy::too_many_lines)]
    fn offline_sign_with_a_custom_output_is_valid() {
        use crate::{
            offline_signing::PayloadSummary,
            test_helpers::{TestParams, create_wallet_output_with_data},
        };

        let rules = create_consensus_manager();
        let alice_key_manager = KeyManager::new_random().unwrap();
        let alice_keys = ViewWallet::new(
            alice_key_manager.get_spend_key().pub_key,
            alice_key_manager.get_private_view_key(),
            None,
        );
        let alice_view_key_manager = create_view_key_manager(alice_keys).unwrap();
        let bob_key_manager = KeyManager::new_random().unwrap();

        let input = create_test_input(MicroMinotari(50000), 0, &alice_key_manager, vec![], None);
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

        // A fully formed output, signed against a sender offset key that the offline signer will replace
        let custom_value = MicroMinotari(6000);
        let custom_output = create_wallet_output_with_data(
            push_pubkey_script(&bob_key_manager.get_spend_key().pub_key),
            OutputFeatures::default(),
            &TestParams::new(&alice_key_manager),
            custom_value,
            &alice_view_key_manager,
        )
        .unwrap();
        assert_ne!(
            custom_output.metadata_signature(),
            &Default::default(),
            "the output must carry a real signature, or the builder takes its placeholder branch"
        );
        let custom_sender_offset = alice_view_key_manager.get_random_key(None, None).unwrap();
        tx_builder
            .with_output(custom_output, custom_sender_offset.key_id, None)
            .unwrap();

        let bob_address = TariAddress::new_dual_address(
            bob_key_manager.get_view_key().pub_key,
            bob_key_manager.get_spend_key().pub_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();
        let alice_address = TariAddress::new_dual_address(
            alice_view_key_manager.get_view_key().pub_key,
            alice_view_key_manager.get_spend_key().pub_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();

        let amount = MicroMinotari(5000);
        let recipients = [PaymentRecipient {
            amount,
            output_features: OutputFeatures::default(),
            address: bob_address,
            payment_id: MemoField::new_empty(),
        }];

        let init = prepare_one_sided_transaction_for_signing(
            &alice_view_key_manager,
            TxId::new_random(),
            tx_builder,
            &recipients,
            MemoField::new_empty(),
            alice_address,
        )
        .unwrap();
        assert_eq!(init.info.outputs.len(), 1);

        // The summary the operator approves must account for the custom output
        let summary = PayloadSummary::from_one_sided(init.tx_id, &init.info);
        assert_eq!(summary.total_output_amount, custom_value);
        assert_eq!(summary.total_spend(), amount + custom_value);

        let signed = sign_locked_transaction(
            &alice_key_manager,
            rules.consensus_constants(0).clone(),
            Network::LocalNet,
            init,
        )
        .unwrap();

        let tx = signed.signed_transaction.transaction.clone();
        // recipient + custom output + change
        assert_eq!(tx.body.outputs().len(), 3);
        for output in tx.body.outputs() {
            output.verify_metadata_signature().unwrap();
        }
        let fee = tx.body.kernels()[0].fee;
        assert_eq!(
            signed.signed_transaction.change_output.clone().unwrap().value(),
            MicroMinotari(50000) - amount - custom_value - fee
        );

        let factories = CryptoFactories::default();
        let validator = TransactionInternalConsistencyValidator::new(false, rules, factories);
        validator.validate(&tx, None, None, u64::MAX).unwrap();
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn batch_offline_sign_is_valid() {
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

        let input = create_test_input(MicroMinotari(10000), 0, &alice_key_manager, vec![], None);
        let input2 = create_test_input(MicroMinotari(2000), 0, &alice_key_manager, vec![], None);
        let input3 = create_test_input(MicroMinotari(15000), 0, &alice_key_manager, vec![], None);
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
            &alice_view_key_manager,
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
            MicroMinotari(4280)
        );
        assert_eq!(signed.signed_transaction.transaction.body.inputs().len(), 3);
        assert_eq!(signed.signed_transaction.transaction.body.outputs().len(), 3);
        assert_eq!(signed.signed_transaction.sent_hashes.len(), 2);
        assert_eq!(signed.signed_transaction.outputs.len(), 2);
        let tx = signed.signed_transaction.transaction.clone();

        let factories = CryptoFactories::default();
        let validator = TransactionInternalConsistencyValidator::new(false, rules, factories);
        assert!(validator.validate(&tx, None, None, u64::MAX).is_ok());
    }

    #[test]
    fn large_batch_offline_sign_is_valid() {
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

        let input = create_test_input(MicroMinotari(1000000000), 0, &alice_key_manager, vec![], None);
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
            &alice_view_key_manager,
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
            MicroMinotari(115660)
        );
        assert_eq!(signed.signed_transaction.transaction.body.inputs().len(), 1);
        assert_eq!(signed.signed_transaction.transaction.body.outputs().len(), 101);
        assert_eq!(signed.signed_transaction.sent_hashes.len(), 100);
        assert_eq!(signed.signed_transaction.outputs.len(), 100);
        let tx = signed.signed_transaction.transaction.clone();

        let factories = CryptoFactories::default();
        let validator = TransactionInternalConsistencyValidator::new(false, rules, factories);
        assert!(validator.validate(&tx, None, None, u64::MAX).is_ok());
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn offline_deposit_multisign_is_valid() {
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

        let input = create_test_input(MicroMinotari(10000), 0, &alice_key_manager, vec![], None);
        let input2 = create_test_input(MicroMinotari(2000), 0, &alice_key_manager, vec![], None);
        let input3 = create_test_input(MicroMinotari(15000), 0, &alice_key_manager, vec![], None);
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
            &alice_view_key_manager,
            TxId::new_random(),
            tx_builder.clone(),
            amount,
            payment_id.clone(),
            output_features.clone(),
            party_number,
            multisignature_participiants.clone(),
            alice_address.clone(),
            bob_address.clone(),
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
            MicroMinotari(3280)
        );
        assert_eq!(signed.signed_transaction.transaction.body.inputs().len(), 3);
        assert_eq!(signed.signed_transaction.transaction.body.outputs().len(), 2);
        assert_eq!(signed.signed_transaction.sent_hashes.len(), 1);
        assert_eq!(signed.signed_transaction.outputs.len(), 1);
        let tx = signed.signed_transaction.transaction.clone();
        let factories = CryptoFactories::default();
        let validator = TransactionInternalConsistencyValidator::new(false, rules.clone(), factories);
        assert!(validator.validate(&tx, None, None, u64::MAX).is_ok());

        // The fee the signer charged is exactly the online estimate for the same declaration, change memo included.
        let mut probe = tx_builder.clone();
        probe.with_memo(payment_id.clone());
        let pending = multisig_pending_output(
            rules.consensus_constants(0),
            &bob_address,
            amount,
            &output_features,
            &payment_id,
            party_number,
            multisignature_participiants.len(),
        )
        .unwrap();
        let change_fee = probe.get_change_output_fee(std::slice::from_ref(&pending)).unwrap();
        let fee_without_change = probe.get_fee_estimate_with(&[pending]).unwrap();
        assert_eq!(
            signed.signed_transaction.transaction.body.kernels()[0].fee,
            fee_without_change + change_fee
        );

        // The same deposit, sized to spend every input exactly, leaves no change and must be refused.
        let init = prepare_deposit_multisig_transaction(
            &alice_view_key_manager,
            TxId::new_random(),
            tx_builder,
            MicroMinotari(27000) - fee_without_change,
            payment_id,
            output_features,
            party_number,
            multisignature_participiants,
            alice_address,
            bob_address,
        )
        .unwrap();
        let err = sign_locked_deposit_multisig_transaction(
            &alice_key_manager,
            rules.consensus_constants(0).clone(),
            Network::LocalNet,
            init,
        )
        .unwrap_err();
        assert!(
            matches!(
                err,
                TransactionBuilderError::OfflineTransactionRequiresChange { remainder, change_fee: c }
                    if remainder == MicroMinotari::zero() && c == change_fee
            ),
            "expected the signer to refuse a deposit without change, got {err:?}"
        );
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn offline_withdraw_multisign_is_valid() {
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

        let sender_offset_key = alice_view_key_manager.get_random_key(None, None).unwrap();

        let mut message = Box::new([0u8; 32]);
        rand::rng().fill_bytes(message.as_mut());

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

        // Mirror what `PrepareWithdrawMultisigTransaction` does: the input goes to one recipient output, and that
        // output carries an `AddressAndData` memo which the builder charges for. The memo records the fee being
        // calculated here, so measure a zero-fee copy first.
        let measured_memo = addressed_output_memo(
            MemoField::default(),
            bob_address.clone(),
            MicroMinotari::zero(),
            TxType::PaymentToOther,
        )
        .unwrap();
        let features_and_scripts_byte_size = recipient_output_features_and_scripts_size(
            consensus_constants.transaction_weight_params(),
            &output_features,
            &script,
            &Covenant::default(),
            &measured_memo,
        )
        .unwrap();

        let fee: MicroMinotari = fee_calculator.calculate(fee_per_gram, 1, 1, 1, features_and_scripts_byte_size);
        let output_payment_id =
            addressed_output_memo(MemoField::default(), bob_address.clone(), fee, TxType::PaymentToOther).unwrap();
        assert!(
            output_payment_id.get_size() > payment_id.get_size(),
            "the output memo must be non-empty, otherwise this test cannot tell whether the estimate counts it"
        );

        assert_eq!(alice_address, alice_address_s);

        // Handing the whole input to the recipient leaves no change, and the signer must refuse it.
        let init = prepare_withdraw_multisig_transaction(
            &alice_view_key_manager,
            TxId::new_random(),
            tx_builder.clone(),
            amount.checked_sub(fee).unwrap(),
            output_payment_id.clone(),
            output_features.clone(),
            alice_address.clone(),
            bob_address.clone(),
        )
        .unwrap();
        let err = sign_locked_withdraw_multisig_transaction(
            &alice_key_manager,
            rules.consensus_constants(0).clone(),
            Network::LocalNet,
            init,
        )
        .unwrap_err();
        // Measured the way the signer measures it: with the memo and the recipient declared.
        let mut probe = tx_builder.clone();
        probe.with_memo(output_payment_id.clone());
        let recipient = PaymentRecipient {
            amount: MicroMinotari::zero(),
            output_features: output_features.clone(),
            address: bob_address.clone(),
            payment_id: output_payment_id.clone(),
        };
        let change_fee = probe
            .get_change_output_fee(&[
                withdraw_pending_output(consensus_constants, &recipient, &output_payment_id).unwrap(),
            ])
            .unwrap();
        assert!(
            matches!(
                err,
                TransactionBuilderError::OfflineTransactionRequiresChange { remainder, change_fee: c }
                    if remainder == MicroMinotari::zero() && c == change_fee
            ),
            "expected the signer to refuse a withdrawal without change, got {err:?}"
        );

        // Leaving just enough for one microminotari of change after the change output's own fee signs.
        let total_amount = amount - fee - change_fee - MicroMinotari(1);
        let init = prepare_withdraw_multisig_transaction(
            &alice_view_key_manager,
            TxId::new_random(),
            tx_builder,
            total_amount,
            output_payment_id,
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
        assert_eq!(
            signed.signed_transaction.transaction.body.kernels()[0].fee,
            fee + change_fee
        );
        assert_eq!(
            signed.signed_transaction.change_output.as_ref().map(|o| o.value()),
            Some(MicroMinotari(1))
        );
        assert_eq!(signed.signed_transaction.transaction.body.inputs().len(), 1);
        assert_eq!(signed.signed_transaction.transaction.body.outputs().len(), 2);
        assert_eq!(signed.signed_transaction.sent_hashes.len(), 1);
        assert_eq!(signed.signed_transaction.outputs.len(), 1);
        let tx = signed.signed_transaction.transaction.clone();
        let factories = CryptoFactories::default();
        let validator = TransactionInternalConsistencyValidator::new(false, rules, factories);
        assert!(validator.validate(&tx, None, None, u64::MAX).is_ok());
    }

    #[test]
    fn offline_sign_can_be_claimed() {
        let rules = create_consensus_manager();
        let alice_key_manager = KeyManager::new_random().unwrap();
        let alice_keys = ViewWallet::new(
            alice_key_manager.get_spend_key().pub_key,
            alice_key_manager.get_private_view_key(),
            None,
        );
        let alice_view_key_manager = create_view_key_manager(alice_keys).unwrap();
        let bob_key_manager = KeyManager::new_random().unwrap();

        let input = create_test_input(MicroMinotari(100000), 0, &alice_key_manager, vec![], None);
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

        let payment_id_bob = MemoField::new_open_from_string("bob message", TxType::PaymentToOther).unwrap();
        let recipients = [PaymentRecipient {
            amount: MicroMinotari(5000),
            output_features: OutputFeatures::default(),
            address: bob_address.clone(),
            payment_id: payment_id_bob.clone(),
        }];
        let init = prepare_one_sided_transaction_for_signing(
            &alice_view_key_manager,
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
        assert!(
            alice_view_key_manager
                .is_this_output_ours(&change_output.commitment, &change_output.encrypted_data, None,)
                .unwrap()
        );
        // lets test the hot wallet
        assert!(
            alice_key_manager
                .is_this_output_ours(&change_output.commitment, &change_output.encrypted_data, None,)
                .unwrap()
        );

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
        assert_eq!(res.2, payment_id_bob);
    }

    #[test]
    fn view_only_cannot_sign_offline() {
        let rules = create_consensus_manager();
        let alice_key_manager = KeyManager::new_random().unwrap();
        let alice_keys = ViewWallet::new(
            alice_key_manager.get_spend_key().pub_key,
            alice_key_manager.get_private_view_key(),
            None,
        );
        let alice_view_key_manager = create_view_key_manager(alice_keys).unwrap();

        let bob_key_manager = KeyManager::new_random().unwrap();

        let input = create_test_input(MicroMinotari(10000), 0, &alice_key_manager, vec![], None);
        let input2 = create_test_input(MicroMinotari(2000), 0, &alice_key_manager, vec![], None);
        let input3 = create_test_input(MicroMinotari(15000), 0, &alice_key_manager, vec![], None);
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
            &alice_view_key_manager,
            TxId::new_random(),
            tx_builder,
            &recipients,
            payment_id,
            alice_address,
        )
        .unwrap();

        assert_eq!(init.info.inputs.len(), 3);
        assert_eq!(init.info.outputs.len(), 0);

        assert!(
            sign_locked_transaction(
                &alice_view_key_manager,
                rules.consensus_constants(0).clone(),
                Network::LocalNet,
                init
            )
            .is_err()
        );
    }

    // -----------------------------------------------------------------------
    // Payload integrity tests (issue #7796)
    // -----------------------------------------------------------------------

    /// Verify that `sign_locked_transaction` rejects a payload whose recipient
    /// address was swapped after `prepare_one_sided_transaction_for_signing`
    /// produced the integrity signature.  This is the exact attack vector
    /// from the disclosure report: MITM swaps `recipients[0].address` from
    /// Bob to Mallory, then hands the tampered JSON to the offline signer.
    #[test]
    fn sign_locked_transaction_rejects_tampered_recipient() {
        let rules = create_consensus_manager();
        let alice_key_manager = KeyManager::new_random().unwrap();
        let alice_keys = ViewWallet::new(
            alice_key_manager.get_spend_key().pub_key,
            alice_key_manager.get_private_view_key(),
            None,
        );
        let alice_view_key_manager = create_view_key_manager(alice_keys).unwrap();

        let bob_key_manager = KeyManager::new_random().unwrap();
        let mallory_key_manager = KeyManager::new_random().unwrap();

        let input = create_test_input(MicroMinotari(10000), 0, &alice_key_manager, vec![], None);
        let input2 = create_test_input(MicroMinotari(10000), 0, &alice_key_manager, vec![], None);
        let mut tx_builder = TransactionBuilder::new(
            rules.consensus_constants(0).clone(),
            alice_view_key_manager.clone(),
            Network::LocalNet,
        )
        .unwrap();
        tx_builder
            .with_fee_per_gram(MicroMinotari(20))
            .with_input(input)
            .unwrap()
            .with_input(input2)
            .unwrap();

        let bob_address = TariAddress::new_dual_address(
            bob_key_manager.get_view_key().pub_key,
            bob_key_manager.get_spend_key().pub_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();
        let mallory_address = TariAddress::new_dual_address(
            mallory_key_manager.get_view_key().pub_key,
            mallory_key_manager.get_spend_key().pub_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();
        let alice_address = TariAddress::new_dual_address(
            alice_view_key_manager.get_view_key().pub_key,
            alice_view_key_manager.get_spend_key().pub_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();

        let recipients = [PaymentRecipient {
            amount: MicroMinotari(5000),
            output_features: OutputFeatures::default(),
            address: bob_address,
            payment_id: MemoField::new_empty(),
        }];

        // Prepare the transaction — the payload is signed by alice's view key.
        let mut prepared = prepare_one_sided_transaction_for_signing(
            &alice_view_key_manager,
            TxId::new_random(),
            tx_builder,
            &recipients,
            MemoField::new_empty(),
            alice_address,
        )
        .unwrap();

        // Simulate MITM: swap recipient from Bob to Mallory.
        prepared.info.recipients[0].address = mallory_address;

        // The offline signer must detect the tamper and refuse to sign.
        let result = sign_locked_transaction(
            &alice_key_manager,
            rules.consensus_constants(0).clone(),
            Network::LocalNet,
            prepared,
        );
        assert!(
            result.is_err(),
            "sign_locked_transaction should reject a payload with a tampered recipient"
        );
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("tampered") || err_msg.contains("integrity") || err_msg.contains("invalid"),
            "Expected an integrity-check error message, got: {err_msg}"
        );
    }

    /// Verify that a payload signed by a *different* wallet's view key is
    /// rejected even if the payload content is otherwise intact.  This
    /// prevents a payload produced by wallet A from being signed by wallet B's
    /// offline signer.
    #[test]
    fn sign_locked_transaction_rejects_wrong_view_key() {
        let rules = create_consensus_manager();
        // Alice prepares the transaction.
        let alice_key_manager = KeyManager::new_random().unwrap();
        let alice_keys = ViewWallet::new(
            alice_key_manager.get_spend_key().pub_key,
            alice_key_manager.get_private_view_key(),
            None,
        );
        let alice_view_key_manager = create_view_key_manager(alice_keys).unwrap();

        // Bob is an unrelated wallet that happens to receive the JSON.
        let bob_key_manager = KeyManager::new_random().unwrap();

        let input = create_test_input(MicroMinotari(10000), 0, &alice_key_manager, vec![], None);
        let mut tx_builder = TransactionBuilder::new(
            rules.consensus_constants(0).clone(),
            alice_view_key_manager.clone(),
            Network::LocalNet,
        )
        .unwrap();
        tx_builder
            .with_fee_per_gram(MicroMinotari(20))
            .with_input(input)
            .unwrap();

        let recipient_address = TariAddress::new_dual_address(
            bob_key_manager.get_view_key().pub_key,
            bob_key_manager.get_spend_key().pub_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();
        let alice_address = TariAddress::new_dual_address(
            alice_view_key_manager.get_view_key().pub_key,
            alice_view_key_manager.get_spend_key().pub_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();

        let recipients = [PaymentRecipient {
            amount: MicroMinotari(1000),
            output_features: OutputFeatures::default(),
            address: recipient_address,
            payment_id: MemoField::new_empty(),
        }];

        // Alice prepares the payload (signed with alice's view key).
        let prepared = prepare_one_sided_transaction_for_signing(
            &alice_view_key_manager,
            TxId::new_random(),
            tx_builder,
            &recipients,
            MemoField::new_empty(),
            alice_address,
        )
        .unwrap();

        // Bob's offline signer receives this JSON.  Bob's view key does not
        // match alice's view key in the payload — this must be rejected.
        let result = sign_locked_transaction(
            &bob_key_manager,
            rules.consensus_constants(0).clone(),
            Network::LocalNet,
            prepared,
        );
        assert!(
            result.is_err(),
            "sign_locked_transaction must reject a payload whose view key does not match the signer's wallet"
        );
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("view public key") || err_msg.contains("integrity") || err_msg.contains("tampered"),
            "Expected a view-key mismatch error, got: {err_msg}"
        );
    }
}
