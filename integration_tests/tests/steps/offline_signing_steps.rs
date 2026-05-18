//   Copyright 2025. The Tari Project
//
//   Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//   following conditions are met:
//
//   1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//   disclaimer.
//
//   2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//   following disclaimer in the documentation and/or other materials provided with the distribution.
//
//   3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//   products derived from this software without specific prior written permission.
//
//   THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//   INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//   DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//   SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//   SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//   WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//   USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{ffi::OsString, time::Duration};

use cucumber::{then, when};
use grpc::{PaymentRecipient, payment_recipient::PaymentType};
use minotari_app_grpc::tari_rpc as grpc;
use minotari_console_wallet::{CliCommands, SignOneSidedTransactionArgs};
use minotari_offline_signer::cli::execute_from_args as execute_offline_signer_from_args;
use tari_integration_tests::{
    TariWorld,
    wallet_process::{create_wallet_client, get_default_cli, spawn_wallet},
};
use tari_transaction_components::transaction_components::memo_field::{MemoField, TxType};

use crate::steps::get_saved_seed_words;

const OFFLINE_SIGNER_TEST_PASSPHRASE: &str = "test";

fn os_args<I, T>(args: I) -> Vec<OsString>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString>,
{
    args.into_iter().map(Into::into).collect()
}

#[when(
    expr = "I prepare an offline one-sided transaction of {int} uT from wallet {word} to wallet {word} at fee {int}"
)]
async fn prepare_offline_transaction(world: &mut TariWorld, amount: u64, sender: String, receiver: String, fee: u64) {
    let mut sender_client = create_wallet_client(world, sender.clone()).await.unwrap();
    let receiver_wallet_address = world.get_wallet_address(&receiver).await.unwrap();

    let payment_id = MemoField::new_open_from_string(
        &format!("Offline one-sided {} uT from {} to {}", amount, sender, receiver),
        TxType::PaymentToOther,
    )
    .unwrap();

    let recipient = PaymentRecipient {
        address: receiver_wallet_address,
        amount,
        fee_per_gram: fee,
        payment_type: PaymentType::OneSidedToStealthAddress as i32,
        raw_payment_id: payment_id.to_bytes(),
        user_payment_id: None,
    };

    let request = grpc::PrepareOneSidedTransactionForSigningRequest {
        recipient: Some(recipient),
    };

    let response = sender_client
        .prepare_one_sided_transaction_for_signing(request)
        .await
        .unwrap()
        .into_inner();

    assert!(
        response.is_success,
        "PrepareOneSidedTransactionForSigning failed: {}",
        response.failure_message
    );
    assert!(!response.result.is_empty(), "Prepared transaction result is empty");

    println!("Prepared offline transaction: {} bytes of JSON", response.result.len());
    world.offline_signing_prepared = Some(response.result);
}

#[then(expr = "I sign the prepared transaction using wallet {word}")]
async fn sign_prepared_transaction_using_wallet(world: &mut TariWorld, wallet_name: String) {
    let prepared_json = world
        .offline_signing_prepared
        .as_ref()
        .expect("No prepared transaction found — run the prepare step first")
        .clone();

    // Materialise the prepared transaction as a file under the signing
    // wallet's temp dir. The console wallet CLI's `SignOneSidedTransaction`
    // subcommand reads the request from `input_file` and writes the signed
    // result to `output_file`.
    let (input_file, output_file, base_node_name, peer_seeds) = {
        let wallet_ps = world
            .wallets
            .get_mut(&wallet_name)
            .unwrap_or_else(|| panic!("Wallet '{wallet_name}' not found"));
        let input = wallet_ps.temp_dir_path.join("offline_signing_input.json");
        let output = wallet_ps.temp_dir_path.join("offline_signing_output.json");
        // Ensure any stale output from a previous run doesn't cause a false-
        // positive read below. `drop` rather than `let _ =` so clippy's
        // `let-underscore-drop` lint is satisfied; we genuinely want to
        // discard the Result here because "file not present" is expected on
        // the first run.
        drop(std::fs::remove_file(&output));
        std::fs::write(&input, &prepared_json).expect("Failed to write prepared transaction file");

        wallet_ps.kill();
        (
            input,
            output,
            wallet_ps.base_node_name.clone(),
            wallet_ps.peer_seeds.clone(),
        )
    };

    // Give the killed wallet a moment to fully release its db/port lock.
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Respawn the spend wallet with `SignOneSidedTransaction` as the boot-time
    // command so the CLI signs the prepared transaction using the wallet's
    // own in-database spend key — exercising the full offline signing cycle
    // through the real wallet binary rather than reconstructing a key manager
    // from file-exported keys in-process.
    let mut cli = get_default_cli();
    cli.command2 = Some(CliCommands::SignOneSidedTransaction(SignOneSidedTransactionArgs {
        input_file: input_file.clone(),
        output_file: output_file.clone(),
    }));

    spawn_wallet(world, wallet_name.clone(), base_node_name, peer_seeds, None, Some(cli)).await;

    // Poll for the signed output file to appear. The CLI writes it once the
    // command has finished, which happens after wallet startup + unlock.
    let poll_interval = Duration::from_millis(500);
    let timeout = Duration::from_secs(60);
    let mut waited = Duration::ZERO;
    while waited < timeout && !output_file.exists() {
        tokio::time::sleep(poll_interval).await;
        waited += poll_interval;
    }
    assert!(
        output_file.exists(),
        "Signed transaction file never appeared at {output_file:?} within {timeout:?}"
    );

    let signed_json =
        std::fs::read_to_string(&output_file).unwrap_or_else(|e| panic!("Failed to read signed transaction file: {e}"));
    assert!(!signed_json.is_empty(), "Signed transaction file is empty");

    println!("Transaction signed via wallet CLI: {} bytes of JSON", signed_json.len());
    world.offline_signing_signed = Some(signed_json);
}

