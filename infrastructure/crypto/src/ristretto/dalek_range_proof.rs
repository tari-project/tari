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
//
// Portions of this file were originally copyrighted (c) 2018 The Grin Developers, issued under the Apache License,
// Version 2.0, available at http://www.apache.org/licenses/LICENSE-2.0.

use crate::{
    range_proof::{RangeProofError, RangeProofService},
    ristretto::{
        pedersen::{PedersenCommitment, PedersenCommitmentFactory},
        RistrettoPublicKey,
        RistrettoSecretKey,
    },
};
use bulletproofs::{BulletproofGens, PedersenGens, RangeProof as DalekProof};
use merlin::Transcript;

/// A wrapper around the Dalek library implementation of Bulletproof range proofs.
pub struct DalekRangeProofService {
    range: usize,
    pc_gens: PedersenGens,
    bp_gens: BulletproofGens,
}

const MASK: usize = 0b111_1000; // Mask for 8,16,32,64; the valid ranges on the Dalek library

impl DalekRangeProofService {
    /// Create a new RangeProofService. The Dalek library can only generate proofs for ranges between [0; 2^range),
    /// where valid range values are 8, 16, 32 and 64.
    pub fn new(range: usize, base: &PedersenCommitmentFactory) -> Result<DalekRangeProofService, RangeProofError> {
        if range == 0 || (range | MASK != MASK) {
            return Err(RangeProofError::InitializationError);
        }
        let pc_gens = PedersenGens {
            B_blinding: base.G,
            B: base.H,
        };
        let bp_gens = BulletproofGens::new(64, 1);
        Ok(DalekRangeProofService {
            range,
            pc_gens,
            bp_gens,
        })
    }
}

impl RangeProofService for DalekRangeProofService {
    type K = RistrettoSecretKey;
    type P = Vec<u8>;
    type PK = RistrettoPublicKey;

    fn construct_proof(&self, key: &RistrettoSecretKey, value: u64) -> Result<Vec<u8>, RangeProofError> {
        let mut pt = Transcript::new(b"tari");
        let k = key.0;
        let (proof, _) = DalekProof::prove_single(&self.bp_gens, &self.pc_gens, &mut pt, value, &k, self.range)
            .map_err(|_| RangeProofError::ProofConstructionError)?;
        Ok(proof.to_bytes())
    }

    fn verify(&self, proof: &Self::P, commitment: &PedersenCommitment) -> bool {
        let rp = DalekProof::from_bytes(&proof).map_err(|_| RangeProofError::InvalidProof);
        if rp.is_err() {
            return false;
        }
        let rp = rp.unwrap();
        let mut pt = Transcript::new(b"tari");
        let c = &commitment.0;
        rp.verify_single(&self.bp_gens, &self.pc_gens, &mut pt, &c.compressed, self.range)
            .is_ok()
    }
}

#[cfg(test)]
mod test {
    use crate::{
        commitment::HomomorphicCommitmentFactory,
        keys::SecretKey,
        range_proof::{RangeProofError, RangeProofService},
        ristretto::{
            dalek_range_proof::DalekRangeProofService,
            pedersen::PedersenCommitmentFactory,
            RistrettoSecretKey,
        },
    };
    use rand::OsRng;

    #[test]
    fn create_and_verify_proof() {
        let base = PedersenCommitmentFactory::default();
        let n: usize = 5;
        let prover = DalekRangeProofService::new(1 << 5, &base).unwrap();
        let mut rng = OsRng::new().unwrap();
        let k = RistrettoSecretKey::random(&mut rng);
        let v = RistrettoSecretKey::from(42);
        let commitment_factory: PedersenCommitmentFactory = PedersenCommitmentFactory::default();
        let c = commitment_factory.commit(&k, &v);
        let proof = prover.construct_proof(&k, 42).unwrap();
        assert_eq!(proof.len(), (2 * n + 9) * 32);
        assert!(prover.verify(&proof, &c));
        // Invalid value
        let v2 = RistrettoSecretKey::from(43);
        let c = commitment_factory.commit(&k, &v2);
        assert_eq!(prover.verify(&proof, &c), false);
        // Invalid key
        let k = RistrettoSecretKey::random(&mut rng);
        let c = commitment_factory.commit(&k, &v);
        assert_eq!(prover.verify(&proof, &c), false);
        // Both invalid
        let c = commitment_factory.commit(&k, &v2);
        assert_eq!(prover.verify(&proof, &c), false);
    }

    #[test]
    fn non_power_of_two_range() {
        let base = PedersenCommitmentFactory::default();
        match DalekRangeProofService::new(10, &base) {
            Err(RangeProofError::InitializationError) => (),
            Err(_) => panic!("Wrong error type"),
            Ok(_) => panic!("Should fail with non power of two range"),
        }
    }

    #[test]
    fn cannot_create_proof_for_out_of_range_value() {
        let base = PedersenCommitmentFactory::default();
        let prover = DalekRangeProofService::new(8, &base).unwrap();
        let in_range = 255;
        let out_of_range = 256;
        let mut rng = OsRng::new().unwrap();
        let k = RistrettoSecretKey::random(&mut rng);
        // Test with value in range
        let v = RistrettoSecretKey::from(in_range);
        let commitment_factory = PedersenCommitmentFactory::default();
        let c = commitment_factory.commit(&k, &v);
        let proof = prover.construct_proof(&k, in_range).unwrap();
        assert!(prover.verify(&proof, &c));
        // Test value out of range
        let proof = prover.construct_proof(&k, out_of_range).unwrap();
        // Test every single value from 0..255 - the proof should fail for every one
        for i in 0..257 {
            let v = RistrettoSecretKey::from(i);
            let c = commitment_factory.commit(&k, &v);
            assert_eq!(prover.verify(&proof, &c), false);
        }
    }
}
