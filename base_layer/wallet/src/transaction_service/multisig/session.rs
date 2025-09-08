// Copyright 2025 The Tari Project
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
use rand::{rngs::OsRng, RngCore};
use tari_common_types::{
    key_branches::TransactionKeyManagerBranch,
    tari_address::TariAddress,
    transaction::TxId,
    types::{CompressedCommitment, CompressedPublicKey, FixedHash},
};
use tari_script::{
    push_pubkey_script,
    CompressedCheckSigSchnorrSignature,
    ExecutionStack,
    Opcode,
    StackItem,
    TariScript,
};
use tari_transaction_components::{
    fee::Fee,
    helpers::borsh::SerializedSize,
    key_manager::{TariKeyId, TransactionKeyManagerInterface},
    transaction_components::{
        covenants::Covenant,
        memo_field::{MemoField, TxType},
        one_sided::{shared_secret_to_output_encryption_key, shared_secret_to_output_spending_key},
        OutputFeatures,
        Transaction,
        WalletOutput,
        WalletOutputBuilder,
    },
    MicroMinotari,
    TransactionBuilder,
};
use tari_utilities::ByteArray;
use uuid::Uuid;

use crate::{
    connectivity_service::WalletConnectivityInterface,
    output_manager_service::{
        error::OutputManagerError,
        storage::{database::OutputBackendQuery, OutputStatus},
        UtxoSelectionCriteria,
    },
    transaction_service::{
        error::TransactionServiceError,
        multisig::{
            script::{derive_multisig_ephemeral_pubkeys, get_multi_sig_script_components},
            types::GetMultisigUtxoDataOutput,
        },
        service::TransactionServiceResources,
        storage::database::TransactionBackend,
    },
};

pub struct MultisigSession<TBackend, TWalletConnectivity, TKeyManagerInterface> {
    resources: TransactionServiceResources<TBackend, TWalletConnectivity, TKeyManagerInterface>,
}

impl<TBackend, TWalletConnectivity, TKeyManagerInterface>
    MultisigSession<TBackend, TWalletConnectivity, TKeyManagerInterface>