#[then(expr = "I initialize standalone offline signer {word} from wallet {word} seed words")]
async fn initialize_standalone_offline_signer(world: &mut TariWorld, signer_name: String, wallet_name: String) {
    let seed_words = get_saved_seed_words(world, &wallet_name).join(" ");
    let signer_dir = world
        .current_base_dir
        .as_ref()
        .expect("Base dir on world")
        .join("offline_signers")
        .join(&signer_name);
    std::fs::create_dir_all(&signer_dir)
        .unwrap_or_else(|e| panic!("Failed to create offline signer dir {signer_dir:?}: {e}"));
    let keystore_file = signer_dir.join("keystore.json");
    drop(std::fs::remove_file(&keystore_file));

    execute_offline_signer_from_args(os_args([
        OsString::from("minotari_offline_signer"),
        OsString::from("--test-keystore-file"),
        keystore_file.as_os_str().to_os_string(),
        OsString::from("init"),
        OsString::from("seed-words"),
        OsString::from("--seed-words"),
        OsString::from(seed_words),
        OsString::from("--passphrase"),
        OsString::from(OFFLINE_SIGNER_TEST_PASSPHRASE),
    ]))
    .unwrap_or_else(|e| panic!("Failed to initialize standalone offline signer: {e}"));

    world.offline_signer_keystores.insert(signer_name, keystore_file);
}

#[then(expr = "I sign the prepared transaction using standalone offline signer {word}")]
async fn sign_prepared_transaction_using_standalone_offline_signer(world: &mut TariWorld, signer_name: String) {
    let prepared_json = world
        .offline_signing_prepared
        .as_ref()
        .expect("No prepared transaction found — run the prepare step first")
        .clone();
    let keystore_file = world
        .offline_signer_keystores
        .get(&signer_name)
        .unwrap_or_else(|| panic!("Standalone offline signer '{signer_name}' not initialized"));
    let signer_dir = keystore_file
        .parent()
        .unwrap_or_else(|| panic!("Keystore file {keystore_file:?} has no parent directory"));
    let input_file = signer_dir.join("offline_signing_input.json");
    let output_file = signer_dir.join("offline_signing_output.json");
    drop(std::fs::remove_file(&output_file));
    std::fs::write(&input_file, prepared_json).expect("Failed to write prepared transaction file");

    execute_offline_signer_from_args(os_args([
        OsString::from("minotari_offline_signer"),
        OsString::from("--test-keystore-file"),
        keystore_file.as_os_str().to_os_string(),
        OsString::from("sign"),
        OsString::from("--input-file"),
        input_file.as_os_str().to_os_string(),
        OsString::from("--output-file"),
        output_file.as_os_str().to_os_string(),
        OsString::from("--passphrase"),
        OsString::from(OFFLINE_SIGNER_TEST_PASSPHRASE),
        OsString::from("--network"),
        OsString::from("localnet"),
    ]))
    .unwrap_or_else(|e| panic!("Failed to sign transaction with standalone offline signer: {e}"));

    let signed_json =
        std::fs::read_to_string(&output_file).unwrap_or_else(|e| panic!("Failed to read signed transaction file: {e}"));
    assert!(!signed_json.is_empty(), "Signed transaction file is empty");

    println!(
        "Transaction signed via standalone offline signer: {} bytes of JSON",
        signed_json.len()
    );
    world.offline_signing_signed = Some(signed_json);
}

