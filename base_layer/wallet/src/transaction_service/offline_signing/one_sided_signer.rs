use tari_common_types::{
    key_branches::TransactionKeyManagerBranch,
    transaction::TxId,
    types::{CompressedCommitment, CompressedPublicKey, FixedHash, Signature, UncompressedPublicKey},
};
use tari_core::{
    consensus::ConsensusManager,
    one_sided::{shared_secret_to_output_encryption_key, shared_secret_to_output_spending_key},
    transactions::{
        tari_amount::MicroMinotari,
        transaction_components::{
            KernelBuilder,
            Transaction,
            TransactionBuilder,
            TransactionKernel,
            TransactionKernelVersion,
            TransactionOutputVersion,
            WalletOutputBuilder,
        },
        transaction_key_manager::{TariKeyId, TransactionKeyManagerInterface, TxoStage},
        transaction_protocol::{
            recipient::RecipientSignedMessage,
            sender::SingleRoundSenderData,
            single_receiver::SingleReceiverTransactionProtocol,
            TransactionProtocolError as TPE,
        },
    },
};
use tari_script::push_pubkey_script;

use crate::transaction_service::{
    error::{TransactionServiceError, TransactionServiceProtocolError},
    offline_signing::models::{OneSidedTransactionInfo, SignedTransaction},
};

struct SignedMessage {
    pub signed_data: RecipientSignedMessage,
    pub sender_info: SingleRoundSenderData,
    pub sender_offset_key_id: TariKeyId,
    pub sent_hashes: Vec<FixedHash>,
    pub change_hashes: Vec<FixedHash>,
}

pub struct OneSidedSigner<'a, KM: TransactionKeyManagerInterface> {
    key_manager: &'a KM,
    consensus_manager: &'a ConsensusManager,
}

impl<'a, KM: TransactionKeyManagerInterface> OneSidedSigner<'a, KM> {
    pub fn new(key_manager: &'a KM, consensus_manager: &'a ConsensusManager) -> Self {
        Self {
            key_manager,
            consensus_manager,
        }
    }

    pub async fn sign_transaction(
        &self,
        tx_id: TxId,
        mut info: OneSidedTransactionInfo,
    ) -> Result<SignedTransaction, TransactionServiceError> {
        self.import_input_script_signatures(&mut info).await?;
        let signed_message = self.sign_message(tx_id, &info).await?;
        let transaction = self
            .build_transaction(
                &info,
                &signed_message.signed_data,
                signed_message.sender_offset_key_id,
                &signed_message.sender_info,
            )
            .await?;
        Ok(SignedTransaction {
            transaction,
            sent_hashes: signed_message.sent_hashes,
            change_hashes: signed_message.change_hashes,
        })
    }

    async fn import_input_script_signatures(&self, info: &mut OneSidedTransactionInfo) -> Result<(), TPE> {
        let mut commitment_mask_key_ids = Vec::new();
        for encrypted_key in &info.encrypted_commitment_mask_keys {
            commitment_mask_key_ids.push(
                self.key_manager
                    .import_encrypted_key(encrypted_key.clone(), None)
                    .await?,
            );
        }

        if info.inputs.len() != commitment_mask_key_ids.len() {
            return Err(TPE::ValidationError(format!(
                "Mismatch between inputs count ({}) and commitment mask key IDs count ({})",
                info.inputs.len(),
                commitment_mask_key_ids.len()
            )));
        }
        for (input, spending_key_id) in info.inputs.iter_mut().zip(commitment_mask_key_ids.into_iter()) {
            let script_signature = input
                .output
                .build_script_signature(self.key_manager, Some(&spending_key_id))
                .await?;
            input.output.script_signature = Some(script_signature);
        }
        Ok(())
    }

    async fn calculate_total_nonce_and_total_public_excess(
        &self,
        info: &OneSidedTransactionInfo,
    ) -> Result<(CompressedPublicKey, CompressedPublicKey), TPE> {
        let mut public_nonce = UncompressedPublicKey::default();
        let mut public_excess = UncompressedPublicKey::default();
        for input in &info.inputs {
            public_nonce = public_nonce +
                self.key_manager
                    .get_public_key_at_key_id(&input.kernel_nonce)
                    .await?
                    .to_public_key()?;
            public_excess = public_excess -
                self.key_manager
                    .get_txo_kernel_signature_excess_with_offset(&input.output.spending_key_id, &input.kernel_nonce)
                    .await?
                    .to_public_key()?;
        }
        for output in &info.outputs {
            public_nonce = public_nonce +
                self.key_manager
                    .get_public_key_at_key_id(&output.kernel_nonce)
                    .await?
                    .to_public_key()?;
            public_excess = public_excess +
                self.key_manager
                    .get_txo_kernel_signature_excess_with_offset(&output.output.spending_key_id, &output.kernel_nonce)
                    .await?
                    .to_public_key()?;
        }

        if let Some(change) = &info.change_output {
            public_nonce = public_nonce +
                self.key_manager
                    .get_public_key_at_key_id(&change.output.kernel_nonce)
                    .await?
                    .to_public_key()?;
            public_excess = public_excess +
                self.key_manager
                    .get_txo_kernel_signature_excess_with_offset(
                        &change.output.output.spending_key_id,
                        &change.output.kernel_nonce,
                    )
                    .await?
                    .to_public_key()?;
        }
        Ok((
            CompressedPublicKey::new_from_pk(public_nonce),
            CompressedPublicKey::new_from_pk(public_excess),
        ))
    }

