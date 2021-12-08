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

use std::convert::TryFrom;

use tari_common_types::types::{
    BlindingFactor,
    ComSignature,
    Commitment,
    HashOutput,
    PrivateKey,
    PublicKey,
    Signature,
};
use tari_crypto::tari_utilities::{ByteArray, ByteArrayError};

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
    type Error = ByteArrayError;

    fn try_from(sig: proto::Signature) -> Result<Self, Self::Error> {
        let public_nonce = PublicKey::from_bytes(&sig.public_nonce)?;
        let signature = PrivateKey::from_bytes(&sig.signature)?;

        Ok(Self::new(public_nonce, signature))
    }
}

impl From<Signature> for proto::Signature {
    fn from(sig: Signature) -> Self {
        Self {
            public_nonce: sig.get_public_nonce().to_vec(),
            signature: sig.get_signature().to_vec(),
        }
    }
}

//---------------------------------- ComSignature --------------------------------------------//

impl TryFrom<proto::ComSignature> for ComSignature {
    type Error = ByteArrayError;

    fn try_from(sig: proto::ComSignature) -> Result<Self, Self::Error> {
        let public_nonce = Commitment::from_bytes(&sig.public_nonce_commitment)?;
        let signature_u = PrivateKey::from_bytes(&sig.signature_u)?;
        let signature_v = PrivateKey::from_bytes(&sig.signature_v)?;

        Ok(Self::new(public_nonce, signature_u, signature_v))
    }
}

impl From<ComSignature> for proto::ComSignature {
    fn from(sig: ComSignature) -> Self {
        Self {
            public_nonce_commitment: sig.public_nonce().to_vec(),
            signature_u: sig.u().to_vec(),
            signature_v: sig.v().to_vec(),
        }
    }
}

//---------------------------------- HashOutput --------------------------------------------//

impl From<proto::HashOutput> for HashOutput {
    fn from(output: proto::HashOutput) -> Self {
        output.data
    }
}

impl From<HashOutput> for proto::HashOutput {
    fn from(output: HashOutput) -> Self {
        Self { data: output }
    }
}

//--------------------------------- BlindingFactor -----------------------------------------//

impl TryFrom<proto::BlindingFactor> for BlindingFactor {
    type Error = ByteArrayError;

    fn try_from(offset: proto::BlindingFactor) -> Result<Self, Self::Error> {
        BlindingFactor::from_bytes(&offset.data)
    }
}

impl From<BlindingFactor> for proto::BlindingFactor {
    fn from(offset: BlindingFactor) -> Self {
        Self { data: offset.to_vec() }
    }
}
