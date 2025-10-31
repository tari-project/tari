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

use tari_common_types::{tari_address::TariAddress, transaction::TxId, types::CompressedPublicKey};

use crate::{
    key_manager::TransactionKeyManagerInterface,
    offline_signing::{
        marshal_output_pair::MarshalOutputPair,
        models::{
            get_latest_version,
            OneSidedMultisigTransactionInfo,
            OneSidedTransactionInfo,
            PaymentRecipient,
            PrepareDepositMultisigTransactionResult,
            PrepareOneSidedTransactionForSigningResult,
            PrepareWithdrawMultisigTransactionResult,
            SignedOneSidedDepositMultisigTransactionResult,
            SignedOneSidedTransactionResult,
            SignedOneSidedWithdrawMultisigTransactionResult,
            TransactionMetadata,
        },
        one_sided_signer::OneSidedSigner,
    },
    transaction_components::{MemoField, OutputFeatures},
    MicroMinotari,
    TransactionBuilder,
    TransactionBuilderError,
};

pub struct OfflineSigner<TKeyManagerInterface> {
    key_manager: TKeyManagerInterface,
}

impl<TKeyManagerInterface> OfflineSigner<TKeyManagerInterface>
where TKeyManagerInterface: TransactionKeyManagerInterface
{
    pub fn new(key_manager: TKeyManagerInterface) -> Self {
        OfflineSigner { key_manager }
    }

    pub fn prepare_one_sided_transaction_for_signing(
        &mut self,
        tx_id: TxId,
        mut tx_builder: TransactionBuilder<TKeyManagerInterface>,
        dest_address: TariAddress,
        amount: MicroMinotari,
        output_features: OutputFeatures,
        payment_id: MemoField,
        sender_address: TariAddress,
    ) -> Result<PrepareOneSidedTransactionForSigningResult, TransactionBuilderError> {
        tx_builder.with_memo(payment_id.clone());
        // we do this to ensure the fee is calculated correctly
        tx_builder.add_stealth_recipient(
            dest_address.clone(),
            amount,
            output_features.clone(),
            payment_id.clone(),
        )?;

        let mut inputs = Vec::new();
        for input_pair in tx_builder.inputs() {
            let mut input = input_pair.clone();
            input.output.set_script_key_id(input.output.script_key_id().clone());
            inputs.push(MarshalOutputPair::marshal(input)?);
        }

        let mut outputs = Vec::new();
        for output_pair in tx_builder.custom_outputs() {
            let mut output = output_pair.clone();
            output.output.set_script_key_id(output.output.script_key_id().clone());
            outputs.push(MarshalOutputPair::marshal(output)?);
        }

        let (fee, change_output) = match tx_builder.get_pre_build_change_output()? {
            (fee, Some(mut change_output)) => {
                change_output
                    .output
                    .set_script_key_id(change_output.output.script_key_id().clone());
                (fee, Some(MarshalOutputPair::marshal(change_output)?))
            },
            (fee, None) => (fee, None),
        };
        let metadata = TransactionMetadata {
            fee,
            ..Default::default()
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
            metadata,
            sender_address,
        };

        Ok(PrepareOneSidedTransactionForSigningResult {
            version: get_latest_version(),
            tx_id,
            info,
        })
    }

    pub fn prepare_deposit_multisig_transaction(
        &self,
        tx_id: TxId,
        mut tx_builder: TransactionBuilder<TKeyManagerInterface>,
        amount: MicroMinotari,
        payment_id: MemoField,
        output_features: OutputFeatures,
        party_number: u8,
        public_keys: Vec<CompressedPublicKey>,
        sender: TariAddress,
        recipient: TariAddress,
    ) -> Result<PrepareDepositMultisigTransactionResult, TransactionBuilderError> {
        tx_builder.with_memo(payment_id.clone());
        // we do this to ensure the fee is calculated correctly
        tx_builder.add_stealth_recipient(recipient.clone(), amount, output_features.clone(), payment_id.clone())?;

        let mut inputs = Vec::new();
        for input_ref in tx_builder.inputs() {
            let mut input = input_ref.clone();
            input.output.set_script_key_id(input.output.script_key_id().clone());
            inputs.push(MarshalOutputPair::marshal(input)?);
        }
        let outputs = Vec::new();

        let (fee, change_output) = match tx_builder.get_pre_build_change_output()? {
            (fee, Some(mut change_output)) => {
                change_output
                    .output
                    .set_script_key_id(change_output.output.script_key_id().clone());
                (fee, Some(MarshalOutputPair::marshal(change_output)?))
            },
            (fee, None) => (fee, None),
        };

        let metadata = TransactionMetadata {
            fee,
            ..Default::default()
        };

        let info = OneSidedMultisigTransactionInfo {
            base: OneSidedTransactionInfo {
                payment_id,
                recipient: PaymentRecipient {
                    amount,
                    output_features,
                    address: recipient,
                },
                change_output,
                inputs,
                outputs,
                metadata,
                sender_address: sender,
            },
            party_number,
            public_keys,
        };

        Ok(PrepareDepositMultisigTransactionResult {
            version: get_latest_version(),
            tx_id,
            info,
        })
    }

    pub fn prepare_withdraw_multisig_transaction(
        &self,
        tx_id: TxId,
        mut tx_builder: TransactionBuilder<TKeyManagerInterface>,
        amount: MicroMinotari,
        payment_id: MemoField,
        output_features: OutputFeatures,
        sender: TariAddress,
        recipient: TariAddress,
    ) -> Result<PrepareWithdrawMultisigTransactionResult, TransactionBuilderError> {
        tx_builder.with_memo(payment_id.clone());
        tx_builder.add_stealth_recipient(recipient.clone(), amount, output_features.clone(), payment_id.clone())?;

        let mut inputs = Vec::new();
        for input_ref in tx_builder.inputs() {
            let mut input = input_ref.clone();
            input.output.set_script_key_id(input.output.script_key_id().clone());
            inputs.push(MarshalOutputPair::marshal(input)?);
        }
        let mut outputs = Vec::new();
        for output_pair in tx_builder.custom_outputs() {
            let mut output = output_pair.clone();
            output.output.set_script_key_id(output.output.script_key_id().clone());
            outputs.push(MarshalOutputPair::marshal(output)?);
        }

        let (fee, change_output) = match tx_builder.get_pre_build_change_output()? {
            (fee, Some(mut change_output)) => {
                change_output
                    .output
                    .set_script_key_id(change_output.output.script_key_id().clone());
                (fee, Some(MarshalOutputPair::marshal(change_output)?))
            },
            (fee, None) => (fee, None),
        };

        let metadata = TransactionMetadata {
            fee,
            ..Default::default()
        };

        let info = OneSidedTransactionInfo {
            payment_id,
            recipient: PaymentRecipient {
                amount,
                output_features,
                address: recipient,
            },
            change_output,
            inputs,
            outputs,
            metadata,
            sender_address: sender,
        };

        Ok(PrepareWithdrawMultisigTransactionResult {
            version: get_latest_version(),
            tx_id,
            info,
        })
    }

    pub fn sign_locked_transaction(
        &mut self,
        request: PrepareOneSidedTransactionForSigningResult,
    ) -> Result<SignedOneSidedTransactionResult, TransactionBuilderError> {
        let mut signer = OneSidedSigner::new(&mut self.key_manager);
        let signed_transaction = signer.sign_transaction(request.tx_id, request.info.clone())?;

        Ok(SignedOneSidedTransactionResult {
            version: get_latest_version(),
            request,
            signed_transaction,
        })
    }

    pub fn sign_locked_deposit_multisig_transaction(
        &mut self,
        request: PrepareDepositMultisigTransactionResult,
    ) -> Result<SignedOneSidedDepositMultisigTransactionResult, TransactionBuilderError> {
        let mut signer = OneSidedSigner::new(&mut self.key_manager);
        let signed_transaction = signer.sign_multisig_transaction(request.tx_id, request.info.clone())?;

        Ok(SignedOneSidedDepositMultisigTransactionResult {
            version: get_latest_version(),
            request,
            signed_transaction,
        })
    }

    pub fn sign_locked_withdraw_multisig_transaction(
        &mut self,
        request: PrepareWithdrawMultisigTransactionResult,
    ) -> Result<SignedOneSidedWithdrawMultisigTransactionResult, TransactionBuilderError> {
        let mut signer = OneSidedSigner::new(&mut self.key_manager);

        let signed_transaction = signer.sign_multisig_withdraw_transaction(request.tx_id, request.info.clone())?;

        Ok(SignedOneSidedWithdrawMultisigTransactionResult {
            version: get_latest_version(),
            request,
            signed_transaction,
        })
    }
}
