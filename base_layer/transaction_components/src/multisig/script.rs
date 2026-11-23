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

/// Maximum number of public keys permitted in a `CheckMultiSig*` opcode. Mirrors the private `MAX_MULTISIG_LIMIT` in
/// `infrastructure/tari_script/src/script.rs`, which is the consensus rule; keep the two in step.
pub const MAX_MULTISIG_LIMIT: u8 = 32;

/// Validates multisig threshold `party_number` (`m`) and participant count `key_count` (`n`) against `TariScript`
/// execution bounds (`1 <= m <= n <= MAX_MULTISIG_LIMIT`).
pub fn validate_multisig_bounds(party_number: u8, key_count: usize) -> Result<u8, TransactionError> {
    let key_count_u8 = u8::try_from(key_count)
        .ok()
        .filter(|&n| (1..=MAX_MULTISIG_LIMIT).contains(&n))
        .ok_or_else(|| {
            TransactionError::BuilderError(format!(
                "Invalid multisig participant count: expected 1..={}, got {}",
                MAX_MULTISIG_LIMIT, key_count
            ))
        })?;
    if party_number == 0 || party_number > key_count_u8 {
        return Err(TransactionError::BuilderError(format!(
            "Invalid multisig threshold party_number={}, participants={}",
            party_number, key_count
        )));
    }
    Ok(key_count_u8)
}

/// Validates the parameters of a new multisig deposit: threshold `party_number` (`m`) and participant count
/// (`n <= MAX_MULTISIG_LIMIT`), so that a deposit cannot create an unspendable UTXO.
pub fn validate_multisig_parameters(
    party_number: u8,
    public_keys: &[CompressedPublicKey],
) -> Result<u8, TransactionError> {
    validate_multisig_bounds(party_number, public_keys.len())
}

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

pub fn get_multi_sig_script_components(script: &TariScript) -> Option<(Vec<CompressedPublicKey>, u8)> {
    for op in script.as_slice() {
        if let Opcode::CheckMultiSigVerify(m, n, keys, _msg) = op {
            if usize::from(*n) != keys.len() || validate_multisig_bounds(*m, keys.len()).is_err() {
                return None;
            }
            return Some((keys.clone(), *m));
        }
    }

    None
}

// Ristretto point/scalar arithmetic, not integer arithmetic: these operators cannot overflow.
#[allow(clippy::arithmetic_side_effects)]
pub fn derive_multisig_ephemeral_pubkey<KM: TransactionKeyManagerInterface>(
    key_manager: &KM,
    public_key: &CompressedPublicKey,
    sender_offset_key: &TariKeyId,
) -> Result<CompressedPublicKey, TransactionError> {
    let dh_shared_secret = key_manager.get_diffie_hellman_shared_secret(sender_offset_key, public_key)?;

    let stealth_hash = diffie_hellman_stealth_domain_hasher(&dh_shared_secret);
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
    use tari_common_types::types::{CompressedPublicKey, PrivateKey};
    use tari_script::{Opcode, TariScript};

    use super::{
        MAX_MULTISIG_LIMIT,
        get_multi_sig_script_components,
        is_multisig_utxo,
        validate_multisig_bounds,
        validate_multisig_parameters,
        validate_multisig_spend_signature_count,
    };

    fn sample_keys(count: usize) -> Vec<CompressedPublicKey> {
        (1..=count)
            .map(|i| CompressedPublicKey::from_secret_key(&PrivateKey::from(i as u64)))
            .collect()
    }

    fn err_text<T: std::fmt::Debug>(res: Result<T, super::TransactionError>) -> String {
        res.unwrap_err().to_string()
    }

    #[test]
    fn validate_multisig_parameters_enforces_bounds() {
        let keys = sample_keys(3);
        assert_eq!(validate_multisig_parameters(2, &keys).unwrap(), 3);
        assert_eq!(
            validate_multisig_bounds(MAX_MULTISIG_LIMIT, usize::from(MAX_MULTISIG_LIMIT)).unwrap(),
            MAX_MULTISIG_LIMIT
        );
        assert!(err_text(validate_multisig_bounds(0, 3)).contains("Invalid multisig threshold party_number=0"));
        assert!(err_text(validate_multisig_bounds(4, 3)).contains("Invalid multisig threshold party_number=4"));
        assert!(err_text(validate_multisig_bounds(1, 0)).contains("expected 1..=32, got 0"));
        assert!(
            err_text(validate_multisig_bounds(1, usize::from(MAX_MULTISIG_LIMIT) + 1))
                .contains("expected 1..=32, got 33")
        );

        let too_many_keys = sample_keys(usize::from(MAX_MULTISIG_LIMIT) + 1);
        assert!(err_text(validate_multisig_parameters(2, &too_many_keys)).contains("expected 1..=32, got 33"));
    }

    #[test]
    fn spend_signature_count_must_equal_threshold() {
        let keys = sample_keys(3);
        let script = TariScript::new(vec![Opcode::CheckMultiSigVerify(2, 3, keys, Box::new([0u8; 32]))]).unwrap();
        assert_eq!(validate_multisig_spend_signature_count(&script, 2).unwrap(), 2);
        assert!(err_text(validate_multisig_spend_signature_count(&script, 1)).contains("expected exactly 2, got 1"));
        assert!(err_text(validate_multisig_spend_signature_count(&script, 3)).contains("expected exactly 2, got 3"));
        let not_multisig = TariScript::new(vec![Opcode::Nop]).unwrap();
        assert!(err_text(validate_multisig_spend_signature_count(&not_multisig, 1)).contains("no keys found"));
    }

    #[test]
    fn get_multi_sig_script_components_rejects_malformed_multisig_opcodes() {
        let keys = sample_keys(2);
        let valid_script = TariScript::new(vec![Opcode::CheckMultiSigVerify(
            2,
            2,
            keys.clone(),
            Box::new([0u8; 32]),
        )])
        .unwrap();
        assert!(is_multisig_utxo(&valid_script));
        assert_eq!(get_multi_sig_script_components(&valid_script), Some((keys.clone(), 2)));

        let zero_threshold = TariScript::new(vec![Opcode::CheckMultiSigVerify(
            0,
            2,
            keys.clone(),
            Box::new([0u8; 32]),
        )])
        .unwrap();
        assert!(!is_multisig_utxo(&zero_threshold));
        assert_eq!(get_multi_sig_script_components(&zero_threshold), None);

        let threshold_exceeds_n = TariScript::new(vec![Opcode::CheckMultiSigVerify(
            3,
            2,
            keys.clone(),
            Box::new([0u8; 32]),
        )])
        .unwrap();
        assert_eq!(get_multi_sig_script_components(&threshold_exceeds_n), None);

        // Spend side accepts whatever consensus accepts, including duplicate keys, so existing outputs stay spendable.
        let repeated = keys.first().cloned().unwrap();
        let duplicate_keys = TariScript::new(vec![Opcode::CheckMultiSigVerify(
            2,
            2,
            vec![repeated.clone(), repeated.clone()],
            Box::new([0u8; 32]),
        )])
        .unwrap();
        assert_eq!(
            get_multi_sig_script_components(&duplicate_keys),
            Some((vec![repeated.clone(), repeated], 2))
        );

        let oversized_keys = sample_keys(usize::from(MAX_MULTISIG_LIMIT) + 1);
        let oversized_script = TariScript::new(vec![Opcode::CheckMultiSigVerify(
            2,
            MAX_MULTISIG_LIMIT + 1,
            oversized_keys,
            Box::new([0u8; 32]),
        )])
        .unwrap();
        assert_eq!(get_multi_sig_script_components(&oversized_script), None);
    }
}
