//  Copyright 2022. The Tari Project
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

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use tari_common_types::{
    epoch::VnEpoch,
    types::{CompressedPublicKey, PrivateKey, Signature},
};
use tari_utilities::ByteArray;

use crate::transactions::transaction_components::ValidatorNodeSignature;

#[derive(Default, Debug, Clone, PartialEq, Eq, Deserialize, Serialize, BorshSerialize, BorshDeserialize)]
pub struct ValidatorNodeExit {
    signature: ValidatorNodeSignature,
    /// The maximum epoch for which this registration is valid. Base nodes will reject any registration that is
    /// submitted after max_epoch. Assuming the epoch is selected sensibly, this mitigates against replay attacks.
    max_epoch: VnEpoch,
}

impl ValidatorNodeExit {
    pub fn new(signature: ValidatorNodeSignature, max_epoch: VnEpoch) -> Self {
        Self { signature, max_epoch }
    }

    pub fn signed(secret_key: &PrivateKey, sidechain_pk: Option<&CompressedPublicKey>, max_epoch: VnEpoch) -> Self {
        Self {
            signature: ValidatorNodeSignature::sign_for_exit(secret_key, sidechain_pk, max_epoch),
            max_epoch,
        }
    }

    pub fn is_valid_signature_for(&self, sidechain_pk: Option<&CompressedPublicKey>) -> bool {
        self.signature.is_valid_exit_signature_for(sidechain_pk, self.max_epoch)
    }

    pub fn public_key(&self) -> &CompressedPublicKey {
        self.signature.public_key()
    }

    pub fn max_epoch(&self) -> VnEpoch {
        self.max_epoch
    }

    pub fn signature(&self) -> &Signature {
        self.signature.signature()
    }

    pub fn sidechain_id_message(&self) -> &[u8] {
        self.public_key().as_bytes()
    }
}

#[cfg(test)]
mod test {
    use rand::rngs::OsRng;
    use tari_common_types::types::PrivateKey;
    use tari_crypto::keys::SecretKey;

    use super::*;

    mod is_valid_signature_for {
        use super::*;

        #[test]
        fn it_returns_true_for_valid_signature() {
            let sk = PrivateKey::random(&mut OsRng);
            let exit = ValidatorNodeExit::signed(&sk, None, VnEpoch(1));
            assert!(exit.is_valid_signature_for(None));
        }

        #[test]
        fn it_returns_false_if_epoch_is_malleated() {
            let sk = PrivateKey::random(&mut OsRng);
            let exit = ValidatorNodeExit::new(ValidatorNodeSignature::sign_for_exit(&sk, None, VnEpoch(1)), VnEpoch(2));
            assert!(!exit.is_valid_signature_for(None));
        }

        #[test]
        fn it_returns_false_for_zero_signature() {
            let sk = PrivateKey::random(&mut OsRng);
            let exit = ValidatorNodeExit::signed(&sk, None, VnEpoch(1));
            let exit = ValidatorNodeExit::new(
                ValidatorNodeSignature::new(exit.public_key().clone(), Signature::default()),
                VnEpoch(1),
            );
            assert!(!exit.is_valid_signature_for(None));
        }
    }
}
