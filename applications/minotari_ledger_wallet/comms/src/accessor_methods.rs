// Copyright 2024 The Tari Project
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

use std::sync::Mutex;

use log::debug;
use minotari_ledger_wallet_common::common_types::{AppSW, Instruction};
use once_cell::sync::Lazy;
use rand::{rngs::OsRng, RngCore};
use tari_common::configuration::Network;
use tari_common_types::{
    key_branches::TransactionKeyManagerBranch,
    types::{ComAndPubSignature, Commitment, PrivateKey, PublicKey, Signature},
};
use tari_crypto::dhke::DiffieHellmanSharedSecret;
use tari_script::CheckSigSchnorrSignature;
use tari_utilities::{hex::Hex, ByteArray};

use crate::{
    error::LedgerDeviceError,
    ledger_wallet::{Command, EXPECTED_NAME, EXPECTED_VERSION},
};

const LOG_TARGET: &str = "ledger_wallet::accessor_methods";

/// The script signature key
pub enum ScriptSignatureKey {
    Managed {
        branch: TransactionKeyManagerBranch,
        index: u64,
    },
    Derived {
        branch_key: PrivateKey,
    },
}

/// Verify that the ledger application is working properly.
pub fn verify_ledger_application() -> Result<(), LedgerDeviceError> {
    static VERIFIED: Lazy<Mutex<Option<Result<(), LedgerDeviceError>>>> = Lazy::new(|| Mutex::new(None));
    if let Ok(mut verified) = VERIFIED.try_lock() {
        if verified.is_none() {
            match verify() {
                Ok(_) => {
                    debug!(target: LOG_TARGET, "Ledger application 'Minotari Wallet' running and verified");
                    *verified = Some(Ok(()))
                },
                Err(e) => return Err(e),
            }
        }
    }
    Ok(())
}

fn verify() -> Result<(), LedgerDeviceError> {
    match ledger_get_app_name() {
        Ok(app_name) => {
            if app_name != EXPECTED_NAME {
                return Err(LedgerDeviceError::Processing(format!(
                    "Ledger application is not the 'Minotari Wallet' application: expected '{}', running '{}'.",
                    EXPECTED_NAME, app_name
                )));
            }
        },
        Err(e) => {
            return Err(LedgerDeviceError::Processing(format!(
                "Ledger application is not the 'Minotari Wallet' application ({})",
                e
            )))
        },
    }

    match ledger_get_version() {
        Ok(version) => {
            if version != EXPECTED_VERSION {
                return Err(LedgerDeviceError::Processing(format!(
                    "'Minotari Wallet' application version mismatch: expected '{}', running '{}'.",
                    EXPECTED_VERSION, version
                )));
            }
        },
        Err(e) => {
            return Err(LedgerDeviceError::Processing(format!(
                "'Minotari Wallet' application version mismatch ({})",
                e
            )))
        },
    }

    let account = OsRng.next_u64();
    let private_key_index = OsRng.next_u64();
    let private_key_branch = TransactionKeyManagerBranch::OneSidedSenderOffset;
    let mut nonce = [0u8; 32];
    OsRng.fill_bytes(&mut nonce);
    let signature_a = match ledger_get_script_schnorr_signature(account, private_key_index, private_key_branch, &nonce)
    {
        Ok(signature) => match ledger_get_public_key(account, private_key_index, private_key_branch) {
            Ok(public_key) => {
                if !signature.verify(&public_key, nonce) {
                    return Err(LedgerDeviceError::Processing(
                        "'Minotari Wallet' application could not create a valid signature. Please update the firmware \
                         on your device."
                            .to_string(),
                    ));
                }
                signature
            },
            Err(e) => {
                return Err(LedgerDeviceError::Processing(format!(
                    "'Minotari Wallet' application could not retrieve a public key ({:?}). Please update the firmware \
                     on your device.",
                    e
                )))
            },
        },
        Err(e) => {
            return Err(LedgerDeviceError::Processing(format!(
                "'Minotari Wallet' application could not create a signature ({:?}). Please update the firmware on \
                 your device.",
                e
            )))
        },
    };
    match ledger_get_script_schnorr_signature(account, private_key_index, private_key_branch, &nonce) {
        Ok(signature_b) => {
            if signature_a == signature_b {
                return Err(LedgerDeviceError::Processing(
                    "'Minotari Wallet' application is not creating unique signatures. Please update the firmware on \
                     your device."
                        .to_string(),
                ));
            }
        },
        Err(e) => {
            return Err(LedgerDeviceError::Processing(format!(
                "'Minotari Wallet' application could not create a signature ({:?}). Please update the firmware on \
                 your device.",
                e
            )))
        },
    }

    Ok(())
}

