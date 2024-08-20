// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! # Multi-party Ledger - command line example

/// This example demonstrates how to use the Ledger Nano S/X for the Tari wallet. In order to run the example, you
/// need to have the `MinoTari Wallet` application installed on your Ledger device. For that, please follow the
/// instructions in the [README](../../wallet/README.md) file.
/// With this example, you can:
/// - Detect the hardware wallet
/// - Verify that the Ledger application is installed and the version is correct
/// - TBD
///
/// -----------------------------------------------------------------------------------------------
/// Example use:
/// `cargo run --release --example ledger_demo`
/// -----------------------------------------------------------------------------------------------
use dialoguer::{theme::ColorfulTheme, Select};
use minotari_ledger_wallet_comms::{
    accessor_methods::{
        ledger_get_app_name,
        ledger_get_dh_shared_secret,
        ledger_get_one_sided_metadata_signature,
        ledger_get_public_key,
        ledger_get_public_spend_key,
        ledger_get_raw_schnorr_signature,
        ledger_get_script_offset,
        ledger_get_script_schnorr_signature,
        ledger_get_script_signature,
        ledger_get_version,
        ledger_get_view_key,
        verify_ledger_application,
        ScriptSignatureKey,
    },
    error::LedgerDeviceError,
    ledger_wallet::get_transport,
};
use rand::{rngs::OsRng, RngCore};
use tari_common::configuration::Network;
use tari_common_types::{
    key_branches::TransactionKeyManagerBranch,
    tari_address::TariAddress,
    types::{Commitment, PrivateKey, PublicKey},
};
use tari_crypto::{
    keys::{PublicKey as PK, SecretKey},
    ristretto::RistrettoSecretKey,
};
use tari_utilities::{hex::Hex, ByteArray};

