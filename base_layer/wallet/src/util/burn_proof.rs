//  Copyright 2023. The Tari Project
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

use tari_common_types::types::{Commitment, PrivateKey, PublicKey};
use tari_comms::types::CommsDHKE;
use tari_utilities::ByteArray;

use crate::types::ConfidentialOutputHasher;

/// Derives a shared DH spending key for a burnt output using a claim public key
pub fn derive_diffie_hellman_burn_claim_spend_key(
    private_key: &PrivateKey,
    claim_public_key: &PublicKey,
) -> PrivateKey {
    let private_key = PrivateKey::from_bytes(CommsDHKE::new(private_key, claim_public_key).as_bytes()).unwrap();
    let hash = ConfidentialOutputHasher::new("spend_key")
        .chain(&private_key)
        .finalize();
    PrivateKey::from_bytes(hash.as_ref()).expect("'DomainSeparatedHash<Blake256>' has correct size")
}

/// Derives a shared DH value encryption key for a burnt output using a claim public key
pub fn derive_burn_claim_encryption_key(private_key: &PrivateKey, commitment: &Commitment) -> PrivateKey {
    let hash = ConfidentialOutputHasher::new("encryption_key")
        .chain(private_key)
        .chain(commitment)
        .finalize();
    PrivateKey::from_bytes(hash.as_ref()).expect("'DomainSeparatedHash<Blake256>' has correct size")
}
