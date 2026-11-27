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
use tari_script::{MAX_MULTISIG_LIMIT, Opcode, TariScript};

use crate::{
    key_manager::{TariKeyId, TransactionKeyManagerInterface},
    transaction_components::{TransactionError, one_sided::diffie_hellman_stealth_domain_hasher},
};

/// Validates that a multisig spend supplies exactly the script's threshold `m` signatures and returns `m`.
///
/// Every supplied signature is pushed onto the input stack and `CheckMultiSigVerify` consumes exactly `m` of them,
/// so any excess is left on the stack and consensus rejects the spend.
pub fn validate_multisig_spend_signature_count(
    script: &TariScript,
    signature_count: usize,
) -> Result<u8, TransactionError> {
    let (_keys, threshold) = get_multi_sig_script_components(script)
        .ok_or_else(|| TransactionError::BuilderError("no keys found".to_string()))?;
    if signature_count != usize::from(threshold) {
        return Err(TransactionError::BuilderError(format!(
            "Invalid signature count for multisig spend: expected exactly {}, got {}",
            threshold, signature_count
        )));
    }
    Ok(threshold)
}

pub fn is_multisig_utxo(tari_script: &TariScript) -> bool {
    get_multi_sig_script_components(tari_script).is_some()
}

/// The keys and threshold of a script's `CheckMultiSigVerify`, or `None` if the opcode is one consensus can never
/// execute (`m = 0`, `m > n`, `n` not matching the key list, or `n > MAX_MULTISIG_LIMIT`).
///
/// This is the spend-side rule, so it accepts everything consensus accepts. New deposits use the stricter
/// `validate_multisig_threshold`, which also caps the script size.
pub fn get_multi_sig_script_components(script: &TariScript) -> Option<(Vec<CompressedPublicKey>, u8)> {
    for op in script.as_slice() {
        if let Opcode::CheckMultiSigVerify(m, n, keys, _msg) = op {
            let executable = *m >= 1 && m <= n && usize::from(*n) == keys.len() && *n <= MAX_MULTISIG_LIMIT;
            return executable.then(|| (keys.clone(), *m));
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

    fn sample_keys(count: usize) -> Vec<CompressedPublicKey> {
        (1..=count)
            .map(|i| CompressedPublicKey::from_secret_key(&PrivateKey::from(i as u64)))
            .collect()
    }

    fn multisig_script(m: u8, n: u8, keys: Vec<CompressedPublicKey>) -> TariScript {
        TariScript::new(vec![Opcode::CheckMultiSigVerify(m, n, keys, Box::new([0u8; 32]))]).unwrap()
    }

    #[test]
    fn spend_signature_count_must_equal_threshold() {
        let script = multisig_script(2, 3, sample_keys(3));
        assert_eq!(validate_multisig_spend_signature_count(&script, 2).unwrap(), 2);
        for count in [1, 3] {
            let err = validate_multisig_spend_signature_count(&script, count)
                .unwrap_err()
                .to_string();
            assert!(err.contains(&format!("expected exactly 2, got {count}")), "{err}");
        }
        let not_multisig = TariScript::new(vec![Opcode::Nop]).unwrap();
        let err = validate_multisig_spend_signature_count(&not_multisig, 1)
            .unwrap_err()
            .to_string();
        assert!(err.contains("no keys found"), "{err}");
    }

    #[test]
    fn get_multi_sig_script_components_rejects_unexecutable_opcodes() {
        let keys = sample_keys(2);
        let valid = multisig_script(2, 2, keys.clone());
        assert!(is_multisig_utxo(&valid));
        assert_eq!(get_multi_sig_script_components(&valid), Some((keys.clone(), 2)));

        let limit = usize::from(MAX_MULTISIG_LIMIT);
        for script in [
            multisig_script(0, 2, keys.clone()),
            multisig_script(3, 2, keys.clone()),
            multisig_script(2, MAX_MULTISIG_LIMIT + 1, sample_keys(limit + 1)),
        ] {
            assert!(!is_multisig_utxo(&script));
            assert_eq!(get_multi_sig_script_components(&script), None);
        }

        // Consensus accepts repeated keys and up to 32 keys, so existing outputs of that shape stay spendable.
        let repeated = keys.first().unwrap().clone();
        let duplicate = multisig_script(2, 2, vec![repeated.clone(), repeated.clone()]);
        assert_eq!(
            get_multi_sig_script_components(&duplicate),
            Some((vec![repeated.clone(), repeated], 2))
        );
        let at_limit = sample_keys(limit);
        assert_eq!(
            get_multi_sig_script_components(&multisig_script(2, MAX_MULTISIG_LIMIT, at_limit.clone())),
            Some((at_limit, 2))
        );
    }

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
