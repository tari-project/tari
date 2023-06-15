// Copyright 2019, The Tari Project
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

use std::{
    borrow::Borrow,
    convert::{TryFrom, TryInto},
};

use tari_common_types::types::{ComAndPubSignature, Commitment, HashOutput, PrivateKey, PublicKey, Signature};
use tari_utilities::{ByteArray, ByteArrayError};

use super::types as proto;

//---------------------------------- Commitment --------------------------------------------//

impl TryFrom<proto::Commitment> for Commitment {
    type Error = ByteArrayError;

    fn try_from(commitment: proto::Commitment) -> Result<Self, Self::Error> {
        Commitment::from_bytes(&commitment.data)
    }
}

impl From<Commitment> for proto::Commitment {
    fn from(commitment: Commitment) -> Self {
        Self {
            data: commitment.to_vec(),
        }
    }
}

//---------------------------------- Signature --------------------------------------------//
impl TryFrom<proto::Signature> for Signature {
    type Error = String;

    fn try_from(sig: proto::Signature) -> Result<Self, Self::Error> {
        let public_nonce = PublicKey::from_bytes(&sig.public_nonce).map_err(|e| e.to_string())?;
        let signature = PrivateKey::from_bytes(&sig.signature).map_err(|e| e.to_string())?;

        Ok(Self::new(public_nonce, signature))
    }
}

impl<T: Borrow<Signature>> From<T> for proto::Signature {
    fn from(sig: T) -> Self {
        Self {
            public_nonce: sig.borrow().get_public_nonce().to_vec(),
            signature: sig.borrow().get_signature().to_vec(),
        }
    }
}

//---------------------------------- ComAndPubSignature --------------------------------------//

impl TryFrom<proto::ComAndPubSignature> for ComAndPubSignature {
    type Error = ByteArrayError;

    fn try_from(sig: proto::ComAndPubSignature) -> Result<Self, Self::Error> {
        let ephemeral_commitment = Commitment::from_bytes(&sig.ephemeral_commitment)?;
        let ephemeral_pubkey = PublicKey::from_bytes(&sig.ephemeral_pubkey)?;
        let u_a = PrivateKey::from_bytes(&sig.u_a)?;
        let u_x = PrivateKey::from_bytes(&sig.u_x)?;
        let u_y = PrivateKey::from_bytes(&sig.u_y)?;

        Ok(Self::new(ephemeral_commitment, ephemeral_pubkey, u_a, u_x, u_y))
    }
}

impl From<ComAndPubSignature> for proto::ComAndPubSignature {
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

impl TryFrom<proto::HashOutput> for HashOutput {
    type Error = String;

    fn try_from(output: proto::HashOutput) -> Result<Self, Self::Error> {
        output
            .data
            .try_into()
            .map_err(|_| "Invalid transaction hash".to_string())
    }
}

impl From<HashOutput> for proto::HashOutput {
    fn from(output: HashOutput) -> Self {
        Self { data: output.to_vec() }
    }
}

//--------------------------------- PrivateKey -----------------------------------------//

impl TryFrom<proto::PrivateKey> for PrivateKey {
    type Error = ByteArrayError;

    fn try_from(offset: proto::PrivateKey) -> Result<Self, Self::Error> {
        PrivateKey::from_bytes(&offset.data)
    }
}

impl From<PrivateKey> for proto::PrivateKey {
    fn from(offset: PrivateKey) -> Self {
        Self { data: offset.to_vec() }
    }
}
