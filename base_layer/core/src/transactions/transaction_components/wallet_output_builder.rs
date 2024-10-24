//  Copyright 2021. The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use derivative::Derivative;
use tari_common_types::{
    tari_address::TariAddress,
    types::{ComAndPubSignature, PublicKey},
};
use tari_script::{ExecutionStack, TariScript};

use crate::{
    covenants::Covenant,
    transactions::{
        key_manager::{TariKeyId, TransactionKeyManagerInterface},
        tari_amount::MicroMinotari,
        transaction_components::{
            encrypted_data::PaymentId,
            EncryptedData,
            OutputFeatures,
            TransactionError,
            TransactionOutput,
            TransactionOutputVersion,
            WalletOutput,
        },
    },
};

#[derive(Derivative, Clone)]
#[derivative(Debug)]
pub struct WalletOutputBuilder {
    version: TransactionOutputVersion,
    value: MicroMinotari,
    commitment_mask_key_id: TariKeyId,
    features: OutputFeatures,
    script: Option<TariScript>,
    script_lock_height: u64,
    covenant: Covenant,
    input_data: Option<ExecutionStack>,
    script_key_id: Option<TariKeyId>,
    sender_offset_public_key: Option<PublicKey>,
    metadata_signature: Option<ComAndPubSignature>,
    metadata_signed_by_receiver: bool,
    metadata_signed_by_sender: bool,
    encrypted_data: EncryptedData,
    custom_recovery_key_id: Option<TariKeyId>,
    minimum_value_promise: MicroMinotari,
    payment_id: PaymentId,
}

#[allow(dead_code)]
impl WalletOutputBuilder {
    pub fn new(value: MicroMinotari, commitment_mask_key_id: TariKeyId) -> Self {
        Self {
            version: TransactionOutputVersion::get_current_version(),
            value,
            commitment_mask_key_id,
            features: OutputFeatures::default(),
            script: None,
            script_lock_height: 0,
            covenant: Covenant::default(),
            input_data: None,
            script_key_id: None,
            sender_offset_public_key: None,
            metadata_signature: None,
            metadata_signed_by_receiver: false,
            metadata_signed_by_sender: false,
            encrypted_data: EncryptedData::default(),
            custom_recovery_key_id: None,
            minimum_value_promise: MicroMinotari::zero(),
            payment_id: PaymentId::Empty,
        }
    }

    pub fn with_sender_offset_public_key(mut self, sender_offset_public_key: PublicKey) -> Self {
        self.sender_offset_public_key = Some(sender_offset_public_key);
        self
    }

    pub fn with_features(mut self, features: OutputFeatures) -> Self {
        self.features = features;
        self
    }

    pub fn with_script(mut self, script: TariScript) -> Self {
        self.script = Some(script);
        self
    }

    pub fn with_script_lock_height(mut self, height: u64) -> Self {
        self.script_lock_height = height;
        self
    }

    pub fn with_input_data(mut self, input_data: ExecutionStack) -> Self {
        self.input_data = Some(input_data);
        self
    }

    pub fn with_covenant(mut self, covenant: Covenant) -> Self {
        self.covenant = covenant;
        self
    }

    pub async fn encrypt_data_for_recovery<KM: TransactionKeyManagerInterface>(
        mut self,
        key_manager: &KM,
        custom_recovery_key_id: Option<&TariKeyId>,
        payment_id: PaymentId,
    ) -> Result<Self, TransactionError> {
        self.encrypted_data = key_manager
            .encrypt_data_for_recovery(
                &self.commitment_mask_key_id,
                custom_recovery_key_id,
                self.value.as_u64(),
                payment_id,
            )
            .await?;
        Ok(self)
    }

    pub fn with_script_key(mut self, script_key_id: TariKeyId) -> Self {
        self.script_key_id = Some(script_key_id);
        self
    }

    pub fn with_version(mut self, version: TransactionOutputVersion) -> Self {
        self.version = version;
        self
    }

    pub fn with_minimum_value_promise(mut self, minimum_value_promise: MicroMinotari) -> Self {
        self.minimum_value_promise = minimum_value_promise;
        self
    }

