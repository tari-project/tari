// Copyright 2019. The Tari Project
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

use crate::{
    consensus::ConsensusConstants,
    transactions::{
        key_manager::{TransactionKeyManagerBranch, TransactionKeyManagerInterface, TxoStage},
        transaction_components::{TransactionKernel, WalletOutput},
        transaction_protocol::{
            recipient::RecipientSignedMessage,
            sender::SingleRoundSenderData,
            TransactionProtocolError as TPE,
        },
    },
};

/// SingleReceiverTransactionProtocol represents the actions taken by the single receiver in the one-round Tari
/// transaction protocol. The procedure is straightforward. Upon receiving the sender's information, the receiver:
/// * Checks the input for validity
/// * Constructs his output, range proof and partial signature
/// * Constructs the reply
/// If any step fails, an error is returned.
pub struct SingleReceiverTransactionProtocol {}

impl SingleReceiverTransactionProtocol {
    pub async fn create<KM: TransactionKeyManagerInterface>(
        sender_info: &SingleRoundSenderData,
        output: WalletOutput,
        key_manager: &KM,
        consensus_constants: &ConsensusConstants,
    ) -> Result<RecipientSignedMessage, TPE> {
        SingleReceiverTransactionProtocol::validate_sender_data(sender_info, consensus_constants)?;
        let transaction_output = output.to_transaction_output(key_manager).await?;

        let (nonce_id, public_nonce) = key_manager
            .get_next_key(TransactionKeyManagerBranch::KernelNonce.get_branch_key())
            .await?;
        let tx_meta = if output.is_burned() {
            let mut meta = sender_info.metadata.clone();
            meta.burn_commitment = Some(transaction_output.commitment().clone());
            meta
        } else {
            sender_info.metadata.clone()
        };
        let public_excess = key_manager
            .get_txo_kernel_signature_excess_with_offset(&output.spending_key_id, &nonce_id)
            .await?;

        let kernel_message = TransactionKernel::build_kernel_signature_message(
            &sender_info.kernel_version,
            tx_meta.fee,
            tx_meta.lock_height,
            &tx_meta.kernel_features,
            &tx_meta.burn_commitment,
        );
        let signature = key_manager
            .get_partial_txo_kernel_signature(
                &output.spending_key_id,
                &nonce_id,
                &(&sender_info.public_nonce + &public_nonce),
                &(&sender_info.public_excess + &public_excess),
                &sender_info.kernel_version,
                &kernel_message,
                &tx_meta.kernel_features,
                TxoStage::Output,
            )
            .await?;
        let offset = key_manager
            .get_txo_private_kernel_offset(&output.spending_key_id, &nonce_id)
            .await?;

        let data = RecipientSignedMessage {
            tx_id: sender_info.tx_id,
            output: transaction_output,
            public_spend_key: public_excess,
            partial_signature: signature,
            tx_metadata: tx_meta,
            offset,
        };
        Ok(data)
    }

