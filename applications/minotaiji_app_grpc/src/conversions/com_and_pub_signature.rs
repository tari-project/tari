// Copyright 2020. The Taiji Project
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

use taiji_common_types::types::{ComAndPubSignature, Commitment, PrivateKey, PublicKey};
use tari_utilities::ByteArray;

use crate::taiji_rpc as grpc;

impl TryFrom<grpc::ComAndPubSignature> for ComAndPubSignature {
    type Error = String;

    fn try_from(sig: grpc::ComAndPubSignature) -> Result<Self, Self::Error> {
        let ephemeral_commitment = Commitment::from_bytes(&sig.ephemeral_commitment)
            .map_err(|_| "Could not get ephemeral commitment".to_string())?;
        let ephemeral_pubkey = PublicKey::from_bytes(&sig.ephemeral_pubkey)
            .map_err(|_| "Could not get ephemeral public key".to_string())?;
        let u_a = PrivateKey::from_bytes(&sig.u_a).map_err(|_| "Could not get partial signature u_a".to_string())?;
        let u_x = PrivateKey::from_bytes(&sig.u_x).map_err(|_| "Could not get partial signature u_x".to_string())?;
        let u_y = PrivateKey::from_bytes(&sig.u_y).map_err(|_| "Could not get partial signature u_y".to_string())?;

        Ok(Self::new(ephemeral_commitment, ephemeral_pubkey, u_a, u_x, u_y))
    }
}