    pub fn value(&self) -> MicroMinotari {
        self.value
    }

    pub fn features(&self) -> &OutputFeatures {
        &self.features
    }

    pub fn script(&self) -> Option<&TariScript> {
        self.script.as_ref()
    }

    pub fn covenant(&self) -> &Covenant {
        &self.covenant
    }

    pub async fn sign_as_sender_and_receiver<KM: TransactionKeyManagerInterface>(
        mut self,
        key_manager: &KM,
        sender_offset_key_id: &TariKeyId,
    ) -> Result<Self, TransactionError> {
        let script = self
            .script
            .as_ref()
            .ok_or_else(|| TransactionError::BuilderError("Cannot sign metadata without a script".to_string()))?;
        let sender_offset_public_key = key_manager.get_public_key_at_key_id(sender_offset_key_id).await?;
        let metadata_message = TransactionOutput::metadata_signature_message_from_parts(
            &self.version,
            script,
            &self.features,
            &self.covenant,
            &self.encrypted_data,
            &self.minimum_value_promise,
        );
        let metadata_signature = key_manager
            .get_metadata_signature(
                &self.commitment_mask_key_id,
                &self.value.into(),
                sender_offset_key_id,
                &self.version,
                &metadata_message,
                self.features.range_proof_type,
            )
            .await?;
        self.metadata_signature = Some(metadata_signature);
        self.metadata_signed_by_receiver = true;
        self.metadata_signed_by_sender = true;
        self.sender_offset_public_key = Some(sender_offset_public_key);
        Ok(self)
    }

    pub async fn sign_as_sender_and_receiver_verified<KM: TransactionKeyManagerInterface>(
        mut self,
        key_manager: &KM,
        sender_offset_key_id: &TariKeyId,
        receiver_address: &TariAddress,
    ) -> Result<Self, TransactionError> {
        let script = self
            .script
            .as_ref()
            .ok_or_else(|| TransactionError::BuilderError("Cannot sign metadata without a script".to_string()))?;
        let sender_offset_public_key = key_manager.get_public_key_at_key_id(sender_offset_key_id).await?;
        let metadata_message_common = TransactionOutput::metadata_signature_message_common_from_parts(
            &self.version,
            &self.features,
            &self.covenant,
            &self.encrypted_data,
            &self.minimum_value_promise,
        );
        let metadata_signature = key_manager
            .get_one_sided_metadata_signature(
                &self.commitment_mask_key_id,
                self.value,
                sender_offset_key_id,
                &self.version,
                &metadata_message_common,
                self.features.range_proof_type,
                script,
                receiver_address,
            )
            .await?;
        self.metadata_signature = Some(metadata_signature);
        self.metadata_signed_by_receiver = true;
        self.metadata_signed_by_sender = true;
        self.sender_offset_public_key = Some(sender_offset_public_key);
        Ok(self)
    }

