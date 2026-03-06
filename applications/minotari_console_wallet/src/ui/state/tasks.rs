// Copyright 2020. The Tari Project
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

use std::path::PathBuf;

use log::{debug, error, warn};
use minotari_wallet::{
    output_manager_service::UtxoSelectionCriteria,
    transaction_service::handle::{TransactionEvent, TransactionServiceHandle},
};
use tari_common_types::{
    tari_address::TariAddress,
    types::{CompressedPublicKey, PrivateKey},
};
use tari_transaction_components::{
    MicroMinotari,
    transaction_components::{MemoField, OutputFeatures},
};
use tari_utilities::ByteArray;
use tokio::sync::{broadcast, watch};

use crate::ui::{
    state::{BurntProofBase64, SignatureBase64, UiTransactionBurnStatus, UiTransactionSendStatus},
    ui_error::UiError,
};

const LOG_TARGET: &str = "wallet::console_wallet::tasks ";

pub async fn send_one_sided_to_stealth_address_transaction(
    address: TariAddress,
    amount: MicroMinotari,
    selection_criteria: UtxoSelectionCriteria,
    output_features: OutputFeatures,
    fee_per_gram: MicroMinotari,
    payment_id: MemoField,
    mut transaction_service_handle: TransactionServiceHandle,
    result_tx: watch::Sender<UiTransactionSendStatus>,
) {
    let _result = result_tx.send(UiTransactionSendStatus::Initiated);
    let mut event_stream = transaction_service_handle.get_event_stream();
    match transaction_service_handle
        .send_one_sided_to_stealth_address_transaction(
            address,
            amount,
            selection_criteria,
            output_features,
            fee_per_gram,
            payment_id,
        )
        .await
    {
        Err(e) => {
            let _result = result_tx.send(UiTransactionSendStatus::Error(UiError::from(e).to_string()));
        },
        Ok(our_tx_id) => {
            loop {
                match event_stream.recv().await {
                    Ok(event) => {
                        if let TransactionEvent::TransactionCompletedImmediately(tx_id) = &*event &&
                            our_tx_id == *tx_id
                        {
                            let _result = result_tx.send(UiTransactionSendStatus::TransactionComplete);
                            return;
                        }
                    },
                    Err(e @ broadcast::error::RecvError::Lagged(_)) => {
                        log::warn!(target: LOG_TARGET, "Error reading from event broadcast channel {e:?}");
                        continue;
                    },
                    Err(broadcast::error::RecvError::Closed) => {
                        break;
                    },
                }
            }

            let _result = result_tx.send(UiTransactionSendStatus::Error(
                "One-sided transaction could not be sent".to_string(),
            ));
        },
    }
}

#[allow(clippy::too_many_lines)]
pub async fn send_burn_transaction_task(
    burn_proof_filepath: Option<PathBuf>,
    claim_public_key: Option<CompressedPublicKey>,
    amount: MicroMinotari,
    selection_criteria: UtxoSelectionCriteria,
    payment_id: MemoField,
    fee_per_gram: MicroMinotari,
    sidechain_deployment_key: Option<PrivateKey>,
    mut transaction_service_handle: TransactionServiceHandle,
    result_tx: watch::Sender<UiTransactionBurnStatus>,
) {
    result_tx.send(UiTransactionBurnStatus::Initiated).unwrap();
    let mut event_stream = transaction_service_handle.get_event_stream();

    // ----------------------------------------------------------------------------
    // burning minotari
    // ----------------------------------------------------------------------------

    debug!(
        target: LOG_TARGET, "Burn tari - amount: {}, fee per gram: {}, payment id: {}, claim pk: {}, selection: {}",
        amount, fee_per_gram, payment_id, claim_public_key.clone().unwrap_or_default(), selection_criteria
    );
    let (burn_tx_id, original_proof) = match transaction_service_handle
        .burn_tari(
            amount,
            selection_criteria,
            fee_per_gram,
            payment_id,
            claim_public_key,
            sidechain_deployment_key,
        )
        .await
    {
        Ok((burn_tx_id, original_proof)) => (burn_tx_id, original_proof),
        Err(e) => {
            error!(target: LOG_TARGET, "failed to burn minotari: {e:?}");
            result_tx
                .send(UiTransactionBurnStatus::Error(format!("burn error: {e}")))
                .unwrap();
            return;
        },
    };
    // ----------------------------------------------------------------------------
    // starting a feedback loop to wait for the answer from the transaction service
    // ----------------------------------------------------------------------------

    loop {
        match event_stream.recv().await {
            Ok(ref event) => {
                let TransactionEvent::TransactionCompletedImmediately(completed_tx_id) = event.as_ref() else {
                    warn!(target: LOG_TARGET, "Encountered an unexpected event: {}", event);
                    continue;
                };

                if burn_tx_id != *completed_tx_id {
                    continue;
                }
                if let Some(original_proof) = original_proof &&
                    let Some(filepath) = burn_proof_filepath
                {
                    let wrapped_proof = BurntProofBase64 {
                        claim_public_key: original_proof.claim_public_key.to_vec(),
                        commitment: original_proof.commitment.to_vec(),
                        ownership_proof: SignatureBase64 {
                            public_nonce: original_proof.ownership_proof.get_compressed_public_nonce().to_vec(),
                            signature: original_proof.ownership_proof.get_signature().to_vec(),
                        },
                    };

                    let serialized_proof = match serde_json::to_string_pretty(&wrapped_proof) {
                        Ok(proof) => proof,
                        Err(e) => {
                            error!(target: LOG_TARGET, "failed to serialize burn proof: {e:?}");
                            result_tx
                                .send(UiTransactionBurnStatus::Error(format!("failure to create proof {e:?}")))
                                .unwrap();
                            return;
                        },
                    };

                    if let Err(e) = std::fs::write(filepath, serialized_proof.as_bytes()) {
                        error!(target: LOG_TARGET, "failed to write burn proof: {e:?}");
                        result_tx
                            .send(UiTransactionBurnStatus::Error(format!("failure to write proof {e:?}")))
                            .unwrap();
                        return;
                    }
                }

                result_tx.send(UiTransactionBurnStatus::TransactionComplete).unwrap();

                return;
            },
            Err(e @ broadcast::error::RecvError::Lagged(_)) => {
                warn!(target: LOG_TARGET, "Error reading from event broadcast channel {e:?}");
                continue;
            },

            Err(broadcast::error::RecvError::Closed) => {
                break;
            },
        }
    }
}
