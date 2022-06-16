// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::sync::Arc;

use tari_common_types::types::{CommitmentFactory, RangeProofService, MAX_RANGE_PROOF_RANGE};
use tari_crypto::ristretto::pedersen::commitment_factory::PedersenCommitmentFactory;

/// A convenience struct wrapping cryptographic factories that are used throughout the rest of the code base
/// Uses Arcs internally so calling clone on this is cheap, no need to wrap this in an Arc
pub struct CryptoFactories {
    pub commitment: Arc<CommitmentFactory>,
    pub commitment_dalek_bulletproofs: Arc<PedersenCommitmentFactory>, // TODO: Remove this when 'bulletproofs_plus'
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
        // TODO: Remove this when 'bulletproofs_plus'
        let commitment_dalek_bulletproofs = Arc::new(PedersenCommitmentFactory::default());
        // TODO: Replace `commitment_dalek_bulletproofs` with `commitment` when 'bulletproofs_plus'
        let range_proof = Arc::new(RangeProofService::new(max_proof_range, &commitment_dalek_bulletproofs).unwrap());
        Self {
            commitment,
            commitment_dalek_bulletproofs,
            range_proof,
        }
    }
}

/// Uses Arc's internally so calling clone on this is cheap, no need to wrap this in an Arc
impl Clone for CryptoFactories {
    fn clone(&self) -> Self {
        Self {
            commitment: self.commitment.clone(),
            commitment_dalek_bulletproofs: self.commitment_dalek_bulletproofs.clone(),
            range_proof: self.range_proof.clone(),
        }
    }
}
