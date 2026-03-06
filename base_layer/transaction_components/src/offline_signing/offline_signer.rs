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
use tari_common::configuration::Network;
use tari_common_types::{tari_address::TariAddress, transaction::TxId, types::CompressedPublicKey};

use crate::{
    MicroMinotari,
    TransactionBuilder,
    TransactionBuilderError,
    consensus::ConsensusConstants,
    key_manager::TransactionKeyManagerInterface,
    offline_signing::{
        models::{
            OneSidedMultisigTransactionInfo,
            OneSidedTransactionInfo,
            PaymentRecipient,
            PrepareDepositMultisigTransactionResult,
            PrepareOneSidedTransactionForSigningResult,
            PrepareWithdrawMultisigTransactionResult,
            SignedOneSidedDepositMultisigTransactionResult,
            SignedOneSidedTransactionResult,
            SignedOneSidedWithdrawMultisigTransactionResult,
            get_latest_version,
        },
        one_sided_signer::{build_and_sign_transaction, sign_multisig_transaction, sign_multisig_withdraw_transaction},
    },
    transaction_components::{MemoField, OutputFeatures, WalletOutput},
};

pub fn prepare_one_sided_transaction_for_signing<TKeyManagerInterface: TransactionKeyManagerInterface>(
    tx_id: TxId,
    tx_builder: TransactionBuilder<TKeyManagerInterface>,
    recipients: &[PaymentRecipient],
    payment_id: MemoField,
    sender_address: TariAddress,
) -> Result<PrepareOneSidedTransactionForSigningResult, TransactionBuilderError> {
    let fee = tx_builder.fee();
    let fee_per_gram = tx_builder.fee_per_gram().unwrap_or_default();
    let outputs = tx_builder
        .custom_outputs()
        .iter()
        .map(|output_pair| output_pair.output.clone())
        .collect::<Vec<WalletOutput>>();
    let inputs = tx_builder
        .inputs()
        .iter()
        .map(|output_pair| output_pair.output.clone())
        .collect::<Vec<WalletOutput>>();
    let info = OneSidedTransactionInfo {
        payment_id,
        recipients: recipients.to_vec(),
        inputs,
        outputs,
        fee,
        fee_per_gram,
        sender_address,
    };

    Ok(PrepareOneSidedTransactionForSigningResult {
        version: get_latest_version(),
        tx_id,
        info,
    })
}

pub fn prepare_deposit_multisig_transaction<TKeyManagerInterface: TransactionKeyManagerInterface>(
    tx_id: TxId,
    tx_builder: TransactionBuilder<TKeyManagerInterface>,
    amount: MicroMinotari,
    payment_id: MemoField,
    output_features: OutputFeatures,
    party_number: u8,
    public_keys: Vec<CompressedPublicKey>,
    sender: TariAddress,
    recipient: TariAddress,
) -> Result<PrepareDepositMultisigTransactionResult, TransactionBuilderError> {
    let fee = tx_builder.fee();
    let fee_per_gram = tx_builder.fee_per_gram().unwrap_or_default();
    let outputs = tx_builder
        .custom_outputs()
        .iter()
        .map(|output_pair| output_pair.output.clone())
        .collect::<Vec<WalletOutput>>();
    let inputs = tx_builder
        .inputs()
        .iter()
        .map(|output_pair| output_pair.output.clone())
        .collect::<Vec<WalletOutput>>();
    let base = OneSidedTransactionInfo {
        payment_id: payment_id.clone(),
        recipients: vec![PaymentRecipient {
            amount,
            output_features,
            address: recipient,
            payment_id,
        }],
        inputs,
        outputs,
        fee,
        fee_per_gram,
        sender_address: sender,
    };

    let info = OneSidedMultisigTransactionInfo {
        base,
        party_number,
        public_keys,
    };

    Ok(PrepareDepositMultisigTransactionResult {
        version: get_latest_version(),
        tx_id,
        info,
    })
}

pub fn prepare_withdraw_multisig_transaction<TKeyManagerInterface: TransactionKeyManagerInterface>(
    tx_id: TxId,
    tx_builder: TransactionBuilder<TKeyManagerInterface>,
    amount: MicroMinotari,
    payment_id: MemoField,
    output_features: OutputFeatures,
    sender: TariAddress,
    recipient: TariAddress,
) -> Result<PrepareWithdrawMultisigTransactionResult, TransactionBuilderError> {
    let fee = tx_builder.fee();
    let fee_per_gram = tx_builder.fee_per_gram().unwrap_or_default();
    let outputs = tx_builder
        .custom_outputs()
        .iter()
        .map(|output_pair| output_pair.output.clone())
        .collect::<Vec<WalletOutput>>();
    let inputs = tx_builder
        .inputs()
        .iter()
        .map(|output_pair| output_pair.output.clone())
        .collect::<Vec<WalletOutput>>();

    let info = OneSidedTransactionInfo {
        payment_id: payment_id.clone(),
        recipients: vec![PaymentRecipient {
            amount,
            output_features,
            address: recipient,
            payment_id,
        }],
        fee,
        fee_per_gram,
        inputs,
        outputs,
        sender_address: sender,
    };

    Ok(PrepareWithdrawMultisigTransactionResult {
        version: get_latest_version(),
        tx_id,
        info,
    })
}

pub fn sign_locked_transaction<KM: TransactionKeyManagerInterface>(
    key_manager: &KM,
    consensus_constants: ConsensusConstants,
    network: Network,
    request: PrepareOneSidedTransactionForSigningResult,
) -> Result<SignedOneSidedTransactionResult, TransactionBuilderError> {
    let signed_transaction =
        build_and_sign_transaction(key_manager, consensus_constants, network, request.info.clone())?;

    Ok(SignedOneSidedTransactionResult {
        version: get_latest_version(),
        request,
        signed_transaction,
    })
}

pub fn sign_locked_deposit_multisig_transaction<KM: TransactionKeyManagerInterface>(
    key_manager: &KM,
    consensus_constants: ConsensusConstants,
    network: Network,
    request: PrepareDepositMultisigTransactionResult,
) -> Result<SignedOneSidedDepositMultisigTransactionResult, TransactionBuilderError> {
    let signed_transaction =
        sign_multisig_transaction(key_manager, consensus_constants, network, request.info.clone())?;

    Ok(SignedOneSidedDepositMultisigTransactionResult {
        version: get_latest_version(),
        request,
        signed_transaction,
    })
}

pub fn sign_locked_withdraw_multisig_transaction<KM: TransactionKeyManagerInterface>(
    key_manager: &KM,
    consensus_constants: ConsensusConstants,
    network: Network,
    request: PrepareWithdrawMultisigTransactionResult,
) -> Result<SignedOneSidedWithdrawMultisigTransactionResult, TransactionBuilderError> {
    let signed_transaction =
        sign_multisig_withdraw_transaction(key_manager, consensus_constants, network, request.info.clone())?;

    Ok(SignedOneSidedWithdrawMultisigTransactionResult {
        version: get_latest_version(),
        request,
        signed_transaction,
    })
}
