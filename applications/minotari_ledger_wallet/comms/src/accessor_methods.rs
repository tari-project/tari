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
use tari_utilities::ByteArray;

use crate::{
    error::LedgerDeviceError,
    ledger_wallet::{Command, EXPECTED_NAME, EXPECTED_VERSION},
};

// hash_domain!(CheckSigHashDomain, "com.tari.script.check_sig", 1);
// type CheckSigSchnorrSignature = SchnorrSignature<RistrettoPublicKey, RistrettoSecretKey, CheckSigHashDomain>;

/// Verify that the ledger application is working properly.
pub fn verify_ledger_application() -> Result<(), LedgerDeviceError> {
    static VERIFIED: Lazy<Mutex<Option<Result<(), LedgerDeviceError>>>> = Lazy::new(|| Mutex::new(None));
    if let Ok(mut verified) = VERIFIED.try_lock() {
        if verified.is_none() {
            match verify() {
                Ok(_) => *verified = Some(Ok(())),
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
    let private_key_branch = TransactionKeyManagerBranch::SenderOffsetLedger;
    let mut nonce = [0u8; 32];
    OsRng.fill_bytes(&mut nonce);
    let signature_a = match ledger_get_script_schnorr_signature(account, private_key_index, private_key_branch, &nonce)
    {
        Ok(signature) => match ledger_get_public_key(account, private_key_index, private_key_branch) {
            Ok(public_key) => {
                if !signature.verify(&public_key, nonce) {
                    return Err(LedgerDeviceError::Processing(
                        "'Minotari Wallet' application could not create a valid signature".to_string(),
                    ));
                }
                signature
            },
            Err(e) => {
                return Err(LedgerDeviceError::Processing(format!(
                    "'Minotari Wallet' application could not retrieve a public key ({:?})",
                    e
                )))
            },
        },
        Err(e) => {
            return Err(LedgerDeviceError::Processing(format!(
                "'Minotari Wallet' application could not create a signature ({:?})",
                e
            )))
        },
    };
    match ledger_get_script_schnorr_signature(account, private_key_index, private_key_branch, &nonce) {
        Ok(signature_b) => {
            if signature_a == signature_b {
                return Err(LedgerDeviceError::Processing(
                    "'Minotari Wallet' application is not creating unique signatures".to_string(),
                ));
            }
        },
        Err(e) => {
            return Err(LedgerDeviceError::Processing(format!(
                "'Minotari Wallet' application could not create a signature ({:?})",
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
    verify_ledger_application()?;

    match Command::<Vec<u8>>::build_command(account, Instruction::GetPublicSpendKey, vec![]).execute() {
        Ok(result) => {
            if result.data().len() < 33 {
                return Err(LedgerDeviceError::Processing(format!(
                    "GetPublicAlpha: expected 33 bytes, got {} ({:?})",
                    result.data().len(),
                    AppSW::try_from(result.retcode())?
                )));
            }
            let public_alpha = PublicKey::from_canonical_bytes(&result.data()[1..33])?;
            Ok(public_alpha)
        },
        Err(e) => Err(LedgerDeviceError::Processing(format!("GetPublicAlpha: {}", e))),
    }
}

/// Get a public key from the ledger device
pub fn ledger_get_public_key(
    account: u64,
    index: u64,
    branch: TransactionKeyManagerBranch,
) -> Result<PublicKey, LedgerDeviceError> {
    verify_ledger_application()?;

    let mut data = Vec::new();
    data.extend_from_slice(&index.to_le_bytes());
    let branch_u64 = u64::from(branch.as_byte()).to_le_bytes();
    data.extend_from_slice(&branch_u64);

    match Command::<Vec<u8>>::build_command(account, Instruction::GetPublicKey, data).execute() {
        Ok(result) => {
            if result.data().len() < 33 {
                return Err(LedgerDeviceError::Processing(format!(
                    "GetPublicAlpha: expected 33 bytes, got {} ({:?})",
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
    branch_key: &PrivateKey,
    value: &PrivateKey,
    commitment_private_key: &PrivateKey,
    commitment: &Commitment,
    script_message: [u8; 32],
) -> Result<ComAndPubSignature, LedgerDeviceError> {
    verify_ledger_application()?;

    let mut data = Vec::new();
    let network = u64::from(network.as_byte()).to_le_bytes();
    data.extend_from_slice(&network);
    let version = u64::from(version).to_le_bytes();
    data.extend_from_slice(&version);
    let branch_key = branch_key.to_vec();
    data.extend_from_slice(&branch_key);
    let value = value.to_vec();
    data.extend_from_slice(&value);
    let commitment_private_key = commitment_private_key.to_vec();
    data.extend_from_slice(&commitment_private_key);
    let commitment = commitment.to_vec();
    data.extend_from_slice(&commitment);
    data.extend_from_slice(&script_message);

    match Command::<Vec<u8>>::build_command(account, Instruction::GetScriptSignature, data).execute() {
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
    derived_key_commitments: &[PrivateKey],
    sender_offset_indexes: &[u64],
) -> Result<PrivateKey, LedgerDeviceError> {
    verify_ledger_application()?;

    let num_commitments = derived_key_commitments.len() as u64;
    let num_offset_key = sender_offset_indexes.len() as u64;

    let mut instructions = num_offset_key.to_le_bytes().to_vec();
    instructions.extend_from_slice(&num_commitments.to_le_bytes());

    let mut data: Vec<Vec<u8>> = vec![instructions.to_vec()];
    let total_script_private_key = PrivateKey::default();
    data.push(total_script_private_key.to_vec());

    for sender_offset_index in sender_offset_indexes {
        data.push(sender_offset_index.to_le_bytes().to_vec());
    }

    for derived_key_commitment in derived_key_commitments {
        data.push(derived_key_commitment.to_vec());
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