/// Get the app name from the ledger device
pub fn ledger_get_app_name() -> Result<String, LedgerDeviceError> {
    verify_ledger_application()?;

    match Command::<Vec<u8>>::build_command(OsRng.next_u64(), Instruction::GetAppName, vec![0]).execute() {
        Ok(response) => {
            let name = match std::str::from_utf8(response.data()) {
                Ok(val) => {
                    if val.is_empty() {
                        return Err(LedgerDeviceError::ApplicationNotStarted);
                    }
                    val
                },
                Err(e) => return Err(LedgerDeviceError::Processing(format!("1 GetAppName: {}", e))),
            };
            Ok(name.to_string())
        },
        Err(e) => Err(LedgerDeviceError::Processing(format!("2 GetAppName: {}", e))),
    }
}

/// Get the version from the ledger device
pub fn ledger_get_version() -> Result<String, LedgerDeviceError> {
    verify_ledger_application()?;

    match Command::<Vec<u8>>::build_command(OsRng.next_u64(), Instruction::GetVersion, vec![0]).execute() {
        Ok(response) => {
            let name = match std::str::from_utf8(response.data()) {
                Ok(val) => {
                    if val.is_empty() {
                        return Err(LedgerDeviceError::ApplicationNotStarted);
                    }
                    val
                },
                Err(e) => return Err(LedgerDeviceError::Processing(format!("1 GetVersion: {}", e))),
            };
            Ok(name.to_string())
        },
        Err(e) => Err(LedgerDeviceError::Processing(format!("2 GetVersion: {}", e))),
    }
}

/// Get the public alpha key from the ledger device
pub fn ledger_get_public_spend_key(account: u64) -> Result<PublicKey, LedgerDeviceError> {
    debug!(target: LOG_TARGET, "ledger_get_public_spend_key: account '{}'", account);
    verify_ledger_application()?;

    match Command::<Vec<u8>>::build_command(account, Instruction::GetPublicSpendKey, vec![]).execute() {
        Ok(result) => {
            if result.data().len() < 33 {
                return Err(LedgerDeviceError::Processing(format!(
                    "GetPublicSpendKey: expected 33 bytes, got {} ({:?})",
                    result.data().len(),
                    AppSW::try_from(result.retcode())?
                )));
            }
            let public_alpha = PublicKey::from_canonical_bytes(&result.data()[1..33])?;
            Ok(public_alpha)
        },
        Err(e) => Err(LedgerDeviceError::Processing(format!("GetPublicSpendKey: {}", e))),
    }
}

/// Get a public key from the ledger device
pub fn ledger_get_public_key(
    account: u64,
    index: u64,
    branch: TransactionKeyManagerBranch,
) -> Result<PublicKey, LedgerDeviceError> {
    debug!(
        target: LOG_TARGET,
        "ledger_get_public_key: account '{}', index '{}', branch '{:?}'",
        account, index, branch
    );
    verify_ledger_application()?;

    let mut data = Vec::new();
    data.extend_from_slice(&index.to_le_bytes());
    let branch_u64 = u64::from(branch.as_byte()).to_le_bytes();
    data.extend_from_slice(&branch_u64);

    match Command::<Vec<u8>>::build_command(account, Instruction::GetPublicKey, data).execute() {
        Ok(result) => {
            if result.data().len() < 33 {
                return Err(LedgerDeviceError::Processing(format!(
                    "GetPublicKey: expected 33 bytes, got {} ({:?})",
                    result.data().len(),
                    AppSW::try_from(result.retcode())?
                )));
            }
            let public_key = PublicKey::from_canonical_bytes(&result.data()[1..33])?;
            Ok(public_key)
        },
        Err(e) => Err(LedgerDeviceError::Processing(format!("GetPublicKey: {}", e))),
    }
}

