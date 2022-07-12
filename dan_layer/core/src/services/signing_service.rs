// Copyright 2021. The Tari Project
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

use std::sync::Arc;

use tari_comms::NodeIdentity;
use tari_core::transactions::transaction_components::{CheckpointChallenge, SignerSignature};

use crate::{digital_assets_error::DigitalAssetError, models::ValidatorSignature};

pub trait SigningService {
    fn sign(&self, challenge: &[u8]) -> Result<ValidatorSignature, DigitalAssetError>;

    fn sign_checkpoint(&self, challenge: &CheckpointChallenge) -> Result<SignerSignature, DigitalAssetError>;
}

pub struct NodeIdentitySigningService {
    node_identity: Arc<NodeIdentity>,
}

impl NodeIdentitySigningService {
    pub fn new(node_identity: Arc<NodeIdentity>) -> Self {
        Self { node_identity }
    }
}

impl SigningService for NodeIdentitySigningService {
    fn sign(&self, _challenge: &[u8]) -> Result<ValidatorSignature, DigitalAssetError> {
        // TODO better sig
        Ok(ValidatorSignature {})
    }

    fn sign_checkpoint(&self, challenge: &CheckpointChallenge) -> Result<SignerSignature, DigitalAssetError> {
        Ok(SignerSignature::sign(self.node_identity.secret_key(), challenge))
    }
}
