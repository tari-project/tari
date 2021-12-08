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

use digest::Digest;
use rand::{CryptoRng, Rng};
use tari_crypto::{
    keys::{PublicKey, SecretKey},
    signatures::{SchnorrSignature, SchnorrSignatureError},
    tari_utilities::message_format::MessageFormat,
};

use crate::types::{Challenge, CommsPublicKey};

pub fn sign_challenge<R>(
    rng: &mut R,
    secret_key: <CommsPublicKey as PublicKey>::K,
    challenge: Challenge,
) -> Result<SchnorrSignature<CommsPublicKey, <CommsPublicKey as PublicKey>::K>, SchnorrSignatureError>
where
    R: CryptoRng + Rng,
{
    let nonce = <CommsPublicKey as PublicKey>::K::random(rng);
    SchnorrSignature::sign(secret_key, nonce, &challenge.finalize())
}

/// Verify that the signature is valid for the challenge
pub fn verify_challenge(public_key: &CommsPublicKey, signature: &[u8], challenge: Challenge) -> bool {
    match SchnorrSignature::<CommsPublicKey, <CommsPublicKey as PublicKey>::K>::from_binary(signature) {
        Ok(signature) => signature.verify_challenge(public_key, &challenge.finalize()),
        Err(_) => false,
    }
}
