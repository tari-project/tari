// Copyright 2019. The Tari Project
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

use tari_common_types::{key_branches::TransactionKeyManagerBranch, types::CompressedPublicKey};

use crate::{
    consensus::ConsensusConstants,
    transactions::{
        transaction_components::{TransactionKernel, WalletOutput},
        transaction_key_manager::{TransactionKeyManagerInterface, TxoStage},
        transaction_protocol::{
            recipient::RecipientSignedMessage,
            sender::SingleRoundSenderData,
            TransactionProtocolError as TPE,
        },
    },
};

/// SingleReceiverTransactionProtocol represents the actions taken by the single receiver in the one-round Tari
/// transaction protocol. The procedure is straightforward. Upon receiving the sender's information, the receiver:
/// * Checks the input for validity
/// * Constructs his output, range proof and partial signature
/// * Constructs the reply
///
/// If any step fails, an error is returned.
pub struct SingleReceiverTransactionProtocol {}

impl SingleReceiverTransactionProtocol {
    pub async fn create<KM: TransactionKeyManagerInterface>(
        sender_info: &SingleRoundSenderData,
        output: WalletOutput,
        key_manager: &KM,
        consensus_constants: &ConsensusConstants,
    ) -> Result<RecipientSignedMessage, TPE> {
        SingleReceiverTransactionProtocol::validate_sender_data(sender_info, consensus_constants)?;
        let transaction_output = output.to_transaction_output(key_manager).await?;

        let public_nonce = key_manager
            .get_next_key(TransactionKeyManagerBranch::KernelNonce.get_branch_key())
            .await?;
        let tx_meta = if output.is_burned() {
            let mut meta = sender_info.metadata.clone();
            meta.burn_commitment = Some(transaction_output.commitment().clone());
            meta
        } else {
            sender_info.metadata.clone()
        };
        let public_excess = key_manager
            .get_txo_kernel_signature_excess_with_offset(&output.spending_key_id, &public_nonce.key_id)
            .await?;

        let kernel_message = TransactionKernel::build_kernel_signature_message(
            &sender_info.kernel_version,
            tx_meta.fee,
            tx_meta.lock_height,
            &tx_meta.kernel_features,
            &tx_meta.burn_commitment,
        );
        let total_nonce = &sender_info.public_nonce.to_public_key()? + &public_nonce.pub_key.to_public_key()?;
        let total_excess = &sender_info.public_excess.to_public_key()? + &public_excess.to_public_key()?;
        let signature = key_manager
            .get_partial_txo_kernel_signature(
                &output.spending_key_id,
                &public_nonce.key_id,
                &CompressedPublicKey::new_from_pk(total_nonce),
                &CompressedPublicKey::new_from_pk(total_excess),
                &sender_info.kernel_version,
                &kernel_message,
                &tx_meta.kernel_features,
                TxoStage::Output,
            )
            .await?;
        let offset = key_manager
            .get_txo_private_kernel_offset(&output.spending_key_id, &public_nonce.key_id)
            .await?;

        let data = RecipientSignedMessage {
            tx_id: sender_info.tx_id,
            output: transaction_output,
            public_spend_key: public_excess,
            partial_signature: signature,
            tx_metadata: tx_meta,
            offset,
        };
        Ok(data)
    }

    /// Validates the sender info
    fn validate_sender_data(
        sender_info: &SingleRoundSenderData,
        consensus_constants: &ConsensusConstants,
    ) -> Result<(), TPE> {
        // validate amount
        if sender_info.amount == 0.into() {
            return Err(TPE::ValidationError("Cannot send zero micro Minotari".into()));
        }

        // validate kernel version
        if !consensus_constants
            .kernel_version_range()
            .contains(&sender_info.kernel_version)
        {
            let msg = format!(
                "Transaction kernel version is not allowed by consensus ({:?})",
                &sender_info.kernel_version
            );
            return Err(TPE::ValidationError(msg));
        }

        // validate output version
        if !consensus_constants
            .output_version_range()
            .outputs
            .contains(&sender_info.output_version)
        {
            let msg = format!(
                "Transaction output version is not allowed by consensus ({:?})",
                &sender_info.output_version
            );
            return Err(TPE::ValidationError(msg));
        }

        Ok(())
    }
}
