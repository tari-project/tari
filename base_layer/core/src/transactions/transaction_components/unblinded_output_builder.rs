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

use tari_common_types::types::{BlindingFactor, ComSignature, PrivateKey, PublicKey};
use tari_crypto::script::{ExecutionStack, TariScript};

use crate::{
    covenants::Covenant,
    transactions::{
        tari_amount::MicroTari,
        transaction_components::{OutputFeatures, TransactionError, TransactionOutput, UnblindedOutput},
    },
};

#[derive(Debug, Clone)]
pub struct UnblindedOutputBuilder {
    pub value: MicroTari,
    spending_key: BlindingFactor,
    pub features: OutputFeatures,
    pub script: Option<TariScript>,
    covenant: Covenant,
    input_data: Option<ExecutionStack>,
    script_private_key: Option<PrivateKey>,
    sender_offset_public_key: Option<PublicKey>,
    metadata_signature: Option<ComSignature>,
    metadata_signed_by_receiver: bool,
    metadata_signed_by_sender: bool,
}

impl UnblindedOutputBuilder {
    pub fn new(value: MicroTari, spending_key: BlindingFactor) -> Self {
        Self {
            value,
            spending_key,
            features: OutputFeatures::default(),
            script: None,
            covenant: Covenant::default(),
            input_data: None,
            script_private_key: None,
            sender_offset_public_key: None,
            metadata_signature: None,
            metadata_signed_by_receiver: false,
            metadata_signed_by_sender: false,
        }
    }

    pub fn sign_as_receiver(
        &mut self,
        sender_offset_public_key: PublicKey,
        public_nonce_commitment: PublicKey,
    ) -> Result<(), TransactionError> {
        self.sender_offset_public_key = Some(sender_offset_public_key.clone());

        let metadata_partial = TransactionOutput::create_partial_metadata_signature(
            &self.value,
            &self.spending_key,
            self.script
                .as_ref()
                .ok_or_else(|| TransactionError::ValidationError("script must be set".to_string()))?,
            &self.features,
            &sender_offset_public_key,
            &public_nonce_commitment,
            &self.covenant,
        )?;
        self.metadata_signature = Some(metadata_partial);
        self.metadata_signed_by_receiver = true;
        Ok(())
    }

    pub fn sign_as_sender(&mut self, sender_offset_private_key: &PrivateKey) -> Result<(), TransactionError> {
        let metadata_sig = TransactionOutput::create_final_metadata_signature(
            &self.value,
            &self.spending_key,
            self.script
                .as_ref()
                .ok_or_else(|| TransactionError::ValidationError("script must be set".to_string()))?,
            &self.features,
            sender_offset_private_key,
            &self.covenant,
        )?;
        self.metadata_signature = Some(metadata_sig);
        self.metadata_signed_by_sender = true;
        Ok(())
    }

    pub fn try_build(self) -> Result<UnblindedOutput, TransactionError> {
        if !self.metadata_signed_by_receiver {
            return Err(TransactionError::ValidationError(
                "Cannot build output because it has not been signed by the receiver".to_string(),
            ));
        }
        if !self.metadata_signed_by_sender {
            return Err(TransactionError::ValidationError(
                "Cannot build output because it has not been signed by the sender".to_string(),
            ));
        }
        let ub = UnblindedOutput::new_current_version(
            self.value,
            self.spending_key,
            self.features,
            self.script
                .ok_or_else(|| TransactionError::ValidationError("script must be set".to_string()))?,
            self.input_data
                .ok_or_else(|| TransactionError::ValidationError("input_data must be set".to_string()))?,
            self.script_private_key
                .ok_or_else(|| TransactionError::ValidationError("script_private_key must be set".to_string()))?,
            self.sender_offset_public_key
                .ok_or_else(|| TransactionError::ValidationError("sender_offset_public_key must be set".to_string()))?,
            self.metadata_signature
                .ok_or_else(|| TransactionError::ValidationError("metadata_signature must be set".to_string()))?,
            0,
            self.covenant,
        );
        Ok(ub)
    }

    pub fn with_features(mut self, features: OutputFeatures) -> Self {
        self.features = features;
        self
    }

    pub fn with_script(mut self, script: TariScript) -> Self {
        self.script = Some(script);
        self
    }

    pub fn with_input_data(mut self, input_data: ExecutionStack) -> Self {
        self.input_data = Some(input_data);
        self
    }

    pub fn with_script_private_key(mut self, script_private_key: PrivateKey) -> Self {
        self.script_private_key = Some(script_private_key);
        self
    }
}
