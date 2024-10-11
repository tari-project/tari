//  Copyright 2024. The Tari Project
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

use std::{
    borrow::Borrow,
    convert::{TryFrom, TryInto},
};

use tari_common_types::types::{ComAndPubSignature, Commitment, HashOutput, PrivateKey, PublicKey, Signature};
use tari_utilities::{ByteArray, ByteArrayError};

use crate::proto;

//---------------------------------- Commitment --------------------------------------------//

impl TryFrom<proto::common::Commitment> for Commitment {
    type Error = ByteArrayError;

    fn try_from(commitment: proto::common::Commitment) -> Result<Self, Self::Error> {
        Commitment::from_canonical_bytes(&commitment.data)
    }
}

impl From<Commitment> for proto::common::Commitment {
    fn from(commitment: Commitment) -> Self {
        Self {
            data: commitment.to_vec(),
        }
    }
}

//---------------------------------- Signature --------------------------------------------//
impl TryFrom<proto::common::Signature> for Signature {
    type Error = String;

    fn try_from(sig: proto::common::Signature) -> Result<Self, Self::Error> {
        let public_nonce = PublicKey::from_canonical_bytes(&sig.public_nonce).map_err(|e| e.to_string())?;
        let signature = PrivateKey::from_canonical_bytes(&sig.signature).map_err(|e| e.to_string())?;

        Ok(Self::new(public_nonce, signature))
    }
}

impl<T: Borrow<Signature>> From<T> for proto::common::Signature {
    fn from(sig: T) -> Self {
        Self {
            public_nonce: sig.borrow().get_public_nonce().to_vec(),
            signature: sig.borrow().get_signature().to_vec(),
        }
    }
}

//---------------------------------- ComAndPubSignature --------------------------------------//

impl TryFrom<proto::common::ComAndPubSignature> for ComAndPubSignature {
    type Error = ByteArrayError;

    fn try_from(sig: proto::common::ComAndPubSignature) -> Result<Self, Self::Error> {
        let ephemeral_commitment = Commitment::from_canonical_bytes(&sig.ephemeral_commitment)?;
        let ephemeral_pubkey = PublicKey::from_canonical_bytes(&sig.ephemeral_pubkey)?;
        let u_a = PrivateKey::from_canonical_bytes(&sig.u_a)?;
        let u_x = PrivateKey::from_canonical_bytes(&sig.u_x)?;
        let u_y = PrivateKey::from_canonical_bytes(&sig.u_y)?;

        Ok(Self::new(ephemeral_commitment, ephemeral_pubkey, u_a, u_x, u_y))
    }
}

impl From<ComAndPubSignature> for proto::common::ComAndPubSignature {
    fn from(sig: ComAndPubSignature) -> Self {
        Self {
            ephemeral_commitment: sig.ephemeral_commitment().to_vec(),
            ephemeral_pubkey: sig.ephemeral_pubkey().to_vec(),
            u_a: sig.u_a().to_vec(),
            u_x: sig.u_x().to_vec(),
            u_y: sig.u_y().to_vec(),
        }
    }
}

//---------------------------------- HashOutput --------------------------------------------//

impl TryFrom<proto::common::HashOutput> for HashOutput {
    type Error = String;

    fn try_from(output: proto::common::HashOutput) -> Result<Self, Self::Error> {
        output
            .data
            .try_into()
            .map_err(|_| "Invalid transaction hash".to_string())
    }
}

impl From<HashOutput> for proto::common::HashOutput {
    fn from(output: HashOutput) -> Self {
        Self { data: output.to_vec() }
    }
}

//--------------------------------- PrivateKey -----------------------------------------//

impl TryFrom<proto::common::PrivateKey> for PrivateKey {
    type Error = ByteArrayError;

    fn try_from(offset: proto::common::PrivateKey) -> Result<Self, Self::Error> {
        PrivateKey::from_canonical_bytes(&offset.data)
    }
}

impl From<PrivateKey> for proto::common::PrivateKey {
    fn from(offset: PrivateKey) -> Self {
        Self { data: offset.to_vec() }
    }
}

//---------------------------------- Wrappers --------------------------------------------//

impl From<Vec<u64>> for proto::base_node::BlockHeights {
    fn from(heights: Vec<u64>) -> Self {
        Self { heights }
    }
}