    async fn sign_message(
        &self,
        tx_id: TxId,
        info: &OneSidedTransactionInfo,
    ) -> Result<SignedMessage, TransactionServiceError> {
        let sender_offset_key = self
            .key_manager
            .get_next_key(TransactionKeyManagerBranch::OneSidedSenderOffset.get_branch_key())
            .await?;
        let shared_secret = self
            .key_manager
            .get_diffie_hellman_shared_secret(
                &sender_offset_key.key_id,
                info.recipient
                    .address
                    .public_view_key()
                    .ok_or(TransactionServiceProtocolError::new(
                        tx_id,
                        TransactionServiceError::OneSidedTransactionError("Missing public view key".to_string()),
                    ))?,
            )
            .await?;
        let commitment_mask_private_key = shared_secret_to_output_spending_key(&shared_secret)
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;
        let commitment_mask_key_id = self.key_manager.import_key(commitment_mask_private_key.clone()).await?;

        let encryption_private_key = shared_secret_to_output_encryption_key(&shared_secret)?;
        let encryption_key = self.key_manager.import_key(encryption_private_key).await?;

        let sender_offset_public_key = self
            .key_manager
            .get_public_key_at_key_id(&sender_offset_key.key_id)
            .await?;

        let minimum_value_promise = MicroMinotari::zero();
        let mut script = info.recipient.script.clone();
        if info.recipient.use_stealth_address {
            let script_spending_key = self
                .key_manager
                .stealth_address_script_spending_key(&commitment_mask_key_id, info.recipient.address.public_spend_key())
                .await?;
            script = push_pubkey_script(&script_spending_key);
        }

        let output = WalletOutputBuilder::new(info.recipient.amount, commitment_mask_key_id.clone())
            .with_features(info.recipient.output_features.clone())
            .with_script(script.clone())
            .encrypt_data_for_recovery(self.key_manager, Some(&encryption_key), info.payment_id.clone())
            .await?
            .with_input_data(Default::default())
            .with_sender_offset_public_key(sender_offset_public_key)
            .with_script_key(TariKeyId::Zero)
            .with_minimum_value_promise(minimum_value_promise)
            .sign_as_sender_and_receiver_verified(self.key_manager, &sender_offset_key.key_id, &info.recipient.address)
            .await?
            .try_build(self.key_manager)
            .await?;

        let sent_hashes = vec![output.hash(self.key_manager).await?];
        let change_hashes = match &info.change_output {
            Some(change_output) => vec![change_output.output.output.hash(self.key_manager).await?],
            None => vec![],
        };

        let (public_nonce, public_excess) = self.calculate_total_nonce_and_total_public_excess(info).await?;
        let output_version = TransactionOutputVersion::get_current_version();
        let kernel_version = TransactionKernelVersion::get_current_version();
        let sender_info = SingleRoundSenderData {
            tx_id,
            amount: info.recipient.amount,
            public_excess,
            public_nonce,
            metadata: info.metadata.clone(),
            payment_id: info.payment_id.clone(),
            features: info.recipient.output_features.clone(),
            script: info.recipient.script.clone(),
            sender_offset_public_key: info.recipient.sender_offset_public_key.clone(),
            ephemeral_public_nonce: info.recipient.ephemeral_public_key_nonce.clone(),
            covenant: info.recipient.covenant.clone(),
            minimum_value_promise: info.recipient.minimum_value_promise,
            output_version,
            kernel_version,
            sender_address: info.sender_address.clone(),
        };

        let tip_height = info.last_seen_tip_height.unwrap_or(0);
        let consensus_constants = self.consensus_manager.consensus_constants(tip_height);
        let signed_data = SingleReceiverTransactionProtocol::create(
            &sender_info.clone(),
            output,
            self.key_manager,
            consensus_constants,
        )
        .await?;
        Ok(SignedMessage {
            signed_data,
            sender_info,
            sender_offset_key_id: sender_offset_key.key_id,
            sent_hashes,
            change_hashes,
        })
    }

