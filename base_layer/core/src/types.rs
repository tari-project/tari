// Copyright 2018 The Tari Project
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
//
// Portions of this file were originally copyrighted (c) 2018 The Grin Developers, issued under the Apache License,
// Version 2.0, available at http://www.apache.org/licenses/LICENSE-2.0.

use crate::pow::*;
use digest::Digest;
use serde::{Deserialize, Serialize};
use tari_crypto::{
    common::Blake256,
    ristretto::{
        dalek_range_proof::DalekRangeProofService,
        pedersen::{PedersenCommitment, PedersenCommitmentFactory},
        RistrettoPublicKey,
        RistrettoSchnorr,
        RistrettoSecretKey,
    },
};
use tari_utilities::{byte_array::*, hash::*};

/// Define the explicit Signature implementation for the Tari base layer. A different signature scheme can be
/// employed by redefining this type.
pub type Signature = RistrettoSchnorr;

/// Define the explicit Commitment implementation for the Tari base layer.
pub type Commitment = PedersenCommitment;
pub type CommitmentFactory = PedersenCommitmentFactory;

/// Define the explicit Secret key implementation for the Tari base layer.
pub type PrivateKey = RistrettoSecretKey;
pub type BlindingFactor = RistrettoSecretKey;

/// Define the explicit Public key implementation for the Tari base layer
pub type PublicKey = RistrettoPublicKey;

/// Define the hash function that will be used to produce a signature challenge
pub type SignatureHash = Blake256;

/// Specify the Hash function for general hashing
pub type HashDigest = Blake256;

/// Specify the digest type for signature challenges
pub type Challenge = Blake256;

/// The type of output that `Challenge` produces
pub type MessageHash = Vec<u8>;

/// Specify the range proof type
pub type RangeProofService = DalekRangeProofService;

/// Specify the Proof of Work
pub type POW = MockProofOfWork;

#[cfg(test)]
pub const MAX_RANGE_PROOF_RANGE: usize = 32; // 2^32 This is the only way to produce failing range proofs for the tests
#[cfg(not(test))]
pub const MAX_RANGE_PROOF_RANGE: usize = 64; // 2^64

/// Current version of the blockchain
pub const BLOCKCHAIN_VERSION: u16 = 0;
/// The min required lock height before coinbase utxos are spendable
pub const COINBASE_LOCK_HEIGHT: u64 = 1440;

// Set up some "global" services for the Tari blockchain - These are most likely not threadsafe as written, but haven't
// checked.
lazy_static! {
    pub static ref COMMITMENT_FACTORY: CommitmentFactory = CommitmentFactory::default();
    pub static ref PROVER: RangeProofService =
        RangeProofService::new(MAX_RANGE_PROOF_RANGE, &COMMITMENT_FACTORY).unwrap();
}

#[derive(Debug, Default, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RangeProof(pub Vec<u8>);
/// Implement the hashing function for RangeProof for use in the MMR
impl Hashable for RangeProof {
    fn hash(&self) -> Vec<u8> {
        HashDigest::new().chain(&self.0).result().to_vec()
    }
}

impl ByteArray for RangeProof {
    fn to_vec(&self) -> Vec<u8> {
        self.0.clone()
    }

    fn from_vec(v: &Vec<u8>) -> Result<Self, ByteArrayError> {
        Ok(RangeProof { 0: v.clone() })
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self, ByteArrayError> {
        Ok(RangeProof { 0: bytes.to_vec() })
    }

    fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}
