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

use cucumber::{then, when};
use minotari_app_grpc::tari_rpc as grpc;
use grpc::PaymentRecipient;
use tari_common::configuration::Network;
use tari_common_types::types::PrivateKey;
use tari_integration_tests::{TariWorld, wallet_process::create_wallet_client};
use tari_transaction_components::{
    consensus::ConsensusManager,
    key_manager::{
        KeyManager,
        wallet_types::{SpendWallet, WalletType},
    },
    offline_signing::{
        models::{PrepareOneSidedTransactionForSigningResult, SignedOneSidedTransactionResult, TransactionResult},
        sign_locked_transaction,
    },
    transaction_components::memo_field::{MemoField, TxType},
};
use tari_utilities::hex::Hex;

#[when(expr = "I prepare an offline one-sided transaction of {int} uT from wallet {word} to wallet {word} at fee {int}")]
async fn prepare_offline_transaction(
    world: &mut TariWorld,
    amount: u64,
    sender: String,
    receiver: String,
    fee: u64,
) {
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
        payment_type: 2, // ONE_SIDED_TO_STEALTH_ADDRESS
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

#[then(expr = "I sign the prepared transaction offline using keys {word}")]
async fn sign_prepared_transaction_offline(world: &mut TariWorld, keys_name: String) {
    let prepared_json = world
        .offline_signing_prepared
        .as_ref()
        .expect("No prepared transaction found — run the prepare step first")
        .clone();

    // Read the exported keys file
    let keys_file = world
        .view_and_spend_keys
        .get(&keys_name)
        .unwrap_or_else(|| panic!("Keys '{}' not found — export them first", keys_name));

    let keys_content = std::fs::read_to_string(keys_file)
        .unwrap_or_else(|e| panic!("Failed to read keys file: {e}"));
    let keys_json: serde_json::Value =
        serde_json::from_str(&keys_content).unwrap_or_else(|e| panic!("Failed to parse keys JSON: {e}"));

    let spend_key_hex = keys_json["spend_key"]
        .as_str()
        .expect("Missing spend_key in keys file");
    let view_key_hex = keys_json["view_key"]
        .as_str()
        .expect("Missing view_key in keys file");

    let spend_key =
        PrivateKey::from_hex(spend_key_hex).expect("Invalid spend_key hex");
    let view_key =
        PrivateKey::from_hex(view_key_hex).expect("Invalid view_key hex");

    // Create an offline key manager with the full spend wallet
    let spend_wallet = SpendWallet::new(spend_key, view_key, None);
    let wallet_type = WalletType::SpendWallet(spend_wallet);
    let key_manager =
        KeyManager::new(wallet_type).expect("Failed to create key manager for offline signing");

    // Parse the prepared transaction
    let request = PrepareOneSidedTransactionForSigningResult::from_json(&prepared_json)
        .expect("Failed to parse prepared transaction JSON");

    // Get consensus constants for LocalNet
    let consensus_manager = ConsensusManager::builder(Network::LocalNet).build();
    let consensus_constants = consensus_manager.consensus_constants(0).clone();

    // Sign the transaction offline
    let signed_result = sign_locked_transaction(&key_manager, consensus_constants, Network::LocalNet, request)
        .expect("Offline signing failed");

    let signed_json = signed_result
        .to_json()
        .expect("Failed to serialize signed transaction");

    println!("Transaction signed offline: {} bytes of JSON", signed_json.len());
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

    let request = grpc::BroadcastSignedOneSidedTransactionRequest {
        request: signed_json,
    };

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
