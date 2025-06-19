use log::*;
use tari_common_types::{
    tari_address::{TariAddress, TariAddressFeatures},
    transaction::TxId,
};
use tari_core::{
    consensus::ConsensusManager,
    covenants::Covenant,
    transactions::{
        tari_amount::MicroMinotari,
        transaction_components::{
            payment_id::{PaymentId, TxType},
            OutputFeatures,
        },
        transaction_key_manager::TransactionKeyManagerInterface,
        transaction_protocol::TransactionMetadata,
    },
};
use tari_script::{push_pubkey_script, TariScript};

use crate::{
    connectivity_service::WalletConnectivityInterface,
    output_manager_service::UtxoSelectionCriteria,
    transaction_service::{
        error::{TransactionServiceError, TransactionServiceProtocolError},
        offline_signing::{
            marshal_output_pair::MarshalOutputPair,
            models::{
                get_supported_version,
                OneSidedTransactionInfo,
                PaymentRecipient,
                PrepareOneSidedTransactionForSigningResult,
                SignedOneSidedTransactionResult,
            },
            one_sided_signer::OneSidedSigner,
        },
        service::TransactionServiceResources,
        storage::database::TransactionBackend,
    },
};

const LOG_TARGET: &str = "wallet::transaction_service::offline_signing::offline_signer";

pub struct OfflineSigner<TBackend, TWalletConnectivity, TKeyManagerInterface> {
    resources: TransactionServiceResources<TBackend, TWalletConnectivity, TKeyManagerInterface>,
    consensus_manager: ConsensusManager,
    last_seen_tip_height: Option<u64>,
}

impl<TBackend, TWalletConnectivity, TKeyManagerInterface>
    OfflineSigner<TBackend, TWalletConnectivity, TKeyManagerInterface>
where
    TBackend: TransactionBackend + 'static,
    TWalletConnectivity: WalletConnectivityInterface,
    TKeyManagerInterface: TransactionKeyManagerInterface,
{
    pub fn new(
        resources: TransactionServiceResources<TBackend, TWalletConnectivity, TKeyManagerInterface>,
        consensus_manager: ConsensusManager,
        last_seen_tip_height: Option<u64>,
    ) -> Self {
        OfflineSigner {
            resources,
            consensus_manager,
            last_seen_tip_height,
        }
    }

    pub async fn prepare_one_sided_transaction_for_signing(
        &mut self,
        dest_address: TariAddress,
        amount: MicroMinotari,
        selection_criteria: UtxoSelectionCriteria,
        output_features: OutputFeatures,
        fee_per_gram: MicroMinotari,
        recipient_script: Option<TariScript>,
        mut payment_id: PaymentId,
    ) -> Result<PrepareOneSidedTransactionForSigningResult, TransactionServiceError> {
        debug!(target: LOG_TARGET, "Locking one sided transaction to {} with {}", dest_address, amount);
        let tx_id = TxId::new_random();

        // let override the payment_id if the address says we should
        if dest_address.features().contains(TariAddressFeatures::PAYMENT_ID) {
            debug!(target: LOG_TARGET, "Address contains memo, overriding memo {} with {:?}", payment_id, dest_address.get_payment_id_user_data_bytes());
            payment_id = PaymentId::open(dest_address.get_payment_id_user_data_bytes(), TxType::PaymentToOther);
        }
        let payment_id = match payment_id {
            PaymentId::Open { .. } | PaymentId::Empty => payment_id.add_sender_address(
                self.resources.one_sided_tari_address.clone(),
                true,
                fee_per_gram,
                if dest_address == self.resources.one_sided_tari_address ||
                    dest_address == self.resources.interactive_tari_address
                {
                    Some(TxType::PaymentToSelf)
                } else {
                    Some(TxType::PaymentToOther)
                },
            ),
            _ => payment_id,
        };

        // For a stealth transaction, the script is not provided because the public key that should be included
        // is not known at this stage. This will only be known later. For now,
        // we include a default public key to ensure that the script size is correct.
        let (script, use_stealth_address) = match recipient_script {
            Some(s) => (s, false),
            None => (push_pubkey_script(&Default::default()), true),
        };

        // Prepare sender part of the transaction
        let mut stp = self
            .resources
            .output_manager_service
            .prepare_transaction_to_send(
                tx_id,
                amount,
                selection_criteria,
                output_features.clone(),
                fee_per_gram,
                TransactionMetadata::default(),
                script.clone(),
                Covenant::default(),
                MicroMinotari::zero(),
                dest_address.clone(),
                payment_id.clone(),
            )
            .await?;

        let single_round_sender_data = stp
            .build_single_round_message(&self.resources.transaction_key_manager_service)
            .await
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;

        let mut inputs = Vec::new();
        for input in stp.get_spent_inputs()? {
            inputs.push(MarshalOutputPair::marshal(&self.resources.transaction_key_manager_service, input).await?);
        }
        let mut outputs = Vec::new();
        for output in stp.get_outputs()? {
            outputs.push(MarshalOutputPair::marshal(&self.resources.transaction_key_manager_service, output).await?);
        }

        let change_output = match stp.get_pre_finalized_full_change_output()? {
            Some(change_output) => {
                Some(MarshalOutputPair::marshal(&self.resources.transaction_key_manager_service, change_output).await?)
            },
            None => None,
        };

        let info = OneSidedTransactionInfo {
            last_seen_tip_height: self.last_seen_tip_height,
            payment_id,
            recipient: PaymentRecipient {
                amount,
                output_features,
                script,
                sender_offset_public_key: single_round_sender_data.sender_offset_public_key,
                covenant: single_round_sender_data.covenant,
                minimum_value_promise: single_round_sender_data.minimum_value_promise,
                ephemeral_public_key_nonce: single_round_sender_data.ephemeral_public_nonce,
                address: dest_address,
                use_stealth_address,
            },
            change_output,
            inputs,
            outputs,
            metadata: single_round_sender_data.metadata,
            sender_address: single_round_sender_data.sender_address,
        };

        Ok(PrepareOneSidedTransactionForSigningResult {
            version: get_supported_version(),
            tx_id,
            info,
        })
    }

    pub async fn sign_locked_transaction(
        &self,
        request: PrepareOneSidedTransactionForSigningResult,
    ) -> Result<SignedOneSidedTransactionResult, TransactionServiceError> {
        let signer = OneSidedSigner::new(&self.resources.transaction_key_manager_service, &self.consensus_manager);
        let signed_transaction = signer
            .sign_transaction(request.tx_id.clone(), request.info.clone())
            .await?;

        Ok(SignedOneSidedTransactionResult {
            version: get_supported_version(),
            request,
            signed_transaction,
        })
    }
}