/// Get the script signature from the ledger device
pub fn ledger_get_script_signature(
    account: u64,
    network: Network,
    version: u8,
    signature_key: &ScriptSignatureKey,
    value: &PrivateKey,
    commitment_private_key: &PrivateKey,
    commitment: &Commitment,
    message: [u8; 32],
) -> Result<ComAndPubSignature, LedgerDeviceError> {
    debug!(target: LOG_TARGET, "ledger_get_script_signature: account '{}', message '{}'", account, message.to_hex());
    verify_ledger_application()?;

    let mut data = Vec::new();
    let network = u64::from(network.as_byte()).to_le_bytes();
    data.extend_from_slice(&network);
    let version = u64::from(version).to_le_bytes();
    data.extend_from_slice(&version);

    let value = value.to_vec();
    data.extend_from_slice(&value);
    let commitment_private_key = commitment_private_key.to_vec();
    data.extend_from_slice(&commitment_private_key);
    let commitment = commitment.to_vec();
    data.extend_from_slice(&commitment);
    data.extend_from_slice(&message);

    match signature_key {
        ScriptSignatureKey::Managed { branch, index } => {
            let branch = u64::from(branch.as_byte()).to_le_bytes();
            data.extend_from_slice(&branch);
            let index = index.to_le_bytes();
            data.extend_from_slice(&index);
        },
        ScriptSignatureKey::Derived { branch_key } => {
            data.extend_from_slice(&branch_key.to_vec());
        },
    }

    let instruction = match signature_key {
        ScriptSignatureKey::Managed { .. } => Instruction::GetScriptSignatureManaged,
        ScriptSignatureKey::Derived { .. } => Instruction::GetScriptSignatureDerived,
    };

    match Command::<Vec<u8>>::build_command(account, instruction, data).execute() {
        Ok(result) => {
            if result.data().len() < 161 {
                return Err(LedgerDeviceError::Processing(format!(
                    "GetScriptSignature: expected 161 bytes, got {} ({:?})",
                    result.data().len(),
                    AppSW::try_from(result.retcode())?
                )));
            }
            let data = result.data();
            let signature = ComAndPubSignature::new(
                Commitment::from_canonical_bytes(&data[1..33])?,
                PublicKey::from_canonical_bytes(&data[33..65])?,
                PrivateKey::from_canonical_bytes(&data[65..97])?,
                PrivateKey::from_canonical_bytes(&data[97..129])?,
                PrivateKey::from_canonical_bytes(&data[129..161])?,
            );
            Ok(signature)
        },
        Err(e) => Err(LedgerDeviceError::Processing(format!("GetScriptSignature: {}", e))),
    }
}

/// Get the script offset from the ledger device
pub fn ledger_get_script_offset(
    account: u64,
    partial_script_offset: &PrivateKey,
    derived_script_keys: &[PrivateKey],
    script_key_indexes: &[(TransactionKeyManagerBranch, u64)],
    derived_sender_offsets: &[PrivateKey],
    sender_offset_indexes: &[(TransactionKeyManagerBranch, u64)],
) -> Result<PrivateKey, LedgerDeviceError> {
    debug!(
        target: LOG_TARGET,
        "ledger_get_script_offset: account '{}', partial_script_offset '{}', derived_script_keys: '{:?}', \
        script_key_indexes: '{:?}', derived_sender_offsets '{:?}', sender_offset_indexes '{:?}'",
        account,
        partial_script_offset.to_hex(),
        derived_script_keys,
        script_key_indexes,
        derived_sender_offsets,
        sender_offset_indexes
    );
    verify_ledger_application()?;

    // 1. data sizes
    let mut instructions: Vec<u8> = Vec::new();
    instructions.extend_from_slice(&(sender_offset_indexes.len() as u64).to_le_bytes());
    instructions.extend_from_slice(&(script_key_indexes.len() as u64).to_le_bytes());
    instructions.extend_from_slice(&(derived_sender_offsets.len() as u64).to_le_bytes());
    instructions.extend_from_slice(&(derived_script_keys.len() as u64).to_le_bytes());
    let mut data: Vec<Vec<u8>> = vec![instructions.to_vec()];

    // 2. partial_script_offset
    data.push(partial_script_offset.to_vec());

    // 3. sender_offset_indexes
    for (branch, index) in sender_offset_indexes {
        let mut payload = u64::from(branch.as_byte()).to_le_bytes().to_vec();
        payload.extend_from_slice(&index.to_le_bytes());
        data.push(payload);
    }
    // 4. script_key_indexes
    for (branch, index) in script_key_indexes {
        let mut payload = u64::from(branch.as_byte()).to_le_bytes().to_vec();
        payload.extend_from_slice(&index.to_le_bytes());
        data.push(payload);
    }
    // 5. derived_sender_offsets
    for sender_offset in derived_sender_offsets {
        data.push(sender_offset.to_vec());
    }
    // 6. derived_script_keys
    for script_key in derived_script_keys {
        data.push(script_key.to_vec());
    }

    let commands = Command::<Vec<u8>>::chunk_command(account, Instruction::GetScriptOffset, data);

    let mut result = None;
    for command in commands {
        match command.execute() {
            Ok(r) => result = Some(r),
            Err(e) => return Err(LedgerDeviceError::Processing(format!("GetScriptOffset: {}", e))),
        }
    }

    match result {
        Some(result) => {
            if result.data().len() < 33 {
                return Err(LedgerDeviceError::Processing(format!(
                    "GetScriptOffset: expected 33 bytes, got {} ({:?})",
                    result.data().len(),
                    AppSW::try_from(result.retcode())?
                )));
            }
            let script_offset = PrivateKey::from_canonical_bytes(&result.data()[1..33])?;
            Ok(script_offset)
        },
        None => Err(LedgerDeviceError::Processing("GetScriptOffset: No result".to_string())),
    }
}

