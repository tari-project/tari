use log::*;
use semver::Version;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tari_common_types::{
    key_branches::TransactionKeyManagerBranch,
    tari_address::{TariAddress, TariAddressFeatures},
    transaction::TxId,
    wallet_types::WalletType,
};
use tari_core::{
    consensus::ConsensusManager,
    covenants::Covenant,
    one_sided::{shared_secret_to_output_encryption_key, shared_secret_to_output_spending_key},
    transactions::{
        tari_amount::MicroMinotari,
        transaction_components::{
            encrypted_data::{PaymentId, TxType},
            OutputFeatures, WalletOutputBuilder,
        },
        transaction_key_manager::{TariKeyId, TransactionKeyManagerInterface},
        transaction_protocol::{sender::TransactionSenderMessage, TransactionMetadata},
        ReceiverTransactionProtocol, SenderTransactionProtocol,
    },
};
use tari_script::{push_pubkey_script, TariScript};

use crate::{
    connectivity_service::WalletConnectivityInterface,
    output_manager_service::UtxoSelectionCriteria,
    transaction_service::{
        error::{TransactionServiceError, TransactionServiceProtocolError},
        storage::database::TransactionBackend,
    },
};

use super::service::TransactionServiceResources;

const LOG_TARGET: &str = "wallet::transaction_service::offline_signing";
const SUPPORTED_VERSION: &str = "1.0.0";

fn get_supported_version() -> Version {
    Version::parse(SUPPORTED_VERSION).unwrap()
}

pub trait HasVersion {
    fn get_version(&self) -> &Version;
}

pub trait TransactionResult: HasVersion + Serialize + DeserializeOwned + Sized {
    fn from_string(s: &str) -> Result<Self, TransactionServiceError> {
        let deserialized_obj: Self =
            serde_json::from_str(s).map_err(|e| TransactionServiceError::SerializationError(e.to_string()))?;

        let version = deserialized_obj.get_version();
        let supported_version = get_supported_version();
        if version != &supported_version {
            return Err(TransactionServiceError::SerializationError(format!(
                "Unsupported version. Expected '{}', got '{}'",
                supported_version.to_string(),
                version.to_string(),
            )));
        }

        Ok(deserialized_obj)
    }