#[allow(clippy::too_many_lines)]
fn main() {
    println!();

    // Repeated access to the transport is efficient
    for _i in 0..10 {
        let instant = std::time::Instant::now();
        match get_transport() {
            Ok(_) => {},
            Err(e) => {
                println!("\nError: {}\n", e);
                return;
            },
        };
        println!("Transport created in {:?}", instant.elapsed());
    }

    println!();

    // Repeated ledger app verification is efficient
    for _i in 0..10 {
        let instant = std::time::Instant::now();
        match verify_ledger_application() {
            Ok(_) => {},
            Err(e) => {
                println!("\nError: {}\n", e);
                return;
            },
        }
        println!("Application verified in {:?}", instant.elapsed());
    }

    println!();

    // GetAppName
    println!("\ntest: GetAppName");
    match ledger_get_app_name() {
        Ok(name) => println!("app name:       {}", name),
        Err(e) => {
            println!("\nError: {}\n", e);
            return;
        },
    }

    // GetVersion
    println!("\ntest: GetVersion");
    match ledger_get_version() {
        Ok(name) => println!("version:        {}", name),
        Err(e) => {
            println!("\nError: {}\n", e);
            return;
        },
    }

    // GetPublicAlpha
    println!("\ntest: GetPublicAlpha");
    let account = OsRng.next_u64();
    match ledger_get_public_spend_key(account) {
        Ok(public_alpha) => println!("public_alpha:   {}", public_alpha.to_hex()),
        Err(e) => {
            println!("\nError: {}\n", e);
            return;
        },
    }

    // GetPublicKey
    println!("\ntest: GetPublicKey");
    let index = OsRng.next_u64();

    for branch in &[
        TransactionKeyManagerBranch::OneSidedSenderOffset,
        TransactionKeyManagerBranch::Spend,
        TransactionKeyManagerBranch::RandomKey,
        TransactionKeyManagerBranch::PreMine,
    ] {
        match ledger_get_public_key(account, index, *branch) {
            Ok(public_key) => println!("public_key:     {}", public_key.to_hex()),
            Err(e) => {
                println!("\nError: {}\n", e);
                return;
            },
        }
    }

    let branch = TransactionKeyManagerBranch::CommitmentMask;
    match ledger_get_public_key(account, index, branch) {
        Ok(_public_key) => {
            println!("\nError: Should not have returned a public key for '{:?}'\n", branch);
            return;
        },
        Err(e) => {
            if e != LedgerDeviceError::Processing("GetPublicKey: expected 33 bytes, got 0 (BadBranchKey)".to_string()) {
                println!("\nError: Unexpected response ({})\n", e);
                return;
            }
        },
    }

    // GetScriptSignature
    println!("\ntest: GetScriptSignature");
    let network = Network::LocalNet;
    let version = 0u8;
    let value = PrivateKey::from(123456);
    let spend_private_key = get_random_nonce();
    let commitment = Commitment::from_public_key(&PublicKey::from_secret_key(&get_random_nonce()));
    let mut script_message = [0u8; 32];
    script_message.copy_from_slice(&get_random_nonce().to_vec());

    for branch_key in [
        ScriptSignatureKey::Derived {
            branch_key: get_random_nonce(),
        },
        ScriptSignatureKey::Managed {
            branch: TransactionKeyManagerBranch::Spend,
            index: OsRng.next_u64(),
        },
    ] {
        match ledger_get_script_signature(
            account,
            network,
            version,
            &branch_key,
            &value,
            &spend_private_key,
            &commitment,
            script_message,
        ) {
            Ok(signature) => println!(
                "script_sig:     ({},{},{},{},{})",
                signature.ephemeral_commitment().to_hex(),
                signature.ephemeral_pubkey().to_hex(),
                signature.u_x().to_hex(),
                signature.u_a().to_hex(),
                signature.u_y().to_hex()
            ),
            Err(e) => {
                println!("\nError: {}\n", e);
                return;
            },
        }
    }

    // GetScriptOffset
    println!("\ntest: GetScriptOffset");
    let partial_script_offset = PrivateKey::default();
    let mut derived_script_keys = Vec::new();
    let mut script_key_indexes = Vec::new();
    let mut derived_sender_offsets = Vec::new();
    let mut sender_offset_indexes = Vec::new();
    for _i in 0..5 {
        derived_script_keys.push(get_random_nonce());
        script_key_indexes.push((TransactionKeyManagerBranch::Spend, OsRng.next_u64()));
        derived_sender_offsets.push(get_random_nonce());
        sender_offset_indexes.push((TransactionKeyManagerBranch::OneSidedSenderOffset, OsRng.next_u64()));
    }

    match ledger_get_script_offset(
        account,
        &partial_script_offset,
        &derived_script_keys,
        &script_key_indexes,
        &derived_sender_offsets,
        &sender_offset_indexes,
    ) {
        Ok(script_offset) => println!("script_offset:  {}", script_offset.to_hex()),
        Err(e) => {
            println!("\nError: {}\n", e);
            return;
        },
    }

    // GetViewKey
    println!("\ntest: GetViewKey");

    let view_key_1 = match ledger_get_view_key(account) {
        Ok(val) => val,
        Err(e) => {
            println!("\nError: {}\n", e);
            return;
        },
    };
    println!("view_key:       {}", view_key_1.to_hex());

    // GetDHSharedSecret
    println!("\ntest: GetDHSharedSecret");
    let index = OsRng.next_u64();
    let branch = TransactionKeyManagerBranch::OneSidedSenderOffset;
    let public_key = PublicKey::from_secret_key(&get_random_nonce());

    match ledger_get_dh_shared_secret(account, index, branch, &public_key) {
        Ok(shared_secret) => println!("shared_secret:  {}", shared_secret.as_bytes().to_vec().to_hex()),
        Err(e) => {
            println!("\nError: {}\n", e);
            return;
        },
    }

    // GetRawSchnorrSignature
    println!("\ntest: GetRawSchnorrSignature");
    let private_key_index = OsRng.next_u64();
    let private_key_branch = TransactionKeyManagerBranch::Spend;
    let nonce_index = OsRng.next_u64();
    let nonce_branch = TransactionKeyManagerBranch::RandomKey;
    let mut challenge = [0u8; 64];
    OsRng.fill_bytes(&mut challenge);

    match ledger_get_raw_schnorr_signature(
        account,
        private_key_index,
        private_key_branch,
        nonce_index,
        nonce_branch,
        &challenge,
    ) {
        Ok(signature) => println!(
            "signature:      ({},{})",
            signature.get_signature().to_hex(),
            signature.get_public_nonce().to_hex()
        ),
        Err(e) => {
            println!("\nError: {}\n", e);
            return;
        },
    }

    // GetScriptSchnorrSignature
    println!("\ntest: GetScriptSchnorrSignature");
    let private_key_index = OsRng.next_u64();
    let private_key_branch = TransactionKeyManagerBranch::Spend;
    let mut nonce = [0u8; 32];
    OsRng.fill_bytes(&mut nonce);

    match ledger_get_script_schnorr_signature(account, private_key_index, private_key_branch, &nonce) {
        Ok(signature) => println!(
            "signature:      ({},{})",
            signature.get_signature().to_hex(),
            signature.get_public_nonce().to_hex()
        ),
        Err(e) => {
            println!("\nError: {}\n", e);
            return;
        },
    }

    // GetOneSidedMetadataSignature
    println!("\ntest: GetOneSidedMetadataSignature");
    let sender_offset_key_index = OsRng.next_u64();
    let mut metadata_signature_message_common = [0u8; 32];
    OsRng.fill_bytes(&mut metadata_signature_message_common);
    let commitment_mask = get_random_nonce();
    let receiver_address = TariAddress::from_base58(
        "f48ScXDKxTU3nCQsQrXHs4tnkAyLViSUpi21t7YuBNsJE1VpqFcNSeEzQWgNeCqnpRaCA9xRZ3VuV11F8pHyciegbCt",
    )
    .unwrap();

    match ledger_get_one_sided_metadata_signature(
        account,
        network,
        version,
        12345,
        sender_offset_key_index,
        &commitment_mask,
        &receiver_address,
        &metadata_signature_message_common,
    ) {
        Ok(signature) => println!(
            "signature:      ({},{},{},{},{})",
            signature.ephemeral_commitment().to_hex(),
            signature.ephemeral_pubkey().to_hex(),
            signature.u_a().to_hex(),
            signature.u_x().to_hex(),
            signature.u_y().to_hex()
        ),
        Err(e) => {
            println!("\nError: {}\n", e);
            return;
        },
    }

    // Test ledger app not started
    println!("\ntest: Ledger app not running");
    prompt_with_message("Exit the 'MinoTari Wallet' Ledger app and press Enter to continue..");
    match ledger_get_view_key(account) {
        Ok(_) => {
            println!("\nError: Ledger app is still running\n");
            return;
        },
        Err(e) => {
            if e != LedgerDeviceError::Processing(
                "GetViewKey: Native HID transport error `Ledger device: Io error`".to_string(),
            ) {
                println!("\nError: Unexpected response ({})\n", e);
                return;
            }
        },
    }

    // Test ledger disconnect
    println!("\ntest: Ledger disconnected");
    prompt_with_message("Disconnect the Ledger device and press Enter to continue..");
    match ledger_get_view_key(account) {
        Ok(_) => {
            println!("\nError: Ledger not disconnected\n");
            return;
        },
        Err(e) => {
            if e != LedgerDeviceError::Processing(
                "GetViewKey: Native HID transport error `Ledger device: Io error`".to_string(),
            ) {
                println!("\nError: Unexpected response ({})\n", e);
                return;
            }
        },
    }

    // Test ledger reconnect
    println!("\ntest: Ledger reconnected");
    prompt_with_message("Reconnect the Ledger device (with password) and press Enter to continue..");
    match ledger_get_view_key(account) {
        Ok(_) => {
            println!("\nError: Ledger app should not be running\n");
            return;
        },
        Err(e) => {
            if e != LedgerDeviceError::Processing(
                "GetViewKey: Native HID transport error `Ledger device: Io error`".to_string(),
            ) {
                println!("\nError: Unexpected response ({})\n", e);
                return;
            }
        },
    }

    // Test ledger app restart
    println!("\ntest: Ledger app restart");
    prompt_with_message("Start the 'MinoTari Wallet' Ledger app and press Enter to continue..");
    match ledger_get_view_key(account) {
        Ok(view_key_2) => {
            println!("view_key:       {}", view_key_2.to_hex());
            assert_eq!(view_key_1, view_key_2, "View key not repeatable")
        },
        Err(e) => {
            println!("\nError: {}\n", e);
            return;
        },
    }

    println!("\nTest completed successfully\n");
}

pub fn get_random_nonce() -> PrivateKey {
    let mut raw_bytes = [0u8; 64];
    OsRng.fill_bytes(&mut raw_bytes);
    RistrettoSecretKey::from_uniform_bytes(&raw_bytes).expect("will not fail")
}

fn prompt_with_message(prompt_text: &str) -> usize {
    Select::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt_text)
        .default(0)
        .item("Ok")
        .interact()
        .unwrap()
}
