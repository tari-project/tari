// Copyright 2020. The Tari Project
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use digest::Digest;
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    ristretto::{
        pedersen::{PedersenCommitment, PedersenCommitmentFactory},
        RistrettoSecretKey,
    },
};
use tari_utilities::{ByteArray, ByteArrayError};
use thiserror::Error;

use crate::{HashValue, TariScript};

#[derive(Debug, Clone, Error, PartialEq)]
pub enum ScriptCommitmentError {
    #[error("The digest function must produce output of exactly 32 bytes")]
    InvalidDigestLength,
    #[error("An unexpected error occurred: {0}")]
    Unexpected(String),
}

impl From<ByteArrayError> for ScriptCommitmentError {
    fn from(e: ByteArrayError) -> Self {
        match e {
            ByteArrayError::ConversionError(s) => ScriptCommitmentError::Unexpected(s),
            ByteArrayError::IncorrectLength => ScriptCommitmentError::InvalidDigestLength,
        }
    }
}

/// A modified Pedersen commitment that includes a commitment to a Tari Script hash.
///
/// The modified script definition is _ C* = C + H(C||s).G
///
/// The ScriptCommitment is then still a Pedersen commitment with a key vale of _k + H(C||s)_. This struct is a
/// intermediate container struct that holds the individual terms in the equation above for easy reference.
///
/// To calculate the modified Pedersen commitment from an instance of `ScriptCommitment`, use
/// [ScriptCommitmentFactory::script_to_pedersen].
pub struct ScriptCommitment {
    blinding_factor: RistrettoSecretKey,
    value: u64,
    script_hash: HashValue,
    adj_blinding_factor: RistrettoSecretKey,
    naked_commitment: PedersenCommitment,
}

impl ScriptCommitment {
    /// Return a reference to the blinding factor of the naked commitment, i.e. `k` in  _C = k.G + v.H_
    pub fn blinding_factor(&self) -> &RistrettoSecretKey {
        &self.blinding_factor
    }

    /// Return the value that this ScriptCommitment is committing.
    pub fn value(&self) -> u64 {
        self.value
    }

    /// Return a slice representing the hash of the Tari script that this `ScriptCommitment` is committing.
    pub fn script_hash(&self) -> &[u8] {
        &self.script_hash
    }

    /// Return a reference to the adjusted blinding factor of the naked commitment, i.e. `k + H(C||s)`
    pub fn adj_blinding_factor(&self) -> &RistrettoSecretKey {
        &self.adj_blinding_factor
    }

    /// Return a reference to the naked commitment, i.e.  _C = k.G + v.H_
    pub fn naked_commitment(&self) -> &PedersenCommitment {
        &self.naked_commitment
    }
}

/// A factory for generating script commitments. The default [PedersenCommitmentFactory] is used by default.
///
/// ## Example
///
/// To create a script commitment factory, generate the corresponding pedersen commitment and attest that the
/// original data opens the commitment:
/// ```
/// # use blake2::Blake2b;
/// # use rand::RngCore;
/// # use tari_crypto::ristretto::RistrettoSecretKey;
/// # use tari_script::{ScriptCommitmentFactory, TariScript};
/// # use tari_crypto::{
///     common::Blake256,
///     keys::SecretKey,
/// };
///
/// let mut rng = rand::thread_rng();
/// let k = RistrettoSecretKey::random(&mut rng);
/// let scf = ScriptCommitmentFactory::default();
/// let value = rng.next_u64();
/// let script = TariScript::default();
/// let sc = scf.commit_script::<Blake256>(&k, value, &script).unwrap();
/// let c = scf.script_to_pedersen(&sc);
/// assert!(scf.open_script::<Blake256>(&k, value, &script, &c));
/// ```

#[derive(Default)]
pub struct ScriptCommitmentFactory {
    factory: PedersenCommitmentFactory,
}

impl ScriptCommitmentFactory {
    /// Create a new script commitment form the given blinding factor (key), value and Tari script instance, using
    /// the generators of this factory.
    pub fn commit_script<D: Digest>(
        &self,
        key: &RistrettoSecretKey,
        value: u64,
        script: &TariScript,
    ) -> Result<ScriptCommitment, ScriptCommitmentError> {
        if D::output_size() < 32 {
            return Err(ScriptCommitmentError::InvalidDigestLength);
        }
        let commitment = self.factory.commit_value(key, value);
        let adj_blinding_factor = ScriptCommitmentFactory::adjusted_blinding_factor::<D>(key, &commitment, script)?;
        let script_hash = script
            .as_hash::<D>()
            .map_err(|_| ScriptCommitmentError::InvalidDigestLength)?;
        Ok(ScriptCommitment {
            blinding_factor: key.clone(),
            value,
            script_hash,
            adj_blinding_factor,
            naked_commitment: commitment,
        })
    }

    /// Return the adjusted Pedersen commitment associated with this `ScriptCommitment`, i.e. C = C + H(C||s).G
    pub fn script_to_pedersen(&self, sc: &ScriptCommitment) -> PedersenCommitment {
        self.factory.commit_value(&sc.adj_blinding_factor, sc.value)
    }

    /// Test whether the given private key, value and script open the given commitment
    pub fn open_script<D: Digest>(
        &self,
        k: &RistrettoSecretKey,
        v: u64,
        script: &TariScript,
        commitment: &PedersenCommitment,
    ) -> bool {
        match self.commit_script::<D>(k, v, script) {
            Ok(sc) => commitment == &self.script_to_pedersen(&sc),
            _ => false,
        }
    }

    /// Returns the adjusted blinding factor, _k + H(C||s)_
    fn adjusted_blinding_factor<D: Digest>(
        key: &RistrettoSecretKey,
        c: &PedersenCommitment,
        s: &TariScript,
    ) -> Result<RistrettoSecretKey, ScriptCommitmentError> {
        let script_hash = s
            .as_hash::<D>()
            .map_err(|_| ScriptCommitmentError::InvalidDigestLength)?;
        let h = D::new().chain(c.as_bytes()).chain(&script_hash[..]).finalize();
        let hash = RistrettoSecretKey::from_bytes(&h[..]).map_err(ScriptCommitmentError::from)?;
        Ok(key + &hash)
    }
}

#[cfg(test)]
mod tests {
    use blake2::Blake2b;
    use rand::RngCore;
    use tari_crypto::{common::Blake256, keys::PublicKey, ristretto::RistrettoPublicKey};

    use super::*;
    use crate::TariScript;

    #[test]
    fn script_commitment_default_script() {
        let mut rng = rand::thread_rng();
        let (k, _) = RistrettoPublicKey::random_keypair(&mut rng);
        let scf = ScriptCommitmentFactory::default();
        let value = rng.next_u64();
        let script = TariScript::default();
        let sc = scf.commit_script::<Blake256>(&k, value, &script).unwrap();
        let c = scf.script_to_pedersen(&sc);
        assert!(scf.open_script::<Blake256>(&k, value, &script, &c));
    }

    #[test]
    fn invalid_digest() {
        let mut rng = rand::thread_rng();
        let (k, _) = RistrettoPublicKey::random_keypair(&mut rng);
        let scf = ScriptCommitmentFactory::default();
        let value = rng.next_u64();
        let script = TariScript::default();
        let sc = scf.commit_script::<Blake2b>(&k, value, &script);
        assert_eq!(sc.err(), Some(ScriptCommitmentError::InvalidDigestLength))
    }
}
