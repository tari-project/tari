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
use std::str::FromStr;

use log::*;
use tari_common_types::{
    tari_address::{TariAddress, TariAddressFeatures},
    transaction::TxId,
    types::CompressedPublicKey,
};
use tari_core::{
    covenants::Covenant,
    transactions::{
        tari_amount::MicroMinotari,
        transaction_components::{
            payment_id::{PaymentId, TxType},
            OutputFeatures,
        },
        transaction_key_manager::{TariKeyId, TransactionKeyManagerInterface},
        transaction_protocol::TransactionMetadata,
    },
};
use tari_script::push_pubkey_script;

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
}

impl<TBackend, TWalletConnectivity, TKeyManagerInterface>
    OfflineSigner<TBackend, TWalletConnectivity, TKeyManagerInterface>
where
    TBackend: TransactionBackend + 'static,
    TWalletConnectivity: WalletConnectivityInterface,
    TKeyManagerInterface: TransactionKeyManagerInterface,
{
    pub fn new(resources: TransactionServiceResources<TBackend, TWalletConnectivity, TKeyManagerInterface>) -> Self {
        OfflineSigner { resources }
    }

    pub async fn prepare_one_sided_transaction_for_signing(
        &mut self,
        dest_address: TariAddress,
        amount: MicroMinotari,
        selection_criteria: UtxoSelectionCriteria,
        output_features: OutputFeatures,
        fee_per_gram: MicroMinotari,
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

        let script = push_pubkey_script(&Default::default());
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
                script,
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
        for mut input in stp.get_spent_inputs()? {
            input.output.script_key_id = self
                .make_key_id_export_safe(&input.output.script_key_id)
                .await
                .map_err(TransactionServiceError::NotSupported)?;
            inputs.push(MarshalOutputPair::marshal(&self.resources.transaction_key_manager_service, input).await?);
        }
        let mut outputs = Vec::new();
        for mut output in stp.get_outputs()? {
            output.output.script_key_id = self
                .make_key_id_export_safe(&output.output.script_key_id)
                .await
                .map_err(TransactionServiceError::NotSupported)?;
            outputs.push(MarshalOutputPair::marshal(&self.resources.transaction_key_manager_service, output).await?);
        }

        let change_output = match stp.get_pre_finalized_full_change_output()? {
            Some(mut change_output) => {
                change_output.output.script_key_id = self
                    .make_key_id_export_safe(&change_output.output.script_key_id)
                    .await
                    .map_err(TransactionServiceError::NotSupported)?;
                Some(MarshalOutputPair::marshal(&self.resources.transaction_key_manager_service, change_output).await?)
            },
            None => None,
        };

        let info = OneSidedTransactionInfo {
            payment_id,
            recipient: PaymentRecipient {
                amount,
                output_features,
                address: dest_address,
            },
            change_output,
            inputs,
            outputs,
            metadata: single_round_sender_data.metadata,
            sender_address: single_round_sender_data.sender_address,
        };

        self.resources
            .output_manager_service
            .confirm_pending_transaction(tx_id, None)
            .await
            .map_err(|e| TransactionServiceProtocolError::new(tx_id, e.into()))?;

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
        let signer = OneSidedSigner::new(&self.resources.transaction_key_manager_service);
        let signed_transaction = signer.sign_transaction(request.tx_id, request.info.clone()).await?;

        Ok(SignedOneSidedTransactionResult {
            version: get_supported_version(),
            request,
            signed_transaction,
        })
    }

    async fn make_key_id_export_safe(&self, key_id: &TariKeyId) -> Result<TariKeyId, String> {
        if *key_id ==
            self.resources
                .transaction_key_manager_service
                .get_spend_key()
                .await
                .map_err(|err| err.to_string())?
                .key_id
        {
            return Ok(key_id.clone());
        }
        if *key_id ==
            self.resources
                .transaction_key_manager_service
                .get_view_key()
                .await
                .map_err(|err| err.to_string())?
                .key_id
        {
            return Ok(key_id.clone());
        }

        match key_id {
            TariKeyId::Zero => Ok(TariKeyId::Zero),
            TariKeyId::Imported { .. } => {
                // This is an imported key, so we can safely export it
                Ok(key_id.clone())
            },
            TariKeyId::Derived { key } => {
                let inner_key = TariKeyId::from_str(key.to_string().as_str())?;
                let public_key = self
                    .resources
                    .transaction_key_manager_service
                    .get_public_key_at_key_id(&inner_key)
                    .await
                    .map_err(|err| err.to_string())?;
                let modified_key = TariKeyId::Imported {
                    key: CompressedPublicKey::new_from_pk(public_key.to_public_key().map_err(|err| err.to_string())?),
                };
                let key = TariKeyId::Derived {
                    key: modified_key.into(),
                };
                Ok(key)
            },
            TariKeyId::Managed { .. } => {
                let key = self
                    .resources
                    .transaction_key_manager_service
                    .get_public_key_at_key_id(key_id)
                    .await
                    .map_err(|err| err.to_string())?;

                Ok(TariKeyId::Imported {
                    key: CompressedPublicKey::new_from_pk(key.to_public_key().map_err(|err| err.to_string())?),
                })
            },
        }
    }
}
