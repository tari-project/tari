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

use std::fmt;

use serde::{Deserialize, Serialize};
use tari_common_types::{
    transaction::TxId,
    types::{PrivateKey, PublicKey, Signature},
};

use crate::{
    consensus::ConsensusConstants,
    transactions::{
        key_manager::TransactionKeyManagerInterface,
        transaction_components::{TransactionOutput, WalletOutput},
        transaction_protocol::{
            sender::{SingleRoundSenderData, TransactionSenderMessage},
            single_receiver::SingleReceiverTransactionProtocol,
            TransactionMetadata,
            TransactionProtocolError,
        },
    },
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum RecipientState {
    Finalized(Box<RecipientSignedMessage>),
    Failed(TransactionProtocolError),
}

impl fmt::Display for RecipientState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use RecipientState::{Failed, Finalized};
        match self {
            Finalized(signed_message) => write!(
                f,
                "Finalized({:?}, maturity = {})",
                signed_message.output.features.output_type, signed_message.output.features.maturity
            ),
            Failed(err) => write!(f, "Failed({:?})", err),
        }
    }
}

/// This is the message containing the public data that the Receiver will send back to the Sender
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipientSignedMessage {
    pub tx_id: TxId,
    pub output: TransactionOutput,
    pub public_spend_key: PublicKey,
    pub partial_signature: Signature,
    pub tx_metadata: TransactionMetadata,
    pub offset: PrivateKey,
}

/// The generalised transaction recipient protocol. A different state transition network is followed depending on
/// whether this is a single recipient or one of many.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ReceiverTransactionProtocol {
    pub state: RecipientState,
}

/// Initiate a new recipient protocol state.
///
/// It takes as input the transaction message from the sender (which will indicate how many rounds the transaction
/// protocol will undergo, the recipient's nonce and spend key, as well as the output features for this recipient's
/// transaction output.
///
/// The function returns the protocol in the relevant state. If this is a single-round protocol, the state will
/// already be finalised, and the return message will be accessible from the `get_signed_data` method.
impl ReceiverTransactionProtocol {
    pub async fn new<KM: TransactionKeyManagerInterface>(
        info: TransactionSenderMessage,
        output: WalletOutput,
        key_manager: &KM,
        consensus_constants: &ConsensusConstants,
    ) -> ReceiverTransactionProtocol {
        let state = match info {
            TransactionSenderMessage::None => RecipientState::Failed(TransactionProtocolError::InvalidStateError),
            TransactionSenderMessage::Single(v) => {
                ReceiverTransactionProtocol::single_round(output, &v, key_manager, consensus_constants).await
            },
            TransactionSenderMessage::Multiple => Self::multi_round(),
        };
        ReceiverTransactionProtocol { state }
    }

    /// Returns true if the recipient protocol is finalised, and the signature data is ready to be sent to the sender.
    pub fn is_finalized(&self) -> bool {
        matches!(self.state, RecipientState::Finalized(_))
    }

    /// Method to determine if the transaction protocol has failed
    pub fn is_failed(&self) -> bool {
        matches!(&self.state, RecipientState::Failed(_))
    }

    /// Method to return the error behind a failure, if one has occurred
    pub fn failure_reason(&self) -> Option<TransactionProtocolError> {
        match &self.state {
            RecipientState::Failed(e) => Some(e.clone()),
            _ => None,
        }
    }

    /// Retrieve the final signature data to be returned to the sender to complete the transaction.
    pub fn get_signed_data(&self) -> Result<&RecipientSignedMessage, TransactionProtocolError> {
        match &self.state {
            RecipientState::Finalized(data) => Ok(data),
            _ => Err(TransactionProtocolError::InvalidStateError),
        }
    }

    /// Run the single-round recipient protocol, which can immediately construct an output and sign the data
    async fn single_round<KM: TransactionKeyManagerInterface>(
        output: WalletOutput,
        data: &SingleRoundSenderData,
        key_manager: &KM,
        consensus_constants: &ConsensusConstants,
    ) -> RecipientState {
        let signer = SingleReceiverTransactionProtocol::create(data, output, key_manager, consensus_constants).await;
        match signer {
            Ok(signed_data) => RecipientState::Finalized(Box::new(signed_data)),
            Err(e) => RecipientState::Failed(e),
        }
    }

    fn multi_round() -> RecipientState {
        RecipientState::Failed(TransactionProtocolError::UnsupportedError(
            "Multiple recipients aren't supported yet".into(),
        ))
    }

    /// Create an empty SenderTransactionProtocol that can be used as a placeholder in data structures that do not
    /// require a well formed version
    pub fn new_placeholder() -> Self {
        ReceiverTransactionProtocol {
            state: RecipientState::Failed(TransactionProtocolError::IncompleteStateError(
                "This is a placeholder protocol".to_string(),
            )),
        }
    }
}