    fn to_string(&self) -> Result<String, TransactionServiceError> {
        serde_json::to_string(&self).map_err(|e| TransactionServiceError::SerializationError(e.to_string()))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PrepareOneSidedTransactionForSigningResult {
    pub version: Version,
    pub dest_address: TariAddress,
    pub amount: MicroMinotari,
    pub payment_id: PaymentId,
    pub tx_id: TxId,
    pub stp: SenderTransactionProtocol,
    pub encrypted_commitment_mask_keys: Vec<Vec<u8>>,
}

impl TransactionResult for PrepareOneSidedTransactionForSigningResult {}

impl HasVersion for PrepareOneSidedTransactionForSigningResult {
    fn get_version(&self) -> &Version {
        &self.version
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SignedOneSidedTransactionResult {
    pub version: Version,
    pub request: PrepareOneSidedTransactionForSigningResult,
    pub stp: SenderTransactionProtocol,
}

impl TransactionResult for SignedOneSidedTransactionResult {}

impl HasVersion for SignedOneSidedTransactionResult {
    fn get_version(&self) -> &Version {
        &self.version
    }
}

pub struct OfflineSigning<TBackend, TWalletConnectivity, TKeyManagerInterface> {
    resources: TransactionServiceResources<TBackend, TWalletConnectivity, TKeyManagerInterface>,
    consensus_manager: ConsensusManager,
    last_seen_tip_height: Option<u64>,
}

impl<TBackend, TWalletConnectivity, TKeyManagerInterface>
    OfflineSigning<TBackend, TWalletConnectivity, TKeyManagerInterface>
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
        OfflineSigning {
            resources,
            consensus_manager,
            last_seen_tip_height,
        }
    }

    fn verify_send(
        &self,
        address: &TariAddress,
        sending_method: TariAddressFeatures,
    ) -> Result<(), TransactionServiceError> {
        if address.network() != self.resources.interactive_tari_address.network() {
            return Err(TransactionServiceError::InvalidNetwork);
        }
        if !address.features().contains(sending_method) {
            return Err(TransactionServiceError::InvalidAddress(format!(
                "Address does not support feature {} ",
                sending_method
            )));
        }
        if sending_method.contains(TariAddressFeatures::create_interactive_only())
            && matches!(*self.resources.wallet_type, WalletType::Ledger(_))
        {
            return Err(TransactionServiceError::NotSupported(
                "Interactive transactions are not supported on Ledger wallets".to_string(),
            ));
        }
        Ok(())
    }

    /// Creates and locks a one sided transaction for offline signing
    /// After the transaction is signed it will be broadcasted using ``
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
        let payment_id = match payment_id.clone() {
            PaymentId::Open { .. } | PaymentId::Empty => PaymentId::add_sender_address(
                payment_id,
                self.resources.one_sided_tari_address.clone(),
                true,
                amount,
                fee_per_gram,
                if dest_address == self.resources.one_sided_tari_address
                    || dest_address == self.resources.interactive_tari_address
                {
                    Some(TxType::PaymentToSelf)
                } else {
                    Some(TxType::PaymentToOther)
                },
            ),
            _ => payment_id,
        };
        self.verify_send(&dest_address, TariAddressFeatures::create_one_sided_only())?;

        // For a stealth transaction, the script is not provided because the public key that should be included
        // is not known at this stage. This will only be known later. For now,
        // we include a default public key to ensure that the script size is correct.
        let (mut script, use_stealth_address) = match recipient_script {
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

        // This call is needed to advance the state from `SingleRoundMessageReady` to `SingleRoundMessageReady`,
        // but the returned value is not used. We have to wait until the sender transaction protocol creates a
        // sender_offset_private_key for us, so we can use it to create the shared secret
        let key = self
            .resources
            .transaction_key_manager_service
            .get_next_key(TransactionKeyManagerBranch::OneSidedSenderOffset.get_branch_key())
            .await?;

        stp.change_recipient_sender_offset_private_key(key.key_id)?;
        let _single_round_sender_data = stp
            .build_single_round_message(&self.resources.transaction_key_manager_service)
            .await
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;

        let sender_offset_private_key = stp
            .get_recipient_sender_offset_private_key()
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?
            .ok_or(TransactionServiceProtocolError::new(
                tx_id,
                TransactionServiceError::InvalidKeyId("Missing sender offset keyid".to_string()),
            ))?;
        let shared_secret = self
            .resources
            .transaction_key_manager_service
            .get_diffie_hellman_shared_secret(
                &sender_offset_private_key,
                dest_address
                    .public_view_key()
                    .ok_or(TransactionServiceProtocolError::new(
                        tx_id,
                        TransactionServiceError::OneSidedTransactionError("Missing public view key".to_string()),
                    ))?,
            )
            .await?;
        let commitment_mask_private_key = shared_secret_to_output_spending_key(&shared_secret)
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;
        let commitment_mask_key_id = &self
            .resources
            .transaction_key_manager_service
            .import_key(commitment_mask_private_key.clone())
            .await?;

        if use_stealth_address {
            let script_spending_key = self
                .resources
                .transaction_key_manager_service
                .stealth_address_script_spending_key(commitment_mask_key_id, dest_address.public_spend_key())
                .await?;
            script = push_pubkey_script(&script_spending_key);
        }

        let sender_message = TransactionSenderMessage::new_single_round_message(
            stp.get_single_round_message(&self.resources.transaction_key_manager_service)
                .await?,
        );

        let encryption_private_key = shared_secret_to_output_encryption_key(&shared_secret)?;
        let encryption_key = self
            .resources
            .transaction_key_manager_service
            .import_key(encryption_private_key)
            .await?;

        let spending_key_id = self
            .resources
            .transaction_key_manager_service
            .import_key(commitment_mask_private_key.clone())
            .await?;

        let sender_offset_public_key = self
            .resources
            .transaction_key_manager_service
            .get_public_key_at_key_id(&sender_offset_private_key)
            .await?;

        let minimum_value_promise = MicroMinotari::zero();

        let output = WalletOutputBuilder::new(amount, spending_key_id)
            .with_features(
                sender_message
                    .single()
                    .ok_or(TransactionServiceProtocolError::new(
                        tx_id,
                        TransactionServiceError::InvalidMessageError("Sent invalid message type".to_string()),
                    ))?
                    .features
                    .clone(),
            )
            .with_script(script.clone())
            .encrypt_data_for_recovery(
                &self.resources.transaction_key_manager_service,
                Some(&encryption_key),
                payment_id.clone(),
            )
            .await?
            .with_input_data(Default::default())
            .with_sender_offset_public_key(sender_offset_public_key)
            .with_script_key(TariKeyId::Zero)
            .with_minimum_value_promise(minimum_value_promise)
            .sign_as_sender_and_receiver_verified(
                &self.resources.transaction_key_manager_service,
                &sender_offset_private_key,
                &dest_address,
            )
            .await?
            .try_build(&self.resources.transaction_key_manager_service)
            .await?;

        let tip_height = self.last_seen_tip_height.unwrap_or(0);
        let consensus_constants = self.consensus_manager.consensus_constants(tip_height);
        let rtp = ReceiverTransactionProtocol::new(
            sender_message,
            output,
            &self.resources.transaction_key_manager_service,
            consensus_constants,
        )
        .await;

        let recipient_reply = rtp.get_signed_data()?.clone();

        // Start finalizing
        stp.add_presigned_recipient_info(recipient_reply)
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;

        let encrypted_commitment_mask_keys = stp
            .get_encrypted_input_keys(&self.resources.transaction_key_manager_service)
            .await?;

        Ok(PrepareOneSidedTransactionForSigningResult {
            version: get_supported_version(),
            dest_address,
            amount,
            payment_id,
            tx_id,
            stp,
            encrypted_commitment_mask_keys,
        })
    }

    pub async fn sign_locked_transaction(
        &self,
        request: PrepareOneSidedTransactionForSigningResult,
    ) -> Result<SignedOneSidedTransactionResult, TransactionServiceError> {
        let mut stp = request.stp.clone();

        let mut commitment_mask_key_ids = Vec::new();
        for encrypted_key in &request.encrypted_commitment_mask_keys {
            let key = self
                .resources
                .transaction_key_manager_service
                .decrypt_key(encrypted_key.clone())
                .await?;
            let key_id = self.resources.transaction_key_manager_service.import_key(key).await?;
            commitment_mask_key_ids.push(key_id);
        }

        stp.persist_input_script_signatures(&self.resources.transaction_key_manager_service, commitment_mask_key_ids)
            .await?;
        stp.persist_script_private_key(&self.resources.transaction_key_manager_service)
            .await?;

        Ok(SignedOneSidedTransactionResult {
            version: get_supported_version(),
            request,
            stp,
        })
    }
}