/// Tamper the prepared offline signing payload by incrementing the `tx_id` field.
///
/// Any modification to the signed content (every field except `payload_signature`
/// itself, which is stripped before hashing) invalidates the integrity signature
/// that the view wallet embedded when it called `prepare_one_sided_transaction_for_signing`.
/// This step simulates the MITM attack described in issue #7796.
#[when(expr = "I tamper with the prepared offline signing payload")]
async fn tamper_offline_signing_payload(world: &mut TariWorld) {
    let prepared_json = world
        .offline_signing_prepared
        .as_ref()
        .expect("No prepared transaction found — run the prepare step first")
        .clone();

    let mut value: serde_json::Value =
        serde_json::from_str(&prepared_json).expect("Failed to parse prepared transaction JSON");

    // Increment tx_id by 1.  This produces a structurally valid JSON document
    // that deserialises without error, but whose canonical bytes differ from those
    // that were signed — so the Schnorr integrity check inside sign_locked_transaction
    // will fail and signing will be aborted.
    if let Some(tx_id) = value.get_mut("tx_id") {
        let original = tx_id.as_u64().unwrap_or(0);
        *tx_id = serde_json::Value::Number((original.wrapping_add(1)).into());
    }

    world.offline_signing_prepared =
        Some(serde_json::to_string(&value).expect("Failed to re-serialise tampered payload"));
    println!("Tampered prepared offline signing payload (incremented tx_id by 1)");
}

/// Attempt to sign the tampered payload using the standalone offline signer and
/// assert that the operation fails with a payload integrity error.
///
/// The signer must NOT produce an output file: writing signed material to disk
/// for a tampered payload would constitute a security bypass.
#[then(expr = "signing the tampered payload using standalone offline signer {word} fails with an integrity error")]
async fn sign_tampered_payload_fails_integrity_check(world: &mut TariWorld, signer_name: String) {
    let prepared_json = world
        .offline_signing_prepared
        .as_ref()
        .expect("No prepared transaction found")
        .clone();
    let keystore_file = world
        .offline_signer_keystores
        .get(&signer_name)
        .unwrap_or_else(|| panic!("Standalone offline signer '{signer_name}' not initialized"))
        .clone();
    let signer_dir = keystore_file
        .parent()
        .unwrap_or_else(|| panic!("Keystore file {keystore_file:?} has no parent directory"))
        .to_path_buf();
    let input_file = signer_dir.join("offline_signing_tampered_input.json");
    let output_file = signer_dir.join("offline_signing_tampered_output.json");
    drop(std::fs::remove_file(&output_file));
    std::fs::write(&input_file, prepared_json).expect("Failed to write tampered transaction file");

    let result = execute_offline_signer_from_args(os_args([
        OsString::from("minotari_offline_signer"),
        OsString::from("--test-keystore-file"),
        keystore_file.as_os_str().to_os_string(),
        OsString::from("sign"),
        OsString::from("--input-file"),
        input_file.as_os_str().to_os_string(),
        OsString::from("--output-file"),
        output_file.as_os_str().to_os_string(),
        OsString::from("--passphrase"),
        OsString::from(OFFLINE_SIGNER_TEST_PASSPHRASE),
        OsString::from("--network"),
        OsString::from("localnet"),
    ]));

    assert!(
        result.is_err(),
        "Expected the standalone offline signer to reject the tampered payload, but it succeeded"
    );
    assert!(
        !output_file.exists(),
        "Offline signer wrote a signed output for a tampered payload — integrity check was bypassed"
    );
    println!(
        "Standalone offline signer correctly rejected the tampered payload: {:?}",
        result.unwrap_err()
    );
}

#[when(expr = "I broadcast the signed transaction via wallet {word}")]
async fn broadcast_signed_transaction(world: &mut TariWorld, wallet_name: String) {
    let signed_json = world
        .offline_signing_signed
        .as_ref()
        .expect("No signed transaction found — run the sign step first")
        .clone();

    let mut client = create_wallet_client(world, wallet_name.clone()).await.unwrap();

    let request = grpc::BroadcastSignedOneSidedTransactionRequest { request: signed_json };

    let response = client
        .broadcast_signed_one_sided_transaction(request)
        .await
        .unwrap()
        .into_inner();

    assert!(
        response.is_success,
        "BroadcastSignedOneSidedTransaction failed: {}",
        response.failure_message
    );

    println!(
        "Signed transaction broadcast successfully, tx_id: {}",
        response.transaction_id
    );
}
