// Copyright 2025 The Tari Project
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
use tari_common_types::types::{CompressedPublicKey, PrivateKey, UncompressedPublicKey};
use tari_crypto::keys::{PublicKey, SecretKey};
use tari_script::{Opcode, TariScript};

use crate::{
    legacy_key_manager::{TariKeyId, TransactionKeyManagerInterface},
    transaction_components::{one_sided::diffie_hellman_stealth_domain_hasher, TransactionError},
};

pub fn is_multisig_utxo(tari_script: &TariScript) -> bool {
    tari_script
        .as_slice()
        .iter()
        .any(|op| matches!(op, Opcode::CheckMultiSigVerify(..)))
}

pub fn get_multi_sig_script_components(script: &TariScript) -> Option<(Vec<CompressedPublicKey>, u8)> {
    for op in script.as_slice() {
        if let Opcode::CheckMultiSigVerify(m, _n, keys, _msg) = op {
            return Some((keys.clone(), *m));
        }
    }

    None
}

pub async fn derive_multisig_ephemeral_pubkey<KM: TransactionKeyManagerInterface>(
    key_manager: &KM,
    public_key: &CompressedPublicKey,
    sender_offset_key: &TariKeyId,
) -> Result<CompressedPublicKey, TransactionError> {
    let dh_shared_secret = key_manager
        .get_diffie_hellman_shared_secret(sender_offset_key, public_key)
        .await?;

    let stealth_hash = diffie_hellman_stealth_domain_hasher(dh_shared_secret);
    let private_key = PrivateKey::from_uniform_bytes(stealth_hash.as_ref())?;

    let shared_secret = UncompressedPublicKey::from_secret_key(&private_key);
    Ok(CompressedPublicKey::new_from_pk(
        public_key.to_public_key()? + shared_secret,
    ))
}

pub async fn derive_multisig_ephemeral_pubkeys<KM: TransactionKeyManagerInterface>(
    key_manager: &KM,
    public_keys: &[CompressedPublicKey],
    sender_offset_key: &TariKeyId,
) -> Result<Vec<CompressedPublicKey>, TransactionError> {
    let mut ephemeral_pubkeys = Vec::new();
    for pub_key in public_keys {
        ephemeral_pubkeys.push(derive_multisig_ephemeral_pubkey(key_manager, pub_key, sender_offset_key).await?);
    }
    Ok(ephemeral_pubkeys)
}
