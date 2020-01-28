// Copyright 2019. The Tari Project
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

use crate::transactions::bullet_rangeproofs::BulletRangeProof;
use std::sync::Arc;
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

/// Define the explicit Signature implementation for the Tari base layer. A different signature scheme can be
/// employed by redefining this type.
pub type Signature = RistrettoSchnorr;

/// Define the explicit Commitment implementation for the Tari base layer.
pub type Commitment = PedersenCommitment;
pub type CommitmentFactory = PedersenCommitmentFactory;

/// Define the explicit Secret key implementation for the Tari base layer.
pub type PrivateKey = RistrettoSecretKey;
pub type BlindingFactor = RistrettoSecretKey;

/// Define the hash function that will be used to produce a signature challenge
pub type SignatureHasher = Blake256;

/// Define the explicit Public key implementation for the Tari base layer
pub type PublicKey = RistrettoPublicKey;

/// Specify the Hash function for general hashing
pub type HashDigest = Blake256;

/// Specify the digest type for signature challenges
pub type Challenge = Blake256;

/// The type of output that `Challenge` produces
pub type MessageHash = Vec<u8>;

/// Specify the range proof type
pub type RangeProofService = DalekRangeProofService;

/// Specify the range proof
pub type RangeProof = BulletRangeProof;

/// Define the data type that is used to store results of `HashDigest`
pub type HashOutput = Vec<u8>;

pub const MAX_RANGE_PROOF_RANGE: usize = 64; // 2^64

/// A convenience struct wrapping cryptographic factories that are used through-out the rest of the code base
/// Uses Arc's internally so calling clone on this is cheap, no need to wrap this in an Arc
pub struct CryptoFactories {
    pub commitment: Arc<CommitmentFactory>,
    pub range_proof: Arc<RangeProofService>,
}

impl Default for CryptoFactories {
    /// Return a default set of crypto factories based on Pedersen commitments with G and H defined in
    /// [pedersen.rs](/infrastructure/crypto/src/ristretto/pedersen.rs), and an associated range proof factory with a
    /// range of `[0; 2^64)`.
    fn default() -> Self {
        CryptoFactories::new(MAX_RANGE_PROOF_RANGE)
    }
}

impl CryptoFactories {
    /// Create a new set of crypto factories.
    ///
    /// ## Parameters
    ///
    /// * `max_proof_range`: Sets the the maximum value in range proofs, where `max = 2^max_proof_range`
    pub fn new(max_proof_range: usize) -> Self {
        let commitment = Arc::new(CommitmentFactory::default());
        let range_proof = Arc::new(RangeProofService::new(max_proof_range, &commitment).unwrap());
        Self {
            commitment,
            range_proof,
        }
    }
}

/// Uses Arc's internally so calling clone on this is cheap, no need to wrap this in an Arc
impl Clone for CryptoFactories {
    fn clone(&self) -> Self {
        Self {
            commitment: self.commitment.clone(),
            range_proof: self.range_proof.clone(),
        }
    }
}
