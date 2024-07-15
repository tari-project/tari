// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! # Multi-party Ledger - command line example

use minotari_ledger_wallet_comms::{
    accessor_methods::{
        ledger_get_app_name,
        ledger_get_dh_shared_secret,
        ledger_get_public_alpha,
        ledger_get_public_key,
        ledger_get_raw_schnorr_signature,
        ledger_get_script_offset,
        ledger_get_script_schnorr_signature,
        ledger_get_script_signature,
        ledger_get_version,
        ledger_get_view_key,
        verify_ledger_application,
    },
    ledger_wallet::get_transport,
};
use rand::rngs::OsRng;
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
use rand::RngCore;
use tari_common::configuration::Network;
use tari_common_types::{
    key_manager::TransactionKeyManagerBranch,
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
    match ledger_get_public_alpha(account) {
        Ok(public_alpha) => println!("public_alpha:   {}", public_alpha.to_hex()),
        Err(e) => {
            println!("\nError: {}\n", e);
            return;
        },
    }

    // GetPublicKey
    println!("\ntest: GetPublicKey");
    let index = OsRng.next_u64();
    let branch = TransactionKeyManagerBranch::RandomKey;

    match ledger_get_public_key(account, index, branch) {
        Ok(public_key) => println!("public_key:     {}", public_key.to_hex()),
        Err(e) => {
            println!("\nError: {}\n", e);
            return;
        },
    }

    // GetScriptSignature
    println!("\ntest: GetScriptSignature");
    let network = Network::LocalNet;
    let version = 0u8;
    let branch_key = get_random_nonce();
    let value = PrivateKey::from(123456);
    let spend_private_key = get_random_nonce();
    let commitment = Commitment::from_public_key(&PublicKey::from_secret_key(&get_random_nonce()));
    let mut script_message = [0u8; 32];
    script_message.copy_from_slice(&get_random_nonce().to_vec());

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

    // GetScriptOffset
    println!("\ntest: GetScriptOffset");
    let mut derived_key_commitments = Vec::new();
    let mut sender_offset_indexes = Vec::new();
    for _i in 0..5 {
        derived_key_commitments.push(get_random_nonce());
        sender_offset_indexes.push(OsRng.next_u64());
    }

    match ledger_get_script_offset(account, &derived_key_commitments, &sender_offset_indexes) {
        Ok(script_offset) => println!("script_offset:  {}", script_offset.to_hex()),
        Err(e) => {
            println!("\nError: {}\n", e);
            return;
        },
    }

    // GetViewKey
    println!("\ntest: GetViewKey");

    match ledger_get_view_key(account) {
        Ok(view_key) => println!("view_key:       {}", view_key.to_hex()),
        Err(e) => {
            println!("\nError: {}\n", e);
            return;
        },
    }

    // GetDHSharedSecret
    println!("\ntest: GetDHSharedSecret");
    let index = OsRng.next_u64();
    let branch = TransactionKeyManagerBranch::SenderOffsetLedger;
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

    println!("\nTest completed successfully\n");
}

pub fn get_random_nonce() -> PrivateKey {
    let mut raw_bytes = [0u8; 64];
    OsRng.fill_bytes(&mut raw_bytes);
    RistrettoSecretKey::from_uniform_bytes(&raw_bytes).expect("will not fail")
}
