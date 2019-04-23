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

// Used in tests only

use crate::types::{SecretKey, PublicKey, CommitmentFactory};
use rand::{Rng, CryptoRng};
use crate::transaction::{TransactionInput, UnblindedOutput, OutputFeatures};
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    keys::{PublicKey as PK, SecretKey as SK},
};

pub struct TestParams {
    pub spend_key: SecretKey,
    pub change_key: SecretKey,
    pub offset: SecretKey,
    pub nonce: SecretKey,
    pub public_nonce: PublicKey,
}

impl TestParams {
    pub fn new<R: Rng + CryptoRng>(rng: &mut R) -> TestParams {
        let r = SecretKey::random(rng);
        TestParams {
            spend_key: SecretKey::random(rng),
            change_key: SecretKey::random(rng),
            offset: SecretKey::random(rng),
            public_nonce: PublicKey::from_secret_key(&r),
            nonce: r,
        }
    }
}

pub fn make_input<R: Rng + CryptoRng>(rng: &mut R, val: u64) -> (TransactionInput, UnblindedOutput) {
    let key = SecretKey::random(rng);
    let v = SecretKey::from(val);
    let commitment = CommitmentFactory::create(&key, &v);
    let input = TransactionInput::new(OutputFeatures::empty(), commitment);
    (input, UnblindedOutput::new(val, key, None))
}

