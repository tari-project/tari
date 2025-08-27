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

#[cfg(test)]
mod test {
    #![allow(clippy::indexing_slicing)]
    use std::sync::Arc;

    use argon2::password_hash::rand_core::OsRng;
    use chacha20poly1305::Key;
    use rand::RngCore;
    use tari_common::configuration::Network;
    use tari_common_types::{
        seeds::cipher_seed::CipherSeed,
        tari_address::{TariAddress, TariAddressFeatures},
        transaction::TxId,
        wallet_types::{ProvidedKeysWallet, WalletType},
    };
    use tari_transaction_components::{
        crypto_factories::CryptoFactories,
        key_manager::{
            create_memory_key_manager,
            error::KeyManagerServiceError,
            memory_key_manager::MemoryKeyManagerBackend,
            MemoryKeyManager,
            TransactionKeyManagerInterface,
            TransactionKeyManagerWrapper,
        },
        tari_amount::MicroMinotari,
        test_helpers::{create_consensus_manager, create_test_input},
        transaction_components::{
            memo_field::MemoField,
            one_sided::shared_secret_to_output_encryption_key,
            EncryptedData,
            OutputFeatures,
        },
        validation::transaction::TransactionInternalConsistencyValidator,
        TransactionBuilder,
    };
    use zeroize::Zeroizing;

    use crate::transaction_service::offline_signing::offline_signer::OfflineSigner;