    /// Sign a partial multi-party metadata signature as the sender and receiver - `sender_offset_public_key_shares` and
    /// `ephemeral_pubkey_shares` from other participants are combined to enable creation of the challenge.
    pub async fn sign_partial_as_sender_and_receiver<KM: TransactionKeyManagerInterface>(
        mut self,
        key_manager: &KM,
        sender_offset_key_id: &TariKeyId,
        aggregated_sender_offset_public_key_shares: &PublicKey,
        aggregated_ephemeral_public_key_shares: &PublicKey,
    ) -> Result<Self, TransactionError> {
        let script = self
            .script
            .as_ref()
            .ok_or_else(|| TransactionError::BuilderError("Cannot sign metadata without a script".to_string()))?;
        let metadata_message = TransactionOutput::metadata_signature_message_from_parts(
            &self.version,
            script,
            &self.features,
            &self.covenant,
            &self.encrypted_data,
            &self.minimum_value_promise,
        );

        let sender_offset_public_key_self = key_manager.get_public_key_at_key_id(sender_offset_key_id).await?;
        let aggregate_sender_offset_public_key =
            aggregated_sender_offset_public_key_shares + &sender_offset_public_key_self;

        let ephemeral_pubkey_self = key_manager.get_random_key().await?;
        let aggregate_ephemeral_pubkey = aggregated_ephemeral_public_key_shares + &ephemeral_pubkey_self.pub_key;

        let receiver_partial_metadata_signature = key_manager
            .get_receiver_partial_metadata_signature(
                &self.commitment_mask_key_id,
                &self.value.into(),
                &aggregate_sender_offset_public_key,
                &aggregate_ephemeral_pubkey,
                &TransactionOutputVersion::get_current_version(),
                &metadata_message,
                self.features.range_proof_type,
            )
            .await?;

        let commitment = key_manager
            .get_commitment(&self.commitment_mask_key_id, &self.value.into())
            .await?;
        let ephemeral_commitment = receiver_partial_metadata_signature.ephemeral_commitment();
        let challenge = TransactionOutput::finalize_metadata_signature_challenge(
            &TransactionOutputVersion::get_current_version(),
            &aggregate_sender_offset_public_key,
            ephemeral_commitment,
            &aggregate_ephemeral_pubkey,
            &commitment,
            &metadata_message,
        );
        let sender_partial_metadata_signature_self = key_manager
            .sign_with_nonce_and_challenge(sender_offset_key_id, &ephemeral_pubkey_self.key_id, &challenge)
            .await?;

        let metadata_signature = &receiver_partial_metadata_signature + &sender_partial_metadata_signature_self;

        self.metadata_signature = Some(metadata_signature);
        self.metadata_signed_by_receiver = true;
        self.metadata_signed_by_sender = true;
        self.sender_offset_public_key = Some(aggregate_sender_offset_public_key);
        Ok(self)
    }

    pub async fn try_build<KM: TransactionKeyManagerInterface>(
        self,
        key_manager: &KM,
    ) -> Result<WalletOutput, TransactionError> {
        if !self.metadata_signed_by_receiver {
            return Err(TransactionError::BuilderError(
                "Cannot build output because it has not been signed by the receiver".to_string(),
            ));
        }
        if !self.metadata_signed_by_sender {
            return Err(TransactionError::BuilderError(
                "Cannot build output because it has not been signed by the sender".to_string(),
            ));
        }
        let ub = WalletOutput::new(
            self.version,
            self.value,
            self.commitment_mask_key_id,
            self.features,
            self.script
                .ok_or_else(|| TransactionError::BuilderError("script must be set".to_string()))?,
            self.input_data
                .ok_or_else(|| TransactionError::BuilderError("input_data must be set".to_string()))?,
            self.script_key_id
                .ok_or_else(|| TransactionError::BuilderError("script_private_key must be set".to_string()))?,
            self.sender_offset_public_key
                .ok_or_else(|| TransactionError::BuilderError("sender_offset_public_key must be set".to_string()))?,
            self.metadata_signature
                .ok_or_else(|| TransactionError::BuilderError("metadata_signature must be set".to_string()))?,
            self.script_lock_height,
            self.covenant,
            self.encrypted_data,
            self.minimum_value_promise,
            self.payment_id,
            key_manager,
        )
        .await?;
        Ok(ub)
    }
}

#[cfg(test)]
mod test {
    use tari_common_types::key_branches::TransactionKeyManagerBranch;
    use tari_key_manager::key_manager_service::KeyManagerInterface;

    use super::*;
    use crate::transactions::key_manager::create_memory_db_key_manager;

