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
use tari_common_types::types::{ComAndPubSignature, PrivateKey, PublicKey};
use tari_crypto::keys::PublicKey as PublicKeyTrait;
use tari_script::{ExecutionStack, TariScript};

use crate::{
    core_key_manager::{BaseLayerKeyManagerInterface, KeyId},
    covenants::Covenant,
    transactions::{
        tari_amount::MicroTari,
        transaction_components::{
            EncryptedData,
            KeyManagerOutput,
            OutputFeatures,
            TransactionError,
            TransactionOutput,
            TransactionOutputVersion,
        },
    },
};

#[derive(Derivative, Clone)]
#[derivative(Debug)]
pub struct KeyManagerOutputBuilder {
    value: MicroTari,
    spending_key_id: KeyId,
    features: OutputFeatures,
    script: Option<TariScript>,
    covenant: Covenant,
    input_data: Option<ExecutionStack>,
    script_private_key_id: Option<KeyId>,
    sender_offset_public_key: Option<PublicKey>,
    metadata_signature: Option<ComAndPubSignature>,
    metadata_signed_by_receiver: bool,
    metadata_signed_by_sender: bool,
    encrypted_data: EncryptedData,
    recovery_key_id: Option<KeyId>,
    minimum_value_promise: MicroTari,
}

#[allow(dead_code)]
impl KeyManagerOutputBuilder {
    pub fn new(value: MicroTari, spending_key_id: KeyId) -> Self {
        Self {
            value,
            spending_key_id,
            features: OutputFeatures::default(),
            script: None,
            covenant: Covenant::default(),
            input_data: None,
            script_private_key_id: None,
            sender_offset_public_key: None,
            metadata_signature: None,
            metadata_signed_by_receiver: false,
            metadata_signed_by_sender: false,
            encrypted_data: EncryptedData::default(),
            recovery_key_id: None,
            minimum_value_promise: MicroTari::zero(),
        }
    }

    pub fn with_sender_offset_public_key(&mut self, sender_offset_public_key: PublicKey) {
        self.sender_offset_public_key = Some(sender_offset_public_key);
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

    pub async fn with_encrypted_data<KM: BaseLayerKeyManagerInterface>(
        mut self,
        _key_manager: &KM,
        recovery_key_id: KeyId,
    ) -> Result<Self, TransactionError> {
        self.recovery_key_id = Some(recovery_key_id);
        self.encrypted_data = EncryptedData::default(); // TODO: use key manager service to encrypt data
        Ok(self)
    }

    pub fn with_script_private_key(mut self, script_private_key: KeyId) -> Self {
        self.script_private_key_id = Some(script_private_key);
        self
    }

    pub fn value(&self) -> MicroTari {
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

    pub fn sign_as_sender_and_receiver(
        &mut self,
        sender_offset_private_key: &PrivateKey,
    ) -> Result<(), TransactionError> {
        let script = self
            .script
            .as_ref()
            .ok_or_else(|| TransactionError::ValidationError("Cannot sign metadata without a script".to_string()))?;
        let metadata_signature = TransactionOutput::create_metadata_signature(
            TransactionOutputVersion::get_current_version(),
            self.value,
            &PrivateKey::default(), // TODO &self.spending_key_id,
            script,
            &self.features,
            sender_offset_private_key,
            &self.covenant,
            &self.encrypted_data,
            self.minimum_value_promise,
        )?;
        self.sender_offset_public_key = Some(PublicKey::from_secret_key(sender_offset_private_key));
        self.metadata_signature = Some(metadata_signature);
        self.metadata_signed_by_receiver = true;
        self.metadata_signed_by_sender = true;
        Ok(())
    }

    pub fn try_build(self) -> Result<KeyManagerOutput, TransactionError> {
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
        let ub = KeyManagerOutput::new_current_version(
            self.value,
            self.spending_key_id,
            self.features,
            self.script
                .ok_or_else(|| TransactionError::ValidationError("script must be set".to_string()))?,
            self.input_data
                .ok_or_else(|| TransactionError::ValidationError("input_data must be set".to_string()))?,
            self.script_private_key_id
                .ok_or_else(|| TransactionError::ValidationError("script_private_key must be set".to_string()))?,
            self.sender_offset_public_key
                .ok_or_else(|| TransactionError::ValidationError("sender_offset_public_key must be set".to_string()))?,
            self.metadata_signature
                .ok_or_else(|| TransactionError::ValidationError("metadata_signature must be set".to_string()))?,
            0,
            self.covenant,
            self.encrypted_data,
            self.minimum_value_promise,
        );
        Ok(ub)
    }
}

#[cfg(test)]
mod test {

    use super::*;

    #[test]
    fn test_try_build() {
        let uob = KeyManagerOutputBuilder::new(100.into(), KeyId::default());
        let mut uob = uob.with_script(TariScript::new(vec![]));
        assert!(uob.clone().try_build().is_err());
        uob.with_sender_offset_public_key(PublicKey::default());
        assert!(uob.sign_as_sender_and_receiver(&PrivateKey::default()).is_ok());
        assert!(uob.clone().try_build().is_err());
        let uob = uob.with_input_data(ExecutionStack::new(vec![]));
        let uob = uob.with_script_private_key(KeyId::default());
        let uob = uob.with_features(OutputFeatures::default());
        assert!(uob.try_build().is_ok());
    }
}
