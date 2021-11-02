//  Copyright 2021. The Tari Project
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

use tari_crypto::{
    ristretto::{RistrettoPublicKey, RistrettoSecretKey},
    signatures::CommitmentSignature,
    tari_utilities::{ByteArray, ByteArrayError},
};

/// Define the explicit Public key implementation for the Tari base layer
pub type PublicKey = RistrettoPublicKey;

pub type ComSig = CommitmentSignature<RistrettoPublicKey, RistrettoSecretKey>;

pub fn create_com_sig_from_bytes(_bytes: &[u8]) -> Result<ComSig, ByteArrayError> {
    Ok(ComSig::default())
    // Ok(ComSig::new(
    //  HomomorphicCommitment::from_bytes(&bytes[0..32])?,
    // RistrettoSecretKey::from_bytes(&bytes[33..64])?,
    // RistrettoSecretKey::from_bytes(&bytes[64..96])?,
    // ))
}

pub fn com_sig_to_bytes(comsig: &ComSig) -> Vec<u8> {
    let mut v = Vec::from(comsig.public_nonce().as_bytes());
    v.extend_from_slice(comsig.u().as_bytes());
    v.extend_from_slice(comsig.v().as_bytes());
    v
}
