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

use std::time::Duration;

use cucumber::{then, when};
use grpc::{PaymentRecipient, payment_recipient::PaymentType};
use minotari_app_grpc::tari_rpc as grpc;
use minotari_console_wallet::{CliCommands, SignOneSidedTransactionArgs};
use tari_integration_tests::{
    TariWorld,
    wallet_process::{create_wallet_client, get_default_cli, spawn_wallet},
};
use tari_transaction_components::transaction_components::memo_field::{MemoField, TxType};

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
