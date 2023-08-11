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

use log::warn;
use rand::random;
use tari_common_types::{tari_address::TariAddress, types::PublicKey};
use tari_core::transactions::{tari_amount::MicroTari, transaction_components::OutputFeatures};
use tari_utilities::{hex::Hex, ByteArray};
use tari_wallet::{
    output_manager_service::UtxoSelectionCriteria,
    storage::{database::WalletDatabase, sqlite_db::wallet::WalletSqliteDatabase},
    transaction_service::handle::{TransactionEvent, TransactionSendStatus, TransactionServiceHandle},
};
use tokio::sync::{broadcast, watch};

use crate::ui::{
    state::{BurntProofBase64, CommitmentSignatureBase64, UiTransactionBurnStatus, UiTransactionSendStatus},
    ui_error::UiError,
};

const LOG_TARGET: &str = "wallet::console_wallet::tasks ";

pub async fn send_transaction_task(
    address: TariAddress,
    amount: MicroTari,
    selection_criteria: UtxoSelectionCriteria,
    output_features: OutputFeatures,
    message: String,
    fee_per_gram: MicroTari,
    mut transaction_service_handle: TransactionServiceHandle,
    result_tx: watch::Sender<UiTransactionSendStatus>,
) {
    let _result = result_tx.send(UiTransactionSendStatus::Initiated);
    let mut event_stream = transaction_service_handle.get_event_stream();
    let mut send_status = TransactionSendStatus::default();
    match transaction_service_handle
        .send_transaction(
            address,
            amount,
            selection_criteria,
            output_features,
            fee_per_gram,
            message,
        )
        .await
    {
        Err(e) => {
            let _result = result_tx.send(UiTransactionSendStatus::Error(UiError::from(e).to_string()));
        },
        Ok(our_tx_id) => {
            loop {
                let next_event = event_stream.recv().await;
                match next_event {
                    Ok(event) => match &*event {
                        TransactionEvent::TransactionDiscoveryInProgress(tx_id) => {
                            if our_tx_id == *tx_id {
                                let _result = result_tx.send(UiTransactionSendStatus::DiscoveryInProgress);
                            }
                        },
                        TransactionEvent::TransactionSendResult(tx_id, status) => {
                            if our_tx_id == *tx_id {
                                send_status = status.clone();
                                break;
                            }
                        },
                        TransactionEvent::TransactionCompletedImmediately(tx_id) => {
                            if our_tx_id == *tx_id {
                                let _result = result_tx.send(UiTransactionSendStatus::TransactionComplete);
                                return;
                            }
                        },
                        _ => (),
                    },
                    Err(e @ broadcast::error::RecvError::Lagged(_)) => {
                        log::warn!(target: LOG_TARGET, "Error reading from event broadcast channel {:?}", e);
                        continue;
                    },
                    Err(broadcast::error::RecvError::Closed) => {
                        break;
                    },
                }
            }

            if send_status.direct_send_result {
                let _result = result_tx.send(UiTransactionSendStatus::SentDirect);
            } else if send_status.store_and_forward_send_result {
                let _result = result_tx.send(UiTransactionSendStatus::SentViaSaf);
            } else if send_status.queued_for_retry {
                let _result = result_tx.send(UiTransactionSendStatus::Queued);
            } else {
                let _result = result_tx.send(UiTransactionSendStatus::Error(
                    "Transaction could not be sent".to_string(),
                ));
            }
        },
    }
}