where
    TBackend: TransactionBackend + 'static,
    TWalletConnectivity: WalletConnectivityInterface,
    TKeyManagerInterface: TransactionKeyManagerInterface,
{
    pub fn new(resources: TransactionServiceResources<TBackend, TWalletConnectivity, TKeyManagerInterface>) -> Self {
        MultisigSession { resources }
    }

    #[allow(clippy::too_many_lines)]
    pub async fn create_deposit_multisig_transaction(
        &self,
        amount: MicroMinotari,
        party_number: u8,
        public_keys: Vec<CompressedPublicKey>,
        recipient: TariAddress,
    ) -> Result<
        (
            TxId,
            Transaction,
            MemoField,
            Vec<FixedHash>,
            Vec<FixedHash>,
            Option<Vec<WalletOutput>>,
        ),
        TransactionServiceError,
    > {
        if party_number == 0 || (party_number as usize) > public_keys.len() {
            return Err(TransactionServiceError::Other(format!(
                "Invalid multisig threshold party_number={}, participants={}",
                party_number,
                public_keys.len()
            )));
        }

        let key_manager = self.resources.transaction_key_manager_service.clone();
        let mut output_manager = self.resources.output_manager_service.clone();

        let mut message = Box::new([0u8; 32]);
        OsRng.fill_bytes(message.as_mut());

        let tx_id = TxId::new_random();
        let uuid = Uuid::new_v4();
        let user_data = uuid.as_bytes().to_vec();
        let fee_per_gram = MicroMinotari::from(1);
        let payment_id =
            MemoField::new_address_and_data(recipient.clone(), fee_per_gram, true, TxType::PaymentToOther, user_data)
                .map_err(|e| TransactionServiceError::Other(format!("Failed to create MemoField: {}", e)))?;

        let selected_criteria = UtxoSelectionCriteria {
            excluding_multisig: true,
            ..Default::default()
        };

        let mut stp = output_manager
            .prepare_transaction_to_send(
                tx_id,
                amount,
                selected_criteria,
                OutputFeatures::default(),
                fee_per_gram,
                push_pubkey_script(&Default::default()),
                Covenant::default(),
            )
            .await?;

        let sender_offset_key = self
            .resources
            .transaction_key_manager_service
            .get_next_key(TransactionKeyManagerBranch::OneSidedSenderOffset.get_branch_key())
            .await?;

        let recipient_spend_key = recipient.public_spend_key();

        let sender_offset_public_key = sender_offset_key.pub_key;

        let encrypted_data_shared_secret = key_manager
            .get_diffie_hellman_shared_secret(
                &sender_offset_key.key_id,
                recipient
                    .public_view_key()
                    .ok_or(TransactionServiceError::Other("Missing public view key".to_string()))?,
            )
            .await?;

        let encryption_private_key = shared_secret_to_output_encryption_key(&encrypted_data_shared_secret)
            .map_err(|e| TransactionServiceError::Other(format!("Failed to derive encryption key: {}", e)))?;

        let encryption_key_id = key_manager.import_key(encryption_private_key.clone()).await?;

        let ephemeral_pubkeys =
            derive_multisig_ephemeral_pubkeys(&key_manager, &public_keys, &sender_offset_key.key_id).await?;

        let mut script_opcodes = vec![Opcode::CheckMultiSigVerify(
            party_number,
            u8::try_from(ephemeral_pubkeys.len()).unwrap(),
            ephemeral_pubkeys.clone(),
            message,
        )];

        let shared_secret = self
            .resources
            .transaction_key_manager_service
            .get_diffie_hellman_shared_secret(&sender_offset_key.key_id, recipient_spend_key)
            .await?;

        let commitment_mask_private_key = shared_secret_to_output_spending_key(&shared_secret)?;

        let commitment_mask_key_id = &self
            .resources
            .transaction_key_manager_service
            .import_key(commitment_mask_private_key.clone())
            .await?;

        let script_pubkey = key_manager
            .stealth_address_script_spending_key(commitment_mask_key_id, recipient_spend_key)
            .await?;

        script_opcodes.push(Opcode::PushPubKey(script_pubkey.clone().into()));

        let final_script = TariScript::new(script_opcodes)?;

        let output = WalletOutputBuilder::new(amount, commitment_mask_key_id.clone())
            .with_script(final_script.clone())
            .with_features(OutputFeatures::default())
            .with_input_data(ExecutionStack::default())
            .encrypt_data_for_recovery(&key_manager, Some(&encryption_key_id), payment_id.clone())
            .await?
            .with_script_key(TariKeyId::Zero)
            .with_sender_offset_public_key(sender_offset_public_key.clone())
            .sign_as_sender_and_receiver_verified(&key_manager, &sender_offset_key.key_id, &recipient)
            .await?
            .try_build(&key_manager)
            .await?;

        stp.add_recipient(recipient, output.clone(), Some(sender_offset_key.key_id))
            .await?;

        let finalized_builder = stp.build().await?;

        let (change_hashes, change) = match finalized_builder.change {
            Some(change_output) => {
                let hash = change_output
                    .hash(&self.resources.transaction_key_manager_service)
                    .await?;
                (vec![hash], Some(vec![change_output]))
            },
            None => (vec![], None),
        };

        let sent_hashes = vec![output.hash(&self.resources.transaction_key_manager_service).await?];

        Ok((
            tx_id,
            finalized_builder.transaction,
            payment_id,
            sent_hashes,
            change_hashes,
            change,
        ))
    }

    pub async fn get_multisig_utxo_data(
        &self,
        commitment: CompressedCommitment,
    ) -> Result<GetMultisigUtxoDataOutput, TransactionServiceError> {
        let mut query = OutputBackendQuery::default();
        query.commitments.push(commitment.clone());

        query.status.push(OutputStatus::Unspent);

        let utxos = self
            .resources
            .output_manager_service
            .clone()
            .get_outputs_by_query(query)
            .await
            .map_err(TransactionServiceError::OutputManagerError)?;

        let selected_utxo = utxos.first().ok_or(TransactionServiceError::Other(format!(
            "UTXO with commitment {:?} not found",
            commitment
        )))?;

        let scripts = selected_utxo.wallet_output.script.clone();
        let mut challenge = Box::new([0; 32]);
        let mut public_keys = Vec::new();

        let sender_offset_pub_key = selected_utxo.wallet_output.sender_offset_public_key.to_public_key()?;

        for op in scripts.as_slice() {
            if let Opcode::CheckMultiSigVerify(_m, _n, k, msg) = op {
                challenge.clone_from_slice(msg.as_bytes());

                public_keys.extend(k.clone());
            }
        }

        Ok(GetMultisigUtxoDataOutput {
            challenge,
            public_keys,
            commitment: selected_utxo.commitment.clone(),
            sender_offset_pub_key: CompressedPublicKey::new_from_pk(sender_offset_pub_key),
        })
    }

    #[allow(clippy::too_many_lines)]
    pub async fn spend_multisig_utxo(
        &self,
        utxo_commitment: CompressedCommitment,
        signatures: Vec<CompressedCheckSigSchnorrSignature>,
        recipient: TariAddress,
    ) -> Result<
        (
            TxId,
            Transaction,
            MemoField,
            MicroMinotari,
            Vec<FixedHash>,
            Vec<FixedHash>,
        ),
        TransactionServiceError,
    > {
        let mut query = OutputBackendQuery::default();
        query.commitments.push(utxo_commitment.clone());

        query.status.push(OutputStatus::Unspent);

        let utxos = self
            .resources
            .output_manager_service
            .clone()
            .get_outputs_by_query(query)
            .await
            .map_err(TransactionServiceError::OutputManagerError)?;

        let selected_utxo = utxos.first().ok_or(TransactionServiceError::Other(format!(
            "UTXO with utxo_commitment {:?} not found",
            utxo_commitment
        )))?;

        // Enforce correct signature count and ordering for the multisig script
        let (_ephemeral_pubkeys, threshold) = get_multi_sig_script_components(&selected_utxo.wallet_output.script)?;
        if signatures.len() < threshold as usize {
            return Err(TransactionServiceError::Other(format!(
                "Insufficient signatures: need at least {}, got {}",
                threshold,
                signatures.len()
            )));
        }

        let mut input_stack = ExecutionStack::default();
        for sig in signatures {
            input_stack
                .push(StackItem::Signature(sig))
                .map_err(|e| TransactionServiceError::Other(format!("Failed to push signature: {}", e)))?;
        }

        let mut input_wallet_output = selected_utxo.wallet_output.clone();
        input_wallet_output.input_data = input_stack;

        let key_manager = self.resources.transaction_key_manager_service.clone();
        let amount = selected_utxo.wallet_output.value;

        let height = self.resources.db.get_last_scanned_height()?.unwrap_or(0);
        let consensus_constants = self.resources.consensus_manager.consensus_constants(height);

        let mut stp_builder =
            TransactionBuilder::new(consensus_constants.clone(), key_manager.clone(), self.resources.network).await?;

        let tx_id: TxId = TxId::new_random();

        let fee_calculator = Fee::new(*consensus_constants.transaction_weight_params());
        let fee_per_gram = MicroMinotari::from(1);
        let script = push_pubkey_script(&Default::default());

        let features_and_scripts_byte_size = consensus_constants
            .transaction_weight_params()
            .round_up_features_and_scripts_size(
                OutputFeatures::default()
                    .get_serialized_size()
                    .map_err(|e| OutputManagerError::ConversionError(e.to_string()))? +
                    script
                        .get_serialized_size()
                        .map_err(|e| OutputManagerError::ConversionError(e.to_string()))? +
                    Covenant::default()
                        .get_serialized_size()
                        .map_err(|e| OutputManagerError::ConversionError(e.to_string()))?,
            );

        let fee: MicroMinotari = fee_calculator.calculate(fee_per_gram, 1, 1, 1, features_and_scripts_byte_size);
        let payment_id =
            MemoField::new_address_and_data(recipient.clone(), fee_per_gram, true, TxType::PaymentToOther, vec![])
                .map_err(|e| TransactionServiceError::Other(format!("Failed to create MemoField: {}", e)))?;

        if fee > amount {
            return Err(TransactionServiceError::Other(format!(
                "insufficient funds: fee: {}, amount: {}",
                fee, amount
            )));
        }

        let total_amount = amount
            .checked_sub(fee)
            .ok_or(TransactionServiceError::Other("Amount too small to cover fee".into()))?;

        stp_builder.with_input(input_wallet_output).await?;
        stp_builder.with_fee_per_gram(fee_per_gram);
        stp_builder.with_lock_height(0);

        stp_builder
            .add_stealth_recipient(recipient, total_amount, OutputFeatures::default(), payment_id.clone())
            .await?;

        let stp = match stp_builder.build().await {
            Ok(stp) => stp,
            Err(e) => {
                return Err(TransactionServiceError::Other(format!(
                    "Failed to build transaction: {:?}",
                    e
                )));
            },
        };

        let (change_hashes, change) = match stp.change {
            Some(change_output) => {
                let hash = change_output
                    .hash(&self.resources.transaction_key_manager_service)
                    .await?;
                (vec![hash], Some(vec![change_output]))
            },
            None => (vec![], None),
        };

        let sent_hashes = stp.sent_output_hashes;

        self.resources
            .output_manager_service
            .clone()
            .confirm_pending_transaction(tx_id, change)
            .await
            .map_err(|e| TransactionServiceError::Other(format!("Failed to confirm pending transaction: {:?}", e)))?;

        Ok((
            tx_id,
            stp.transaction.clone(),
            payment_id,
            total_amount,
            change_hashes,
            sent_hashes,
        ))
    }
}
