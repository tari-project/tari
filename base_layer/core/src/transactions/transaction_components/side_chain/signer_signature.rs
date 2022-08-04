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

use std::io;

use digest::{Digest, Output};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use tari_common_types::types::{PrivateKey, PublicKey, Signature};
use tari_crypto::{hash::blake2::Blake256, keys::PublicKey as PublicKeyT};
use tari_utilities::ByteArray;

use crate::consensus::{ConsensusDecoding, ConsensusEncoding, ConsensusEncodingSized};

#[derive(Debug, Clone, Hash, PartialEq, Deserialize, Serialize, Eq, Default)]
pub struct SignerSignature {
    signer: PublicKey,
    signature: Signature,
}

impl SignerSignature {
    pub fn new(signer: PublicKey, signature: Signature) -> Self {
        Self { signer, signature }
    }

    pub fn sign<C: AsRef<[u8]>>(signer_secret: &PrivateKey, challenge: C) -> Self {
        let signer = PublicKey::from_secret_key(signer_secret);
        let (nonce, public_nonce) = PublicKey::random_keypair(&mut OsRng);

        let final_challenge = Self::build_final_challenge(&signer, challenge, &public_nonce);
        let signature =
            Signature::sign(signer_secret.clone(), nonce, &*final_challenge).expect("challenge is the correct length");
        Self { signer, signature }
    }

    pub fn verify<C: AsRef<[u8]>>(signature: &Signature, signer: &PublicKey, challenge: C) -> bool {
        let public_nonce = signature.get_public_nonce();
        let final_challenge = Self::build_final_challenge(signer, challenge, public_nonce);
        signature.verify_challenge(signer, &final_challenge)
    }

    fn build_final_challenge<C: AsRef<[u8]>>(
        signer: &PublicKey,
        challenge: C,
        public_nonce: &PublicKey,
    ) -> Output<Blake256> {
        // TODO: Use domain-seperated hasher from tari_crypto
        Blake256::new()
            .chain(signer.as_bytes())
            .chain(public_nonce.as_bytes())
            .chain(challenge)
            .finalize()
    }

    pub fn signer(&self) -> &PublicKey {
        &self.signer
    }

    pub fn signature(&self) -> &Signature {
        &self.signature
    }
}

impl ConsensusEncoding for SignerSignature {
    fn consensus_encode<W: io::Write>(&self, writer: &mut W) -> Result<(), io::Error> {
        self.signer.consensus_encode(writer)?;
        self.signature.consensus_encode(writer)?;
        Ok(())
    }
}

impl ConsensusEncodingSized for SignerSignature {
    fn consensus_encode_exact_size(&self) -> usize {
        32 + 64
    }
}

impl ConsensusDecoding for SignerSignature {
    fn consensus_decode<R: io::Read>(reader: &mut R) -> Result<Self, io::Error> {
        Ok(Self {
            signer: PublicKey::consensus_decode(reader)?,
            signature: Signature::consensus_decode(reader)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consensus::check_consensus_encoding_correctness;

    #[test]
    fn it_encodes_and_decodes_correctly() {
        let subject = SignerSignature::default();
        check_consensus_encoding_correctness(subject).unwrap();
    }
}