/// Get the view key from the ledger device
pub fn ledger_get_view_key(account: u64) -> Result<PrivateKey, LedgerDeviceError> {
    debug!(target: LOG_TARGET, "ledger_get_view_key: account '{}'", account);
    verify_ledger_application()?;

    match Command::<Vec<u8>>::build_command(account, Instruction::GetViewKey, vec![]).execute() {
        Ok(result) => {
            if result.data().len() < 33 {
                return Err(LedgerDeviceError::Processing(format!(
                    "GetViewKey: expected 33 bytes, got {} ({:?})",
                    result.data().len(),
                    AppSW::try_from(result.retcode())?
                )));
            }
            let view_key = PrivateKey::from_canonical_bytes(&result.data()[1..33])?;
            Ok(view_key)
        },
        Err(e) => Err(LedgerDeviceError::Processing(format!("GetViewKey: {}", e))),
    }
}

/// Get the Diffie-Hellman shared secret from the ledger device
pub fn ledger_get_dh_shared_secret(
    account: u64,
    index: u64,
    branch: TransactionKeyManagerBranch,
    public_key: &PublicKey,
) -> Result<DiffieHellmanSharedSecret<PublicKey>, LedgerDeviceError> {
    debug!(
        target: LOG_TARGET,
        "ledger_get_dh_shared_secret: account '{}', index '{}', branch '{:?}'",
        account, index, branch
    );
    verify_ledger_application()?;

    let mut data = Vec::new();
    data.extend_from_slice(&index.to_le_bytes());
    data.extend_from_slice(&u64::from(branch.as_byte()).to_le_bytes());
    data.extend_from_slice(&public_key.to_vec());

    match Command::<Vec<u8>>::build_command(account, Instruction::GetDHSharedSecret, data).execute() {
        Ok(result) => {
            if result.data().len() < 33 {
                return Err(LedgerDeviceError::Processing(format!(
                    "GetDHSharedSecret: expected 33 bytes, got {} ({:?})",
                    result.data().len(),
                    AppSW::try_from(result.retcode())?
                )));
            }
            let shared_secret = DiffieHellmanSharedSecret::<PublicKey>::from_canonical_bytes(&result.data()[1..33])?;
            Ok(shared_secret)
        },
        Err(e) => Err(LedgerDeviceError::Processing(format!("GetDHSharedSecret: {}", e))),
    }
}

///  Get the raw schnorr signature from the ledger device
pub fn ledger_get_raw_schnorr_signature(
    account: u64,
    private_key_index: u64,
    private_key_branch: TransactionKeyManagerBranch,
    nonce_index: u64,
    nonce_branch: TransactionKeyManagerBranch,
    challenge: &[u8; 64],
) -> Result<Signature, LedgerDeviceError> {
    debug!(
        target: LOG_TARGET,
        "ledger_get_raw_schnorr_signature: account '{}', pk index '{}', pk branch '{:?}', nonce index '{}', \
        nonce branch' {:?}', challenge '{}'",
        account, private_key_index, private_key_branch, nonce_index, nonce_branch, challenge.to_hex()
    );
    verify_ledger_application()?;

    let mut data = Vec::new();
    data.extend_from_slice(&private_key_index.to_le_bytes());
    data.extend_from_slice(&u64::from(private_key_branch.as_byte()).to_le_bytes());
    data.extend_from_slice(&nonce_index.to_le_bytes());
    data.extend_from_slice(&u64::from(nonce_branch.as_byte()).to_le_bytes());
    data.extend_from_slice(challenge);

    match Command::<Vec<u8>>::build_command(account, Instruction::GetRawSchnorrSignature, data).execute() {
        Ok(result) => {
            if result.data().len() < 65 {
                return Err(LedgerDeviceError::Processing(format!(
                    "GetRawSchnorrSignature: expected 65 bytes, got {} ({:?})",
                    result.data().len(),
                    AppSW::try_from(result.retcode())?
                )));
            }

            let signature = Signature::new(
                PublicKey::from_canonical_bytes(&result.data()[1..33])?,
                PrivateKey::from_canonical_bytes(&result.data()[33..65])?,
            );
            Ok(signature)
        },
        Err(e) => Err(LedgerDeviceError::Processing(format!("GetRawSchnorrSignature: {}", e))),
    }
}

