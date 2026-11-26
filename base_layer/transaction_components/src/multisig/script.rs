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
    key_manager::{TariKeyId, TransactionKeyManagerInterface},
    transaction_components::{TransactionError, one_sided::diffie_hellman_stealth_domain_hasher},
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

pub fn derive_multisig_ephemeral_pubkey<KM: TransactionKeyManagerInterface>(
    key_manager: &KM,
    public_key: &CompressedPublicKey,
    sender_offset_key: &TariKeyId,
) -> Result<CompressedPublicKey, TransactionError> {
    let dh_shared_secret = key_manager.get_diffie_hellman_shared_secret(sender_offset_key, public_key)?;
    add_stealth_tweak(public_key, &dh_shared_secret)
}

/// The public key
/// [`sign_script_message_with_spend_key_trusted`](TransactionKeyManagerInterface::sign_script_message_with_spend_key_trusted)
/// signs with: this wallet's spend key tweaked by `sender_offset_pub_key`, the key the output's creator put in the
/// script with [`derive_multisig_ephemeral_pubkey`], or the bare spend key when no offset is given.
pub fn co_signer_script_public_key<KM: TransactionKeyManagerInterface>(
    key_manager: &KM,
    sender_offset_pub_key: Option<&CompressedPublicKey>,
) -> Result<CompressedPublicKey, TransactionError> {
    let spend_key = key_manager.get_spend_key();
    match sender_offset_pub_key {
        Some(offset) => {
            let dh_shared_secret = key_manager.get_diffie_hellman_shared_secret(&spend_key.key_id, offset)?;
            add_stealth_tweak(&spend_key.pub_key, &dh_shared_secret)
        },
        None => Ok(spend_key.pub_key),
    }
}

// Ristretto point/scalar arithmetic, not integer arithmetic: these operators cannot overflow.
#[allow(clippy::arithmetic_side_effects)]
fn add_stealth_tweak(
    public_key: &CompressedPublicKey,
    dh_shared_secret: &CompressedPublicKey,
) -> Result<CompressedPublicKey, TransactionError> {
    let stealth_hash = diffie_hellman_stealth_domain_hasher(dh_shared_secret);
    let private_key = PrivateKey::from_uniform_bytes(stealth_hash.as_ref())?;

    let shared_secret = UncompressedPublicKey::from_secret_key(&private_key);
    Ok(CompressedPublicKey::new_from_pk(
        public_key.to_public_key()? + shared_secret,
    ))
}

pub fn derive_multisig_ephemeral_pubkeys<KM: TransactionKeyManagerInterface>(
    key_manager: &KM,
    public_keys: &[CompressedPublicKey],
    sender_offset_key: &TariKeyId,
) -> Result<Vec<CompressedPublicKey>, TransactionError> {
    let mut ephemeral_pubkeys = Vec::new();
    for pub_key in public_keys {
        ephemeral_pubkeys.push(derive_multisig_ephemeral_pubkey(
            key_manager,
            pub_key,
            sender_offset_key,
        )?);
    }
    Ok(ephemeral_pubkeys)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::key_manager::KeyManager;

    #[test]
    fn co_signer_key_is_the_creator_ephemeral_key_and_verifies_the_signature() {
        let creator = KeyManager::new_random().unwrap();
        let co_signer = KeyManager::new_random().unwrap();
        let sender_offset = creator.get_random_key(None, None).unwrap();
        let message = [7u8; 32];

        let in_script =
            derive_multisig_ephemeral_pubkey(&creator, &co_signer.get_spend_key().pub_key, &sender_offset.key_id)
                .unwrap();
        let signing_key = co_signer_script_public_key(&co_signer, Some(&sender_offset.pub_key)).unwrap();
        assert_eq!(signing_key, in_script);
        let signature = co_signer
            .sign_script_message_with_spend_key_trusted(&message, Some(&sender_offset.pub_key))
            .unwrap();
        assert!(
            signature
                .to_schnorr_signature()
                .unwrap()
                .verify(&signing_key.to_public_key().unwrap(), message)
        );

        assert_eq!(
            co_signer_script_public_key(&co_signer, None).unwrap(),
            co_signer.get_spend_key().pub_key
        );
    }
}