pub async fn send_one_sided_transaction_task(
    address: TariAddress,
    amount: MicroTari,
    selection_criteria: UtxoSelectionCriteria,
    output_features: OutputFeatures,
    message: String,
    fee_per_gram: MicroTari,
    mut transaction_service_handle: TransactionServiceHandle,
    result_tx: watch::Sender<UiTransactionSendStatus>,
) {
    let _result = result_tx.send(UiTransactionSendStatus::Initiated);
    let mut event_stream = transaction_service_handle.get_event_stream();
    match transaction_service_handle
        .send_one_sided_transaction(
            address,
            amount,
            selection_criteria,
            output_features,
            fee_per_gram,
            message,
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
                        if let TransactionEvent::TransactionCompletedImmediately(tx_id) = &*event {
                            if our_tx_id == *tx_id {
                                let _result = result_tx.send(UiTransactionSendStatus::TransactionComplete);
                                return;
                            }
                        }
                    },
                    Err(e @ broadcast::error::RecvError::Lagged(_)) => {
                        log::warn!(target: LOG_TARGET, "Error reading from event broadcast channel {:?}", e);
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

pub async fn send_one_sided_to_stealth_address_transaction(
    address: TariAddress,
    amount: MicroTari,
    selection_criteria: UtxoSelectionCriteria,
    output_features: OutputFeatures,
    message: String,
    fee_per_gram: MicroTari,
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
            message,
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
                        if let TransactionEvent::TransactionCompletedImmediately(tx_id) = &*event {
                            if our_tx_id == *tx_id {
                                let _result = result_tx.send(UiTransactionSendStatus::TransactionComplete);
                                return;
                            }
                        }
                    },
                    Err(e @ broadcast::error::RecvError::Lagged(_)) => {
                        log::warn!(target: LOG_TARGET, "Error reading from event broadcast channel {:?}", e);
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

pub async fn send_burn_transaction_task(
    burn_proof_filepath: Option<PathBuf>,
    claim_public_key: Option<PublicKey>,
    amount: MicroTari,
    selection_criteria: UtxoSelectionCriteria,
    message: String,
    fee_per_gram: MicroTari,
    mut transaction_service_handle: TransactionServiceHandle,
    db: WalletDatabase<WalletSqliteDatabase>,
    result_tx: watch::Sender<UiTransactionBurnStatus>,
) {
    result_tx.send(UiTransactionBurnStatus::Initiated).unwrap();
    let mut event_stream = transaction_service_handle.get_event_stream();

    // ----------------------------------------------------------------------------
    // burning minotari
    // ----------------------------------------------------------------------------

    let (burn_tx_id, original_proof) = transaction_service_handle
        .burn_tari(amount, selection_criteria, fee_per_gram, message, claim_public_key)
        .await
        .map_err(|err| {
            log::error!("failed to burn minotari: {:?}", err);

            result_tx
                .send(UiTransactionBurnStatus::Error(UiError::from(err).to_string()))
                .unwrap();
        })
        .unwrap();

    // ----------------------------------------------------------------------------
    // starting a feedback loop to wait for the answer from the transaction service
    // ----------------------------------------------------------------------------

    loop {
        let original_proof = original_proof.clone();
        let burn_proof_filepath = burn_proof_filepath.clone();

        match event_stream.recv().await {
            Ok(event) => {
                if let TransactionEvent::TransactionCompletedImmediately(completed_tx_id) = &*event {
                    if burn_tx_id == *completed_tx_id {
                        let wrapped_proof = BurntProofBase64 {
                            reciprocal_claim_public_key: original_proof.reciprocal_claim_public_key.to_vec(),
                            commitment: original_proof.commitment.to_vec(),
                            ownership_proof: original_proof.ownership_proof.map(|x| CommitmentSignatureBase64 {
                                public_nonce: x.public_nonce().to_vec(),
                                u: x.u().to_vec(),
                                v: x.v().to_vec(),
                            }),
                            range_proof: original_proof.range_proof.0,
                        };

                        let serialized_proof =
                            serde_json::to_string_pretty(&wrapped_proof).expect("failed to serialize burn proof");

                        let proof_id = random::<u32>();
                        let filepath =
                            burn_proof_filepath.unwrap_or_else(|| PathBuf::from(format!("{}.json", proof_id)));

                        std::fs::write(filepath, serialized_proof.as_bytes()).expect("failed to save burn proof");

                        let result = db.create_burnt_proof(
                            proof_id,
                            original_proof.reciprocal_claim_public_key.to_hex(),
                            serialized_proof.clone(),
                        );

                        if let Err(err) = result {
                            log::error!("failed to create database entry for the burnt proof: {:?}", err);
                        }

                        result_tx
                            .send(UiTransactionBurnStatus::TransactionComplete((
                                proof_id,
                                original_proof.reciprocal_claim_public_key.to_hex(),
                                serialized_proof,
                            )))
                            .unwrap();

                        return;
                    }
                } else {
                    warn!(target: LOG_TARGET, "Encountered an unexpected event");
                    todo!()
                }
            },

            Err(e @ broadcast::error::RecvError::Lagged(_)) => {
                warn!(target: LOG_TARGET, "Error reading from event broadcast channel {:?}", e);
                continue;
            },

            Err(broadcast::error::RecvError::Closed) => {
                break;
            },
        }
    }
}