/// Get the script schnorr signature from the ledger device
pub fn ledger_get_script_schnorr_signature(
    account: u64,
    private_key_index: u64,
    private_key_branch: TransactionKeyManagerBranch,
    nonce: &[u8],
) -> Result<CheckSigSchnorrSignature, LedgerDeviceError> {
    debug!(
        target: LOG_TARGET,
        "ledger_get_raw_schnorr_signature: account '{}', pk index '{}', pk branch '{:?}'",
        account, private_key_index, private_key_branch
    );
    verify_ledger_application()?;

    let mut data = Vec::new();
    data.extend_from_slice(&private_key_index.to_le_bytes());
    data.extend_from_slice(&u64::from(private_key_branch.as_byte()).to_le_bytes());
    if nonce.len() != 32 {
        return Err(LedgerDeviceError::Processing("Nonce must be 32 bytes".to_string()));
    }
    data.extend_from_slice(nonce);

    match Command::<Vec<u8>>::build_command(account, Instruction::GetScriptSchnorrSignature, data).execute() {
        Ok(result) => {
            if result.data().len() < 65 {
                return Err(LedgerDeviceError::Processing(format!(
                    "GetScriptSchnorrSignature: expected 65 bytes, got {} ({:?})",
                    result.data().len(),
                    AppSW::try_from(result.retcode())?
                )));
            }

            let signature = CheckSigSchnorrSignature::new(
                PublicKey::from_canonical_bytes(&result.data()[1..33])?,
                PrivateKey::from_canonical_bytes(&result.data()[33..65])?,
            );
            Ok(signature)
        },
        Err(e) => Err(LedgerDeviceError::Processing(format!(
            "GetScriptSchnorrSignature: {}",
            e
        ))),
    }
}

/// Get the one sided metadata signature
pub fn ledger_get_one_sided_metadata_signature(
    account: u64,
    network: Network,
    txo_version: u8,
    value: u64,
    sender_offset_key_index: u64,
    commitment_mask: &PrivateKey,
    receiver_public_spend_key: &PublicKey,
    message: &[u8; 32],
) -> Result<ComAndPubSignature, LedgerDeviceError> {
    debug!(
        target: LOG_TARGET,
        "ledger_get_one_sided_metadata_signature: account '{}', message '{}'",
        account, message.to_hex()
    );
    verify_ledger_application()?;

    let mut data = Vec::new();
    data.extend_from_slice(&u64::from(network.as_byte()).to_le_bytes());
    data.extend_from_slice(&u64::from(txo_version).to_le_bytes());
    data.extend_from_slice(&sender_offset_key_index.to_le_bytes());
    data.extend_from_slice(&value.to_le_bytes());
    data.extend_from_slice(&commitment_mask.to_vec());
    data.extend_from_slice(&receiver_public_spend_key.to_vec());
    data.extend_from_slice(&message.to_vec());

    match Command::<Vec<u8>>::build_command(account, Instruction::GetOneSidedMetadataSignature, data).execute() {
        Ok(result) => {
            if result.retcode() == AppSW::UserCancelled as u16 {
                return Err(LedgerDeviceError::UserCancelled);
            }
            if result.data().len() < 161 {
                return Err(LedgerDeviceError::Processing(format!(
                    "'get_one_sided_metadata_signature' insufficient data - expected 161 got {} bytes ({:?})",
                    result.data().len(),
                    result
                )));
            }
            let data = result.data();
            Ok(ComAndPubSignature::new(
                Commitment::from_canonical_bytes(&data[1..33])
                    .map_err(|e| LedgerDeviceError::ByteArrayError(e.to_string()))?,
                PublicKey::from_canonical_bytes(&data[33..65])
                    .map_err(|e| LedgerDeviceError::ByteArrayError(e.to_string()))?,
                PrivateKey::from_canonical_bytes(&data[65..97])
                    .map_err(|e| LedgerDeviceError::ByteArrayError(e.to_string()))?,
                PrivateKey::from_canonical_bytes(&data[97..129])
                    .map_err(|e| LedgerDeviceError::ByteArrayError(e.to_string()))?,
                PrivateKey::from_canonical_bytes(&data[129..161])
                    .map_err(|e| LedgerDeviceError::ByteArrayError(e.to_string()))?,
            ))
        },
        Err(e) => Err(LedgerDeviceError::Instruction(format!(
            "GetOneSidedMetadataSignature: {}",
            e
        ))),
    }
}