    #[tokio::test]
    async fn test_try_build() {
        let key_manager = create_memory_db_key_manager().unwrap();
        let (commitment_mask_key, script_key_id) = key_manager.get_next_commitment_mask_and_script_key().await.unwrap();
        let value = MicroMinotari(100);
        let kmob = WalletOutputBuilder::new(value, commitment_mask_key.key_id.clone());
        let kmob = kmob.with_script(TariScript::new(vec![]).unwrap());
        assert!(kmob.clone().try_build(&key_manager).await.is_err());
        let sender_offset = key_manager
            .get_next_key(TransactionKeyManagerBranch::SenderOffset.get_branch_key())
            .await
            .unwrap();
        let kmob = kmob.with_sender_offset_public_key(sender_offset.pub_key);
        assert!(kmob.clone().try_build(&key_manager).await.is_err());
        let kmob = kmob.with_input_data(ExecutionStack::new(vec![]));
        let kmob = kmob.with_script_key(script_key_id.key_id);
        let kmob = kmob.with_features(OutputFeatures::default());
        let kmob = kmob
            .encrypt_data_for_recovery(&key_manager, None, PaymentId::Empty)
            .await
            .unwrap()
            .sign_as_sender_and_receiver(&key_manager, &sender_offset.key_id)
            .await
            .unwrap();
        match kmob.clone().try_build(&key_manager).await {
            Ok(val) => {
                let output = val.to_transaction_output(&key_manager).await.unwrap();
                assert!(output.verify_metadata_signature().is_ok());
                assert!(key_manager
                    .verify_mask(output.commitment(), &commitment_mask_key.key_id, value.into())
                    .await
                    .unwrap());

                let (recovered_key_id, recovered_value, _) =
                    key_manager.try_output_key_recovery(&output, None).await.unwrap();
                assert_eq!(recovered_key_id, commitment_mask_key.key_id);
                assert_eq!(recovered_value, value);
            },
            Err(e) => panic!("{}", e),
        }
    }

    #[tokio::test]
    async fn test_partial_metadata_signatures() {
        let key_manager = create_memory_db_key_manager().unwrap();
        let (commitment_mask_key, script_key) = key_manager.get_next_commitment_mask_and_script_key().await.unwrap();
        let value = MicroMinotari(100);
        let kmob = WalletOutputBuilder::new(value, commitment_mask_key.key_id.clone());
        let kmob = kmob.with_script(TariScript::new(vec![]).unwrap());
        let sender_offset = key_manager
            .get_next_key(TransactionKeyManagerBranch::SenderOffset.get_branch_key())
            .await
            .unwrap();
        let kmob = kmob.with_sender_offset_public_key(sender_offset.pub_key);
        let kmob = kmob.with_input_data(ExecutionStack::new(vec![]));
        let kmob = kmob.with_script_key(script_key.key_id);
        let kmob = kmob.with_features(OutputFeatures::default());
        let kmob = kmob
            .encrypt_data_for_recovery(&key_manager, None, PaymentId::Empty)
            .await
            .unwrap()
            .sign_as_sender_and_receiver(&key_manager, &sender_offset.key_id)
            .await
            .unwrap();
        match kmob.clone().try_build(&key_manager).await {
            Ok(wallet_output) => {
                let mut output = wallet_output.to_transaction_output(&key_manager).await.unwrap();
                assert!(output.verify_metadata_signature().is_ok());

                // Now we can swap out the metadata signature for one built from partial sender and receiver signatures
                let ephemeral_key = key_manager
                    .get_next_key(TransactionKeyManagerBranch::Nonce.get_branch_key())
                    .await
                    .unwrap();
                let metadata_message = TransactionOutput::metadata_signature_message(&wallet_output);

                let receiver_metadata_signature = key_manager
                    .get_receiver_partial_metadata_signature(
                        &wallet_output.spending_key_id,
                        &wallet_output.value.into(),
                        &wallet_output.sender_offset_public_key,
                        &ephemeral_key.pub_key,
                        &wallet_output.version,
                        &metadata_message,
                        wallet_output.features.range_proof_type,
                    )
                    .await
                    .unwrap();

                let commitment = key_manager
                    .get_commitment(&wallet_output.spending_key_id, &wallet_output.value.into())
                    .await
                    .unwrap();
                let sender_metadata_signature = key_manager
                    .get_sender_partial_metadata_signature(
                        &ephemeral_key.key_id,
                        &sender_offset.key_id,
                        &commitment,
                        receiver_metadata_signature.ephemeral_commitment(),
                        &wallet_output.version,
                        &metadata_message,
                    )
                    .await
                    .unwrap();

                let metadata_signature_from_partials = &receiver_metadata_signature + &sender_metadata_signature;
                assert_ne!(output.metadata_signature, metadata_signature_from_partials);
                output.metadata_signature = metadata_signature_from_partials;
                assert!(output.verify_metadata_signature().is_ok());
            },
            Err(e) => panic!("{}", e),
        }
    }
}