#[cfg(test)]
mod test {
    use tari_common_types::types::PublicKey;
    use tari_crypto::keys::PublicKey as PublicKeyTrait;
    use tari_key_manager::key_manager_service::{KeyId, KeyManagerInterface};
    use tari_script::TariScript;

    use crate::{
        covenants::Covenant,
        test_helpers::create_consensus_constants,
        transactions::{
            crypto_factories::CryptoFactories,
            key_manager::{
                create_memory_db_key_manager,
                TransactionKeyManagerBranch,
                TransactionKeyManagerInterface,
                TxoStage,
            },
            tari_amount::*,
            test_helpers::{TestParams, UtxoTestParams},
            transaction_components::{
                OutputFeatures,
                TransactionKernel,
                TransactionKernelVersion,
                TransactionOutputVersion,
            },
            transaction_protocol::{
                sender::{SingleRoundSenderData, TransactionSenderMessage},
                TransactionMetadata,
            },
            ReceiverTransactionProtocol,
        },
    };

    #[tokio::test]
    async fn single_round_recipient() {
        let key_manager = create_memory_db_key_manager().unwrap();
        let factories = CryptoFactories::default();
        let sender_test_params = TestParams::new(&key_manager).await;
        let m = TransactionMetadata::new(MicroMinotari(125), 0);
        let script = TariScript::default();
        let amount = MicroMinotari(500);
        let features = OutputFeatures::default();
        let msg = SingleRoundSenderData {
            tx_id: 15u64.into(),
            amount,
            public_excess: sender_test_params.kernel_nonce_key_pk, // any random key will do
            public_nonce: sender_test_params.public_nonce_key_pk,  // any random key will do
            metadata: m.clone(),
            message: "".to_string(),
            features,
            script,
            sender_offset_public_key: sender_test_params.sender_offset_key_pk,
            ephemeral_public_nonce: sender_test_params.ephemeral_public_nonce_key_pk,
            covenant: Covenant::default(),
            minimum_value_promise: MicroMinotari::zero(),
            output_version: TransactionOutputVersion::get_current_version(),
            kernel_version: TransactionKernelVersion::get_current_version(),
        };
        let sender_info = TransactionSenderMessage::Single(Box::new(msg.clone()));
        let params = UtxoTestParams {
            value: msg.amount,
            ..Default::default()
        };
        let receiver_test_params = TestParams::new(&key_manager).await;
        let output = receiver_test_params.create_output(params, &key_manager).await.unwrap();
        let consensus_constants = create_consensus_constants(0);
        let receiver =
            ReceiverTransactionProtocol::new(sender_info, output.clone(), &key_manager, &consensus_constants).await;

        assert!(receiver.is_finalized());
        let data = receiver.get_signed_data().unwrap();
        let pubkey = key_manager
            .get_public_key_at_key_id(&receiver_test_params.commitment_mask_key_id)
            .await
            .unwrap();
        let offset = data.offset.clone();
        let public_offset = PublicKey::from_secret_key(&offset);
        let signing_pubkey = &pubkey - &public_offset;
        assert_eq!(data.tx_id.as_u64(), 15);
        assert_eq!(data.public_spend_key, signing_pubkey);
        let commitment = key_manager
            .get_commitment(&receiver_test_params.commitment_mask_key_id, &500.into())
            .await
            .unwrap();
        assert_eq!(&commitment, &data.output.commitment);
        data.output.verify_range_proof(&factories.range_proof).unwrap();

        let index = key_manager
            .find_key_index(
                TransactionKeyManagerBranch::KernelNonce.get_branch_key(),
                data.partial_signature.get_public_nonce(),
            )
            .await
            .unwrap();
        let nonce_id = KeyId::Managed {
            branch: TransactionKeyManagerBranch::KernelNonce.get_branch_key(),
            index,
        };
        let kernel_message = TransactionKernel::build_kernel_signature_message(
            &TransactionKernelVersion::get_current_version(),
            m.fee,
            m.lock_height,
            &m.kernel_features,
            &m.burn_commitment,
        );
        let p_nonce = key_manager.get_public_key_at_key_id(&nonce_id).await.unwrap();
        let p_commitment_mask_key = key_manager
            .get_txo_kernel_signature_excess_with_offset(&receiver_test_params.commitment_mask_key_id, &nonce_id)
            .await
            .unwrap();
        let r_sum = &msg.public_nonce + &p_nonce;
        let excess = &msg.public_excess + &p_commitment_mask_key;
        let kernel_signature = key_manager
            .get_partial_txo_kernel_signature(
                &receiver_test_params.commitment_mask_key_id,
                &nonce_id,
                &r_sum,
                &excess,
                &TransactionKernelVersion::get_current_version(),
                &kernel_message,
                &m.kernel_features,
                TxoStage::Output,
            )
            .await
            .unwrap();
        assert_eq!(data.partial_signature, kernel_signature);

        let (mask, value, _) = key_manager.try_output_key_recovery(&data.output, None).await.unwrap();
        assert_eq!(output.spending_key_id, mask);
        assert_eq!(output.value, value);
    }
}
