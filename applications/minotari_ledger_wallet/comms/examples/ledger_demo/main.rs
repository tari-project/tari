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
use dialoguer::{Select, theme::ColorfulTheme};
use minotari_ledger_wallet_common::common_types::{AppSW, Instruction, LedgerKeyBranch};
use minotari_ledger_wallet_comms::{
    accessor_methods::{
        ScriptSignatureKey,
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
    },
    error::LedgerDeviceError,
    ledger_wallet::{Command, get_transport},
};
use rand::Rng;
use tari_common::configuration::Network;
use tari_common_types::{
    tari_address::TariAddress,
    types::{CompressedCommitment, CompressedPublicKey, PrivateKey},
};
use tari_crypto::{keys::SecretKey, ristretto::RistrettoSecretKey};
use tari_utilities::{ByteArray, hex::Hex};

#[allow(clippy::too_many_lines)]
fn main() {
    println!();

    // Repeated access to the transport is efficient
    for _i in 0..10 {
        let instant = std::time::Instant::now();
        match get_transport() {
            Ok(_) => {},
            Err(e) => {
                println!("\nError: {e}\n");
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
                println!("\nError: {e}\n");
                return;
            },
        }
        println!("Application verified in {:?}", instant.elapsed());
    }

    println!();

    // GetAppName
    println!("\ntest: GetAppName");
    match ledger_get_app_name() {
        Ok(name) => println!("app name:       {name}"),
        Err(e) => {
            println!("\nError: {e}\n");
            return;
        },
    }

    // GetVersion
    println!("\ntest: GetVersion");
    match ledger_get_version() {
        Ok(name) => println!("version:        {name}"),
        Err(e) => {
            println!("\nError: {e}\n");
            return;
        },
    }

    // GetPublicAlpha
    println!("\ntest: GetPublicAlpha");
    let account = rand::rng().next_u64();
    match ledger_get_public_spend_key(account) {
        Ok(public_alpha) => println!("public_alpha:   {}", public_alpha.to_hex()),
        Err(e) => {
            println!("\nError: {e}\n");
            return;
        },
    }

    // GetPublicKey
    println!("\ntest: GetPublicKey");
    let index = rand::rng().next_u64();

    // Note: the spend branch is deliberately not addressable from the host.
    for branch in [
        LedgerKeyBranch::OneSidedSenderOffset,
        LedgerKeyBranch::Random,
        LedgerKeyBranch::PreMine,
    ] {
        match ledger_get_public_key(account, index, branch) {
            Ok(public_key) => println!("public_key:     {}", public_key.to_hex()),
            Err(e) => {
                println!("\nError: {e}\n");
                return;
            },
        }
    }

    // GetScriptSignature
    println!("\ntest: GetScriptSignature");
    let network = Network::LocalNet;
    let version = 0u8;
    let value = PrivateKey::from(123456);
    let spend_private_key = get_random_nonce();
    let commitment =
        CompressedCommitment::from_compressed_key(CompressedPublicKey::from_secret_key(&get_random_nonce()));
    let mut script_message = [0u8; 32];
    script_message.copy_from_slice(&get_random_nonce().to_vec());

    for branch_key in [
        ScriptSignatureKey::Derived {
            branch_key: get_random_nonce(),
        },
        ScriptSignatureKey::Managed {
            branch: LedgerKeyBranch::PreMine,
            index: rand::rng().next_u64(),
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
                println!("\nError: {e}\n");
                return;
            },
        }
    }

    // GetScriptOffset
    println!("\ntest: GetScriptOffset");
    let partial_script_offset = PrivateKey::default();
    let mut derived_script_keys = Vec::new();
    let mut script_key_indexes = Vec::new();
    for _i in 0..5 {
        derived_script_keys.push(get_random_nonce());
        script_key_indexes.push((LedgerKeyBranch::PreMine, rand::rng().next_u64()));
    }

    let sender_offset_count = 3;
    let (script_offset, sender_offset_indexes) = match ledger_get_script_offset(
        account,
        &partial_script_offset,
        &derived_script_keys,
        &script_key_indexes,
        sender_offset_count,
    ) {
        Ok(val) => val,
        Err(e) => {
            println!("\nError: {e}\n");
            return;
        },
    };
    println!("script_offset:  {}", script_offset.to_hex());
    println!("offset indexes: {sender_offset_indexes:?}");
    if sender_offset_indexes.len() != sender_offset_count {
        println!("\nError: expected {sender_offset_count} sender offset indexes\n");
        return;
    }

    // The device must pick a fresh index every call, otherwise two calls could be differenced to strip the blinding.
    match ledger_get_script_offset(
        account,
        &partial_script_offset,
        &derived_script_keys,
        &script_key_indexes,
        sender_offset_count,
    ) {
        Ok((_, repeat_indexes)) => {
            if repeat_indexes == sender_offset_indexes {
                println!("\nError: the device reused its sender offset key indexes\n");
                return;
            }
        },
        Err(e) => {
            println!("\nError: {e}\n");
            return;
        },
    }

    // An unblinded script offset must be refused: it is the plain sum of the input script private keys, and since
    // the host chose the blinding factors those keys were derived from, it could strip them off and be left with
    // the wallet's spend key.
    //
    // These go over raw APDUs on purpose. `ledger_get_script_offset` refuses a zero count before it opens the
    // transport, so calling through it would never exercise the device's own rejection - the check that matters.
    println!("\ntest: GetScriptOffset (no sender offset keys, must fail on the device)");
    let mut zero_count_header = Vec::new();
    zero_count_header.extend_from_slice(&0u64.to_le_bytes()); // sender_offset_count
    zero_count_header.extend_from_slice(&0u64.to_le_bytes()); // script_index_count
    zero_count_header.extend_from_slice(&1u64.to_le_bytes()); // derived_script_key_count
    let header_response = match Command::<Vec<u8>>::build_chunk_command(
        account,
        Instruction::GetScriptOffset,
        0,
        true,
        zero_count_header,
    )
    .execute()
    {
        Ok(response) => response,
        Err(e) => {
            println!("\nError: {e}\n");
            return;
        },
    };
    if header_response.retcode() == AppSW::Ok as u16 {
        println!("\nError: the device accepted a script offset header with no sender offset keys\n");
        return;
    }
    println!("rejected as expected: {:?}", AppSW::try_from(header_response.retcode()));

    // The rejected header must leave nothing behind. Otherwise a host could follow it with a chunk that folds an
    // alpha derived script key into the sum and then, because the (rejected) count was zero, read the sum back
    // unblinded - full spend key recovery in two calls.
    println!("\ntest: GetScriptOffset (resume after a rejected header, must fail on the device)");
    let resume_response = match Command::<Vec<u8>>::build_chunk_command(
        account,
        Instruction::GetScriptOffset,
        2,
        false,
        get_random_nonce().to_vec(),
    )
    .execute()
    {
        Ok(response) => response,
        Err(e) => {
            println!("\nError: {e}\n");
            return;
        },
    };
    if resume_response.retcode() == AppSW::Ok as u16 {
        println!(
            "\nError: the device resumed a rejected script offset and returned {} bytes\n",
            resume_response.data().len()
        );
        return;
    }
    println!("rejected as expected: {:?}", AppSW::try_from(resume_response.retcode()));

    // The mirror case. With no script key the device derived itself, the reply is `-k_sender` for a key the device
    // just generated: enough for the host to recompute the one sided Diffie-Hellman secrets for that output and
    // re-sign its metadata signature without the device. `partial_script_key_sum` does not count, because a host
    // with no script keys sends the zero scalar and the device cannot tell that apart from a real sum of zero.
    //
    // This also goes over raw APDUs: `ledger_get_script_offset` refuses it before it opens the transport, so
    // calling through it would never exercise the device's own rejection.
    println!("\ntest: GetScriptOffset (no device derived script keys, must fail on the device)");
    let mut no_script_key_header = Vec::new();
    no_script_key_header.extend_from_slice(&1u64.to_le_bytes()); // sender_offset_count
    no_script_key_header.extend_from_slice(&0u64.to_le_bytes()); // script_index_count
    no_script_key_header.extend_from_slice(&0u64.to_le_bytes()); // derived_script_key_count
    let no_script_key_response = match Command::<Vec<u8>>::build_chunk_command(
        account,
        Instruction::GetScriptOffset,
        0,
        true,
        no_script_key_header,
    )
    .execute()
    {
        Ok(response) => response,
        Err(e) => {
            println!("\nError: {e}\n");
            return;
        },
    };
    if no_script_key_response.retcode() == AppSW::Ok as u16 {
        println!("\nError: the device accepted a script offset header with no device derived script keys\n");
        return;
    }
    println!(
        "rejected as expected: {:?}",
        AppSW::try_from(no_script_key_response.retcode())
    );

    // ...and that rejection must leave nothing behind either, for the same reason: a follow-up chunk numbered into
    // a later section must not be able to resume the accumulation and read the withheld value back.
    println!("\ntest: GetScriptOffset (resume after a rejected script key header, must fail on the device)");
    let resume_response = match Command::<Vec<u8>>::build_chunk_command(
        account,
        Instruction::GetScriptOffset,
        2,
        false,
        get_random_nonce().to_vec(),
    )
    .execute()
    {
        Ok(response) => response,
        Err(e) => {
            println!("\nError: {e}\n");
            return;
        },
    };
    if resume_response.retcode() == AppSW::Ok as u16 {
        println!(
            "\nError: the device resumed a rejected script offset and returned {} bytes\n",
            resume_response.data().len()
        );
        return;
    }
    println!("rejected as expected: {:?}", AppSW::try_from(resume_response.retcode()));

    // The host side mirrors of both device rules refuse before the transport is opened, so a caller gets a legible
    // error rather than a status word.
    println!("\ntest: GetScriptOffset (host side refusals)");
    for (label, keys, indexes, count) in [
        (
            "no sender offset keys",
            derived_script_keys.clone(),
            script_key_indexes.clone(),
            0usize,
        ),
        ("no device derived script keys", Vec::new(), Vec::new(), 1usize),
    ] {
        match ledger_get_script_offset(account, &partial_script_offset, &keys, &indexes, count) {
            Ok(_) => {
                println!("\nError: the host accepted a script offset with {label}\n");
                return;
            },
            Err(e) => println!("rejected as expected ({label}): {e}"),
        }
    }

    // Only pre-mine script keys may be addressed by index; anything else lets the host name a key of its choosing.
    // This one does reach the device: the count is valid, so the host side guard passes it through.
    println!("\ntest: GetScriptOffset (non pre-mine script index, must fail on the device)");
    match ledger_get_script_offset(
        account,
        &partial_script_offset,
        &derived_script_keys,
        &[(LedgerKeyBranch::Random, rand::rng().next_u64())],
        1,
    ) {
        Ok(_) => {
            println!("\nError: a script key index outside the pre-mine branch was accepted\n");
            return;
        },
        Err(e) => println!("rejected as expected: {e}"),
    }

    // GetViewKey
    println!("\ntest: GetViewKey");

    let view_key_1 = match ledger_get_view_key(account) {
        Ok(val) => val,
        Err(e) => {
            println!("\nError: {e}\n");
            return;
        },
    };
    println!("view_key:       {}", view_key_1.to_hex());

    // GetDHSharedSecret
    println!("\ntest: GetDHSharedSecret");
    let index = rand::rng().next_u64();
    let branch = LedgerKeyBranch::OneSidedSenderOffset;
    let public_key = CompressedPublicKey::from_secret_key(&get_random_nonce());

    match ledger_get_dh_shared_secret(account, index, branch, &public_key) {
        Ok(shared_secret) => println!("shared_secret:  {}", shared_secret.as_bytes().to_vec().to_hex()),
        Err(e) => {
            println!("\nError: {e}\n");
            return;
        },
    }

    // GetRawSchnorrSignature
    println!("\ntest: GetRawSchnorrSignature");
    let private_key_index = rand::rng().next_u64();
    let private_key_branch = LedgerKeyBranch::PreMine;
    let nonce_index = rand::rng().next_u64();
    let nonce_branch = LedgerKeyBranch::Random;
    let mut challenge = [0u8; 64];
    rand::rng().fill_bytes(&mut challenge);

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
            signature.get_compressed_public_nonce().to_hex()
        ),
        Err(e) => {
            println!("\nError: {e}\n");
            return;
        },
    }

    // GetScriptSchnorrSignature
    println!("\ntest: GetScriptSchnorrSignature");
    let private_key_index = rand::rng().next_u64();
    let private_key_branch = LedgerKeyBranch::OneSidedSenderOffset;
    let mut nonce = [0u8; 32];
    rand::rng().fill_bytes(&mut nonce);

    match ledger_get_script_schnorr_signature(account, private_key_index, private_key_branch, &nonce) {
        Ok(signature) => println!(
            "signature:      ({},{})",
            signature.get_signature().to_hex(),
            signature.get_compressed_public_nonce().to_hex()
        ),
        Err(e) => {
            println!("\nError: {e}\n");
            return;
        },
    }

    // GetOneSidedMetadataSignature
    println!("\ntest: GetOneSidedMetadataSignature");
    let sender_offset_key_index = rand::rng().next_u64();
    let mut metadata_signature_message_common = [0u8; 32];
    rand::rng().fill_bytes(&mut metadata_signature_message_common);
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
            println!("\nError: {e}\n");
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
        Err(LedgerDeviceError::Processing(e)) => {
            println!("\nLedger comms responded with: '{e}'\n");
        },
        Err(e) => {
            println!("\nError: Unexpected response ({e})\n");
            return;
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
        Err(LedgerDeviceError::Processing(e)) => {
            println!("\nLedger comms responded with: '{e}'\n");
        },
        Err(e) => {
            println!("\nError: Unexpected response ({e})\n");
            return;
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
        Err(LedgerDeviceError::Processing(e)) => {
            println!("\nLedger comms responded with: '{e}'\n");
        },
        Err(e) => {
            println!("\nError: Unexpected response ({e})\n");
            return;
        },
    }

    // Test ledger app restart
    println!("\ntest: Ledger app restart");
    prompt_with_message("Start the 'MinoTari Wallet' Ledger app and press Enter to continue..");
    match ledger_get_view_key(account) {
        Ok(view_key_2) => {
            println!("view_key:       {}\n", view_key_2.to_hex());
        },
        Err(e) => {
            println!("\nError: {e}\n");
        },
    }
}

pub fn get_random_nonce() -> PrivateKey {
    let mut raw_bytes = [0u8; 64];
    rand::rng().fill_bytes(&mut raw_bytes);
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
