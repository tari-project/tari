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
use tari_common_types::{
    key_branches::TransactionKeyManagerBranch,
    transaction::TxId,
    types::{CompressedCommitment, CompressedPublicKey, FixedHash, Signature, UncompressedPublicKey},
};
use tari_core::{
    one_sided::{shared_secret_to_output_encryption_key, shared_secret_to_output_spending_key},
    transactions::{
        tari_amount::MicroMinotari,
        transaction_components::{
            KernelBuilder,
            Transaction,
            TransactionBuilder,
            TransactionKernel,
            TransactionKernelVersion,
            WalletOutput,
            WalletOutputBuilder,
        },
        transaction_key_manager::{TariKeyId, TransactionKeyManagerInterface, TxoStage},
        transaction_protocol::{
            recipient::RecipientSignedMessage,
            sender::OutputPair,
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
    pub sender_public_nonce: CompressedPublicKey,
    pub sender_public_excess: CompressedPublicKey,
    pub sender_offset_key_id: TariKeyId,
    pub sent_hashes: Vec<FixedHash>,
    pub change_hashes: Vec<FixedHash>,
}

pub struct OneSidedSigner<'a, KM: TransactionKeyManagerInterface> {
    key_manager: &'a KM,
}

impl<'a, KM: TransactionKeyManagerInterface> OneSidedSigner<'a, KM> {
    pub fn new(key_manager: &'a KM) -> Self {
        Self { key_manager }
    }

    pub async fn sign_transaction(
        &self,
        tx_id: TxId,
        mut info: OneSidedTransactionInfo,
    ) -> Result<SignedTransaction, TransactionServiceError> {
        self.marshal_output_pairs(&mut info).await?;
        let signed_message = self.sign_message(tx_id, &info).await?;
        let (transaction, change_output) = self
            .build_transaction(
                info,
                signed_message.signed_data,
                signed_message.sender_offset_key_id,
                signed_message.sender_public_nonce,
                signed_message.sender_public_excess,
            )
            .await?;
        Ok(SignedTransaction {
            transaction,
            sent_hashes: signed_message.sent_hashes,
            change_hashes: signed_message.change_hashes,
            change_output,
        })
    }

    async fn marshal_output_pairs(&self, info: &mut OneSidedTransactionInfo) -> Result<(), TransactionServiceError> {
        if let Some(change_output) = &mut info.change_output {
            change_output.unmarshal(self.key_manager).await?;
        }
        for input in &mut info.inputs {
            input.unmarshal(self.key_manager).await?;
        }
        for output in &mut info.outputs {
            output.unmarshal(self.key_manager).await?;
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
                    .get_public_key_at_key_id(&input.output_pair.kernel_nonce)
                    .await?
                    .to_public_key()?;
            public_excess = public_excess -
                self.key_manager
                    .get_txo_kernel_signature_excess_with_offset(
                        &input.output_pair.output.spending_key_id,
                        &input.output_pair.kernel_nonce,
                    )
                    .await?
                    .to_public_key()?;
        }
        for output in &info.outputs {
            public_nonce = public_nonce +
                self.key_manager
                    .get_public_key_at_key_id(&output.output_pair.kernel_nonce)
                    .await?
                    .to_public_key()?;
            public_excess = public_excess +
                self.key_manager
                    .get_txo_kernel_signature_excess_with_offset(
                        &output.output_pair.output.spending_key_id,
                        &output.output_pair.kernel_nonce,
                    )
                    .await?
                    .to_public_key()?;
        }

        if let Some(change) = &info.change_output {
            public_nonce = public_nonce +
                self.key_manager
                    .get_public_key_at_key_id(&change.output_pair.kernel_nonce)
                    .await?
                    .to_public_key()?;
            public_excess = public_excess +
                self.key_manager
                    .get_txo_kernel_signature_excess_with_offset(
                        &change.output_pair.output.spending_key_id,
                        &change.output_pair.kernel_nonce,
                    )
                    .await?
                    .to_public_key()?;
        }
        Ok((
            CompressedPublicKey::new_from_pk(public_nonce),
            CompressedPublicKey::new_from_pk(public_excess),
        ))
    }

    #[allow(clippy::too_many_lines)]
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
        let script_spending_key = self
            .key_manager
            .stealth_address_script_spending_key(&commitment_mask_key_id, info.recipient.address.public_spend_key())
            .await?;
        let script = push_pubkey_script(&script_spending_key);

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
            Some(change_output) => vec![change_output.output_pair.output.hash(self.key_manager).await?],
            None => vec![],
        };

        let (sender_public_nonce, sender_public_excess) =
            self.calculate_total_nonce_and_total_public_excess(info).await?;
        let kernel_version = TransactionKernelVersion::get_current_version();

        let transaction_output = output.to_transaction_output(self.key_manager).await?;
        let public_nonce = self
            .key_manager
            .get_next_key(TransactionKeyManagerBranch::KernelNonce.get_branch_key())
            .await?;
        let tx_meta = if output.is_burned() {
            let mut meta = info.metadata.clone();
            meta.burn_commitment = Some(transaction_output.commitment().clone());
            meta
        } else {
            info.metadata.clone()
        };
        let public_excess = self
            .key_manager
            .get_txo_kernel_signature_excess_with_offset(&output.spending_key_id, &public_nonce.key_id)
            .await?;

        let kernel_message = TransactionKernel::build_kernel_signature_message(
            &kernel_version,
            tx_meta.fee,
            tx_meta.lock_height,
            &tx_meta.kernel_features,
            &tx_meta.burn_commitment,
        );
        let total_nonce = &sender_public_nonce.to_public_key()? + &public_nonce.pub_key.to_public_key()?;
        let total_excess = &sender_public_excess.to_public_key()? + &public_excess.to_public_key()?;
        let signature = self
            .key_manager
            .get_partial_txo_kernel_signature(
                &output.spending_key_id,
                &public_nonce.key_id,
                &CompressedPublicKey::new_from_pk(total_nonce),
                &CompressedPublicKey::new_from_pk(total_excess),
                &kernel_version,
                &kernel_message,
                &tx_meta.kernel_features,
                TxoStage::Output,
            )
            .await?;
        let offset = self
            .key_manager
            .get_txo_private_kernel_offset(&output.spending_key_id, &public_nonce.key_id)
            .await?;

        let signed_data = RecipientSignedMessage {
            tx_id,
            output: transaction_output,
            public_spend_key: public_excess,
            partial_signature: signature,
            tx_metadata: tx_meta,
            offset,
        };

        Ok(SignedMessage {
            signed_data,
            sender_public_nonce,
            sender_public_excess,
            sender_offset_key_id: sender_offset_key.key_id,
            sent_hashes,
            change_hashes,
        })
    }

    #[allow(clippy::too_many_lines)]
    async fn build_transaction(
        &self,
        info: OneSidedTransactionInfo,
        signed_message: RecipientSignedMessage,
        sender_offset_key_id: TariKeyId,
        sender_public_nonce: CompressedPublicKey,
        sender_public_excess: CompressedPublicKey,
    ) -> Result<(Transaction, Option<WalletOutput>), TPE> {
        let mut tx_builder = TransactionBuilder::new();

        let total_public_nonce = &sender_public_nonce.to_public_key()? +
            signed_message
                .partial_signature
                .get_compressed_public_nonce()
                .to_public_key()?;
        let total_public_excess =
            &sender_public_excess.to_public_key()? + &signed_message.public_spend_key.to_public_key()?;
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
            tx_builder.add_input(input.output_pair.output.to_transaction_input(self.key_manager).await?);
            signature = &signature +
                &self
                    .key_manager
                    .get_partial_txo_kernel_signature(
                        &input.output_pair.output.spending_key_id,
                        &input.output_pair.kernel_nonce,
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
                    .get_txo_private_kernel_offset(
                        &input.output_pair.output.spending_key_id,
                        &input.output_pair.kernel_nonce,
                    )
                    .await?;
            script_keys.push(input.output_pair.output.script_key_id.clone());
        }

        for output in &info.outputs {
            tx_builder.add_output(
                output
                    .output_pair
                    .output
                    .to_transaction_output(self.key_manager)
                    .await?,
            );
            signature = &signature +
                &self
                    .key_manager
                    .get_partial_txo_kernel_signature(
                        &output.output_pair.output.spending_key_id,
                        &output.output_pair.kernel_nonce,
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
                    .get_txo_private_kernel_offset(
                        &output.output_pair.output.spending_key_id,
                        &output.output_pair.kernel_nonce,
                    )
                    .await?;
            let output_sender_offset_key_id = output
                .output_pair
                .sender_offset_key_id
                .clone()
                .ok_or_else(|| TPE::IncompleteStateError("Missing sender offset key id".to_string()))?;
            sender_offset_keys.push(output_sender_offset_key_id);
        }

        sender_offset_keys.push(sender_offset_key_id);

        let change_output = match &info.change_output {
            Some(change) => {
                let change = self
                    .lock_sent_output_in_payment_id(&change.output_pair, signed_message.output.hash())
                    .await?;
                tx_builder.add_output(change.output.to_transaction_output(self.key_manager).await?);
                signature = &signature +
                    &self
                        .key_manager
                        .get_partial_txo_kernel_signature(
                            &change.output.spending_key_id,
                            &change.kernel_nonce,
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
                        .get_txo_private_kernel_offset(&change.output.spending_key_id, &change.kernel_nonce)
                        .await?;
                let sender_offset_key_id = change
                    .sender_offset_key_id
                    .clone()
                    .ok_or_else(|| TPE::IncompleteStateError("Missing sender offset key id".to_string()))?;
                sender_offset_keys.push(sender_offset_key_id);
                Some(change.output)
            },
            None => None,
        };

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
        let transaction = tx_builder.build().map_err(TPE::from)?;
        Ok((transaction, change_output))
    }

    async fn lock_sent_output_in_payment_id(
        &self,
        change: &OutputPair,
        output_hash: FixedHash,
    ) -> Result<OutputPair, TPE> {
        let mut payment_id = change.output.payment_id.clone();
        payment_id.transaction_info_set_sent_output_hashes(vec![output_hash]);
        let encrypted_data = self
            .key_manager
            .encrypt_data_for_recovery(
                &change.output.spending_key_id,
                None,
                change.output.value.as_u64(),
                payment_id,
            )
            .await?;
        let mut change_output = change.output.clone();
        change_output
            .change_encrypted_data(
                encrypted_data,
                change
                    .sender_offset_key_id
                    .as_ref()
                    .ok_or_else(|| TPE::IncompleteStateError("Missing sender offset key id".to_string()))?,
                self.key_manager,
            )
            .await?;
        Ok(OutputPair {
            output: change_output,
            kernel_nonce: change.kernel_nonce.clone(),
            sender_offset_key_id: change.sender_offset_key_id.clone(),
        })
    }
}