    async fn create_view_key_manager(keys: ProvidedKeysWallet) -> Result<MemoryKeyManager, KeyManagerServiceError> {
        let cipher = CipherSeed::new();
        let mut key = Zeroizing::new([0u8; size_of::<Key>()]);
        OsRng.fill_bytes(key.as_mut());
        let factory = CryptoFactories::new(64);

        let backend = MemoryKeyManagerBackend::new();
        TransactionKeyManagerWrapper::new(cipher, backend, factory, Arc::new(WalletType::ProvidedKeys(keys))).await
    }
    #[tokio::test]
    async fn offline_sign_is_valid() {
        let rules = create_consensus_manager();
        let alice_key_manager = create_memory_key_manager().await.unwrap();
        let keys = ProvidedKeysWallet {
            public_spend_key: alice_key_manager.get_spend_key().await.unwrap().pub_key,
            private_spend_key: None,
            private_comms_key: None,
            view_key: alice_key_manager.get_private_view_key().await.unwrap(),
            birthday: None,
        };
        let alice_view_key_manager = create_view_key_manager(keys).await.unwrap();
        let bob_key_manager = create_memory_key_manager().await.unwrap();

        let input = create_test_input(MicroMinotari(10000), 0, &alice_view_key_manager, vec![], None).await;
        let input2 = create_test_input(MicroMinotari(2000), 0, &alice_view_key_manager, vec![], None).await;
        let input3 = create_test_input(MicroMinotari(15000), 0, &alice_view_key_manager, vec![], None).await;
        // this replicates the behaviour od the oms that selects the inputs and starts the build tx process.
        let mut tx_builder = TransactionBuilder::new(
            rules.consensus_constants(0).clone(),
            alice_view_key_manager.clone(),
            Network::LocalNet,
        )
        .await
        .unwrap();
        tx_builder
            .with_lock_height(0)
            .with_fee_per_gram(MicroMinotari(20))
            .with_input(input)
            .await
            .unwrap()
            .with_input(input2)
            .await
            .unwrap()
            .with_input(input3)
            .await
            .unwrap();

        // now we start the offline process
        let mut offline_signing = OfflineSigner::new(alice_view_key_manager.clone());
        let tx_id = TxId::new_random();
        let payment_id = MemoField::new_empty();
        let output_features = OutputFeatures::default();
        let amount = MicroMinotari(5000);

        let spend_key = bob_key_manager.get_spend_key().await.unwrap().pub_key;
        let view_key = bob_key_manager.get_view_key().await.unwrap().pub_key;
        let bob_address = TariAddress::new_dual_address(
            view_key,
            spend_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();
        let spend_key = alice_view_key_manager.get_spend_key().await.unwrap().pub_key;
        let view_key = alice_view_key_manager.get_view_key().await.unwrap().pub_key;
        let alice_address = TariAddress::new_dual_address(
            view_key,
            spend_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();

        let spend_key = alice_key_manager.get_spend_key().await.unwrap().pub_key;
        let view_key = alice_key_manager.get_view_key().await.unwrap().pub_key;
        let alice_address_s = TariAddress::new_dual_address(
            view_key,
            spend_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();

        assert_eq!(alice_address, alice_address_s);

        let init = offline_signing
            .prepare_one_sided_transaction_for_signing(
                tx_id,
                tx_builder,
                bob_address,
                amount,
                output_features,
                payment_id,
                alice_address,
            )
            .await
            .unwrap();

        assert!(init.info.change_output.is_some());
        assert_eq!(init.info.metadata.fee, MicroMinotari(2960));
        assert_eq!(init.info.inputs.len(), 3);
        assert_eq!(init.info.outputs.len(), 0);

        let signer = OfflineSigner::new(alice_key_manager.clone());
        let signed = signer.sign_locked_transaction(init).await.unwrap();
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
    async fn offline_sign_can_be_claimed() {
        let rules = create_consensus_manager();
        let alice_key_manager = create_memory_key_manager().await.unwrap();
        let keys = ProvidedKeysWallet {
            public_spend_key: alice_key_manager.get_spend_key().await.unwrap().pub_key,
            private_spend_key: None,
            private_comms_key: None,
            view_key: alice_key_manager.get_private_view_key().await.unwrap(),
            birthday: None,
        };
        let alice_view_key_manager = create_view_key_manager(keys).await.unwrap();
        let bob_key_manager = create_memory_key_manager().await.unwrap();

        let input = create_test_input(MicroMinotari(100000), 0, &alice_view_key_manager, vec![], None).await;
        // this replicates the behaviour od the oms that selects the inputs and starts the build tx process.
        let mut tx_builder = TransactionBuilder::new(
            rules.consensus_constants(0).clone(),
            alice_view_key_manager.clone(),
            Network::LocalNet,
        )
        .await
        .unwrap();
        tx_builder
            .with_lock_height(0)
            .with_fee_per_gram(MicroMinotari(20))
            .with_input(input)
            .await
            .unwrap();

        // now we start the offline process
        let mut offline_signing = OfflineSigner::new(alice_view_key_manager.clone());
        let spend_key = bob_key_manager.get_spend_key().await.unwrap().pub_key;
        let view_key = bob_key_manager.get_view_key().await.unwrap().pub_key;
        let bob_address = TariAddress::new_dual_address(
            view_key,
            spend_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();
        let spend_key = alice_view_key_manager.get_spend_key().await.unwrap().pub_key;
        let view_key = alice_view_key_manager.get_view_key().await.unwrap().pub_key;
        let alice_address = TariAddress::new_dual_address(
            view_key,
            spend_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();

        let init = offline_signing
            .prepare_one_sided_transaction_for_signing(
                TxId::new_random(),
                tx_builder,
                bob_address,
                MicroMinotari(5000),
                OutputFeatures::default(),
                MemoField::new_empty(),
                alice_address,
            )
            .await
            .unwrap();

        let signer = OfflineSigner::new(alice_key_manager.clone());
        let signed = signer.sign_locked_transaction(init).await.unwrap();
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
            .await
            .unwrap());
        // lets test the hot wallet
        assert!(alice_key_manager
            .is_this_output_ours(&change_output.commitment, &change_output.encrypted_data, None,)
            .await
            .unwrap());

        // lets see if bob's wallet can claim the sent:
        let sent_output = &signed.signed_transaction.transaction.body.outputs()[sent_index].clone();
        let view_key = bob_key_manager.get_view_key().await.unwrap();
        let shared_secret = bob_key_manager
            .get_diffie_hellman_shared_secret(&view_key.key_id, &sent_output.sender_offset_public_key)
            .await
            .unwrap();

        let recovery_key = shared_secret_to_output_encryption_key(&shared_secret).unwrap();
        let res =
            EncryptedData::decrypt_data(&recovery_key, &sent_output.commitment, &sent_output.encrypted_data).unwrap();
        assert_eq!(res.0, MicroMinotari(5000));
    }

    #[tokio::test]
    async fn view_only_cannot_sign_offline() {
        let rules = create_consensus_manager();
        let alice_key_manager = create_memory_key_manager().await.unwrap();
        let keys = ProvidedKeysWallet {
            public_spend_key: alice_key_manager.get_spend_key().await.unwrap().pub_key,
            private_spend_key: None,
            private_comms_key: None,
            view_key: alice_key_manager.get_private_view_key().await.unwrap(),
            birthday: None,
        };
        let alice_view_key_manager = create_view_key_manager(keys).await.unwrap();
        let bob_key_manager = create_memory_key_manager().await.unwrap();

        let input = create_test_input(MicroMinotari(10000), 0, &alice_view_key_manager, vec![], None).await;
        let input2 = create_test_input(MicroMinotari(2000), 0, &alice_view_key_manager, vec![], None).await;
        let input3 = create_test_input(MicroMinotari(15000), 0, &alice_view_key_manager, vec![], None).await;
        // this replicates the behaviour od the oms that selects the inputs and starts the build tx process.
        let mut tx_builder = TransactionBuilder::new(
            rules.consensus_constants(0).clone(),
            alice_view_key_manager.clone(),
            Network::LocalNet,
        )
        .await
        .unwrap();
        tx_builder
            .with_lock_height(0)
            .with_fee_per_gram(MicroMinotari(20))
            .with_input(input)
            .await
            .unwrap()
            .with_input(input2)
            .await
            .unwrap()
            .with_input(input3)
            .await
            .unwrap();

        // now we start the offline process
        let mut offline_signing = OfflineSigner::new(alice_view_key_manager.clone());
        let tx_id = TxId::new_random();
        let payment_id = MemoField::new_empty();
        let output_features = OutputFeatures::default();
        let amount = MicroMinotari(5000);

        let spend_key = bob_key_manager.get_spend_key().await.unwrap().pub_key;
        let view_key = bob_key_manager.get_view_key().await.unwrap().pub_key;
        let bob_address = TariAddress::new_dual_address(
            view_key,
            spend_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();
        let spend_key = alice_view_key_manager.get_spend_key().await.unwrap().pub_key;
        let view_key = alice_view_key_manager.get_view_key().await.unwrap().pub_key;
        let alice_address = TariAddress::new_dual_address(
            view_key,
            spend_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();

        let spend_key = alice_key_manager.get_spend_key().await.unwrap().pub_key;
        let view_key = alice_key_manager.get_view_key().await.unwrap().pub_key;
        let alice_address_s = TariAddress::new_dual_address(
            view_key,
            spend_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();

        assert_eq!(alice_address, alice_address_s);

        let init = offline_signing
            .prepare_one_sided_transaction_for_signing(
                tx_id,
                tx_builder,
                bob_address,
                amount,
                output_features,
                payment_id,
                alice_address,
            )
            .await
            .unwrap();

        assert!(init.info.change_output.is_some());
        assert_eq!(init.info.metadata.fee, MicroMinotari(2960));
        assert_eq!(init.info.inputs.len(), 3);
        assert_eq!(init.info.outputs.len(), 0);

        let signer = OfflineSigner::new(alice_view_key_manager.clone());
        let _signed = signer.sign_locked_transaction(init).await.is_err();
    }
}
