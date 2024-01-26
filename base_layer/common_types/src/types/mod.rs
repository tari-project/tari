// Copyright 2020. The Tari Project
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

mod bullet_rangeproofs;
mod fixed_hash;

use blake2::Blake2b;
pub use bullet_rangeproofs::BulletRangeProof;
use digest::consts::{U32, U64};
use tari_crypto::{
    hasher,
    ristretto::{
        bulletproofs_plus::BulletproofsPlusService,
        pedersen::{extended_commitment_factory::ExtendedPedersenCommitmentFactory, PedersenCommitment},
        RistrettoComAndPubSig,
        RistrettoPublicKey,
        RistrettoSchnorr,
        RistrettoSchnorrWithDomain,
        RistrettoSecretKey,
    },
};

pub type BlockHash = FixedHash;

pub use fixed_hash::{FixedHash, FixedHashSizeError};

/// Define the explicit Signature implementation for the Tari base layer. A different signature scheme can be
/// employed by redefining this type.
pub type Signature = RistrettoSchnorr;
/// Define a generic signature type using a hash domain.
pub type SignatureWithDomain<H> = RistrettoSchnorrWithDomain<H>;
/// Define the explicit Commitment Signature implementation for the Tari base layer.
pub type ComAndPubSignature = RistrettoComAndPubSig;

/// Define the explicit Commitment implementation for the Tari base layer.
pub type Commitment = PedersenCommitment;
pub type CommitmentFactory = ExtendedPedersenCommitmentFactory;

/// Define the explicit Public key implementation for the Tari base layer
pub type PublicKey = RistrettoPublicKey;

/// Define the explicit Secret key implementation for the Tari base layer.
pub type PrivateKey = RistrettoSecretKey;

/// Define the hash function that will be used to produce a signature challenge
pub type SignatureHasher = Blake2b<U64>;

/// Specify the digest type for signature challenges
pub type Challenge = Blake2b<U64>;

/// Define the data type that is used to store results of a hash output
pub type HashOutput = FixedHash;

pub const RANGE_PROOF_BIT_LENGTH: usize = 64; // 2^64
pub const RANGE_PROOF_AGGREGATION_FACTOR: usize = 1;

/// Specify the range proof type
pub type RangeProofService = BulletproofsPlusService;

/// Specify the range proof
pub type RangeProof = BulletRangeProof;

use tari_crypto::{hash_domain, hashing::DomainSeparatedHasher};

hasher!(
    Blake2b<U64>,
    WalletHasher,
    "com.tari.base_layer.wallet",
    1,
    wallet_hasher
);

hash_domain!(
    BulletRangeProofHashDomain,
    "com.tari.base_layer.common_types.bullet_rangeproofs",
    1
);

pub type BulletRangeProofHasherBlake256 = DomainSeparatedHasher<Blake2b<U32>, BulletRangeProofHashDomain>;