    /// Validates the sender info
    fn validate_sender_data(
        sender_info: &SingleRoundSenderData,
        consensus_constants: &ConsensusConstants,
    ) -> Result<(), TPE> {
        // validate amount
        if sender_info.amount == 0.into() {
            return Err(TPE::ValidationError("Cannot send zero micro Minotari".into()));
        }

        // validate kernel version
        if !consensus_constants
            .kernel_version_range()
            .contains(&sender_info.kernel_version)
        {
            let msg = format!(
                "Transaction kernel version is not allowed by consensus ({:?})",
                &sender_info.kernel_version
            );
            return Err(TPE::ValidationError(msg));
        }

        // validate output version
        if !consensus_constants
            .output_version_range()
            .outputs
            .contains(&sender_info.output_version)
        {
            let msg = format!(
                "Transaction output version is not allowed by consensus ({:?})",
                &sender_info.output_version
            );
            return Err(TPE::ValidationError(msg));
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use tari_common_types::types::PublicKey;
    use tari_crypto::{keys::PublicKey as PublicKeyTrait, signatures::CommitmentAndPublicKeySignature};
    use tari_key_manager::key_manager_service::KeyManagerInterface;
    use tari_script::{script, ExecutionStack};

    use crate::{
        covenants::Covenant,
        test_helpers::create_consensus_constants,
        transactions::{
            key_manager::TransactionKeyManagerInterface,
            tari_amount::*,
            test_helpers::{create_test_core_key_manager_with_memory_db, TestParams},
            transaction_components::{
                EncryptedData,
                OutputFeatures,
                TransactionKernel,
                TransactionKernelVersion,
                TransactionOutput,
                TransactionOutputVersion,
                WalletOutput,
            },
            transaction_protocol::{
                sender::SingleRoundSenderData,
                single_receiver::SingleReceiverTransactionProtocol,
                TransactionMetadata,
                TransactionProtocolError,
            },
        },
    };

    #[tokio::test]
    async fn zero_amount_fails() {
        let key_manager = create_test_core_key_manager_with_memory_db();
        let test_params = TestParams::new(&key_manager).await;
        let consensus_constants = create_consensus_constants(0);
        let info = SingleRoundSenderData::default();
        let bob_output = WalletOutput::new_current_version(
            MicroMinotari(5000),
            test_params.spend_key_id,
            OutputFeatures::default(),
            script!(Nop),
            ExecutionStack::default(),
            test_params.script_key_id,
            PublicKey::default(),
            CommitmentAndPublicKeySignature::default(),
            0,
            Covenant::default(),
            EncryptedData::default(),
            0.into(),
            &key_manager,
        )
        .await
        .unwrap();

        #[allow(clippy::match_wild_err_arm)]
        match SingleReceiverTransactionProtocol::create(&info, bob_output, &key_manager, &consensus_constants).await {
            Ok(_) => panic!("Zero amounts should fail"),
            Err(TransactionProtocolError::ValidationError(s)) => assert_eq!(s, "Cannot send zero micro Minotari"),
            Err(_) => panic!("Protocol fails for the wrong reason"),
        };
    }

    #[tokio::test]
    async fn invalid_version_fails() {
        let key_manager = create_test_core_key_manager_with_memory_db();
        let test_params = TestParams::new(&key_manager).await;
        let consensus_constants = create_consensus_constants(0);

        // let's use a sender's output version (V1) outside of the allowed range used by the receiver (V0..V0 by
        // default)
        let info = SingleRoundSenderData {
            amount: MicroMinotari(5000),
            output_version: TransactionOutputVersion::V1,
            ..Default::default()
        };

        let bob_output = WalletOutput::new_current_version(
            MicroMinotari(5000),
            test_params.spend_key_id,
            OutputFeatures::default(),
            script!(Nop),
            ExecutionStack::default(),
            test_params.script_key_id,
            PublicKey::default(),
            CommitmentAndPublicKeySignature::default(),
            0,
            Covenant::default(),
            EncryptedData::default(),
            0.into(),
            &key_manager,
        )
        .await
        .unwrap();

        #[allow(clippy::match_wild_err_arm)]
        match SingleReceiverTransactionProtocol::create(&info, bob_output, &key_manager, &consensus_constants).await {
            Ok(_) => panic!("Invalid version should fail"),
            Err(TransactionProtocolError::ValidationError(s)) => {
                assert_eq!(s, "Transaction output version is not allowed by consensus (V1)")
            },
            Err(_) => panic!("Protocol fails for the wrong reason"),
        };
    }

    #[tokio::test]
    async fn valid_request() {
        let key_manager: crate::transactions::key_manager::TransactionKeyManagerWrapper<
            tari_key_manager::key_manager_service::storage::sqlite_db::KeyManagerSqliteDatabase<
                tari_common_sqlite::connection::DbConnection,
            >,
        > = create_test_core_key_manager_with_memory_db();
        let consensus_constants = create_consensus_constants(0);
        let m = TransactionMetadata::new(MicroMinotari(100), 0);
        let test_params = TestParams::new(&key_manager).await;
        let test_params2 = TestParams::new(&key_manager).await;
        let script = script!(Nop);
        let sender_offset_public_key = key_manager
            .get_public_key_at_key_id(&test_params.sender_offset_key_id)
            .await
            .unwrap();
        let ephemeral_public_nonce = key_manager
            .get_public_key_at_key_id(&test_params.kernel_nonce_key_id)
            .await
            .unwrap();
        let pub_xs = key_manager
            .get_public_key_at_key_id(&test_params.spend_key_id)
            .await
            .unwrap();
        let pub_rs = key_manager
            .get_public_key_at_key_id(&test_params.kernel_nonce_key_id)
            .await
            .unwrap();
        let info = SingleRoundSenderData {
            tx_id: 500u64.into(),
            amount: MicroMinotari(1500),
            public_excess: pub_xs.clone(),
            public_nonce: pub_rs.clone(),
            metadata: m.clone(),
            message: "".to_string(),
            features: OutputFeatures::default(),
            script: script.clone(),
            sender_offset_public_key,
            ephemeral_public_nonce: ephemeral_public_nonce.clone(),
            covenant: Default::default(),
            minimum_value_promise: MicroMinotari::zero(),
            output_version: TransactionOutputVersion::get_current_version(),
            kernel_version: TransactionKernelVersion::get_current_version(),
        };
        let bob_public_key = key_manager
            .get_public_key_at_key_id(&test_params.sender_offset_key_id)
            .await
            .unwrap();
        let mut bob_output = WalletOutput::new_current_version(
            MicroMinotari(1500),
            test_params2.spend_key_id.clone(),
            OutputFeatures::default(),
            script.clone(),
            ExecutionStack::default(),
            test_params2.script_key_id,
            bob_public_key,
            CommitmentAndPublicKeySignature::default(),
            0,
            Covenant::default(),
            EncryptedData::default(),
            0.into(),
            &key_manager,
        )
        .await
        .unwrap();
        let metadata_message = TransactionOutput::metadata_signature_message(&bob_output);
        bob_output.metadata_signature = key_manager
            .get_receiver_partial_metadata_signature(
                &bob_output.spending_key_id,
                &bob_output.value.into(),
                &bob_output.sender_offset_public_key,
                &ephemeral_public_nonce,
                &bob_output.version,
                &metadata_message,
                bob_output.features.range_proof_type,
            )
            .await
            .unwrap();

        let prot = SingleReceiverTransactionProtocol::create(&info, bob_output, &key_manager, &consensus_constants)
            .await
            .unwrap();
        assert_eq!(prot.tx_id.as_u64(), 500, "tx_id is incorrect");
        // Check the signature

        let pubkey = key_manager
            .get_public_key_at_key_id(&test_params2.spend_key_id)
            .await
            .unwrap();
        let offset = prot.offset.clone();
        let public_offset = PublicKey::from_secret_key(&offset);
        let signing_pubkey = &pubkey - &public_offset;
        assert_eq!(prot.public_spend_key, signing_pubkey, "Public key is incorrect");
        let excess = &pub_xs + &signing_pubkey;
        let e = TransactionKernel::build_kernel_challenge_from_tx_meta(
            &TransactionKernelVersion::get_current_version(),
            &(&pub_rs + prot.partial_signature.get_public_nonce()),
            &excess,
            &m,
        );
        assert!(
            prot.partial_signature.verify_challenge(&signing_pubkey, &e),
            "Partial signature is incorrect"
        );
    }
}