    async fn build_transaction(
        &self,
        info: &OneSidedTransactionInfo,
        signed_message: &RecipientSignedMessage,
        sender_offset_key_id: TariKeyId,
        sender_info: &SingleRoundSenderData,
    ) -> Result<Transaction, TPE> {
        let mut tx_builder = TransactionBuilder::new();

        let total_public_nonce = &sender_info.public_nonce.to_public_key()? +
            signed_message
                .partial_signature
                .get_compressed_public_nonce()
                .to_public_key()?;
        let total_public_excess =
            &sender_info.public_excess.to_public_key()? + &signed_message.public_spend_key.to_public_key()?;
        let total_public_nonce = CompressedPublicKey::new_from_pk(total_public_nonce);
        let total_public_excess = CompressedPublicKey::new_from_pk(total_public_excess);

        let mut offset = signed_message.offset.clone();
        let mut signature = signed_message.partial_signature.clone().to_schnorr_signature()?;
        let mut script_keys = Vec::new();
        let mut sender_offset_keys = Vec::new();
        let kernel_version = TransactionKernelVersion::get_current_version();
        let burn_commitment = if info.metadata.kernel_features.is_burned() {
            Some(signed_message.output.commitment.clone())
        } else {
            info.metadata.burn_commitment.clone()
        };

        let kernel_message = TransactionKernel::build_kernel_signature_message(
            &kernel_version,
            info.metadata.fee,
            info.metadata.lock_height,
            &info.metadata.kernel_features,
            &burn_commitment.clone(),
        );

        for input in &info.inputs {
            tx_builder.add_input(input.output.to_transaction_input(self.key_manager).await?);
            signature = &signature +
                &self
                    .key_manager
                    .get_partial_txo_kernel_signature(
                        &input.output.spending_key_id,
                        &input.kernel_nonce,
                        &total_public_nonce,
                        &total_public_excess,
                        &kernel_version,
                        &kernel_message,
                        &info.metadata.kernel_features,
                        TxoStage::Input,
                    )
                    .await?
                    .to_schnorr_signature()?;
            offset = offset -
                &self
                    .key_manager
                    .get_txo_private_kernel_offset(&input.output.spending_key_id, &input.kernel_nonce)
                    .await?;
            script_keys.push(input.output.script_key_id.clone());
        }

        for output in &info.outputs {
            tx_builder.add_output(output.output.to_transaction_output(self.key_manager).await?);
            signature = &signature +
                &self
                    .key_manager
                    .get_partial_txo_kernel_signature(
                        &output.output.spending_key_id,
                        &output.kernel_nonce,
                        &total_public_nonce,
                        &total_public_excess,
                        &kernel_version,
                        &kernel_message,
                        &info.metadata.kernel_features,
                        TxoStage::Output,
                    )
                    .await?
                    .to_schnorr_signature()?;
            offset = offset +
                &self
                    .key_manager
                    .get_txo_private_kernel_offset(&output.output.spending_key_id, &output.kernel_nonce)
                    .await?;
            let output_sender_offset_key_id = output
                .sender_offset_key_id
                .clone()
                .ok_or_else(|| TPE::IncompleteStateError("Missing sender offset key id".to_string()))?;
            sender_offset_keys.push(output_sender_offset_key_id);
        }

        sender_offset_keys.push(sender_offset_key_id);

        if let Some(change) = &info.change_output {
            tx_builder.add_output(change.output.output.to_transaction_output(self.key_manager).await?);
            signature = &signature +
                &self
                    .key_manager
                    .get_partial_txo_kernel_signature(
                        &change.output.output.spending_key_id,
                        &change.output.kernel_nonce,
                        &total_public_nonce,
                        &total_public_excess,
                        &kernel_version,
                        &kernel_message,
                        &info.metadata.kernel_features,
                        TxoStage::Output,
                    )
                    .await?
                    .to_schnorr_signature()?;
            offset = offset +
                &self
                    .key_manager
                    .get_txo_private_kernel_offset(&change.output.output.spending_key_id, &change.output.kernel_nonce)
                    .await?;
            let sender_offset_key_id = self
                .key_manager
                .import_encrypted_key(change.encrypted_change_sender_offset_key.clone(), None)
                .await?;
            sender_offset_keys.push(sender_offset_key_id);
        }

        tx_builder.add_output(signed_message.output.clone());
        let script_offset = self
            .key_manager
            .get_script_offset(&script_keys, &sender_offset_keys)
            .await?;

        tx_builder.add_offset(offset);
        tx_builder.add_script_offset(script_offset);
        let excess = CompressedCommitment::from_compressed_key(total_public_excess);

        let kernel = KernelBuilder::new()
            .with_fee(info.metadata.fee)
            .with_features(info.metadata.kernel_features)
            .with_lock_height(info.metadata.lock_height)
            .with_burn_commitment(burn_commitment)
            .with_excess(&excess)
            .with_signature(Signature::new_from_schnorr(signature))
            .build()?;
        tx_builder.with_kernel(kernel);
        tx_builder.build().map_err(TPE::from)
    }
}
