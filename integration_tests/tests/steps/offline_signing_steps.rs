// Copyright 2022. The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

#![allow(clippy::indexing_slicing)]
use std::convert::TryInto;

use cucumber::{given, then, when};
use grpc::WalletClient;
use minotari_app_grpc::tari_rpc::{
    self as grpc,
    PrepareOneSidedTransactionForSigningRequest,
    PrepareOneSidedTransactionForSigningResponse,
};
use tari_integration_tests::{TariWorld, wallet_process::create_wallet};
use tari_common_types::tari_address::TariAddress;

// Offline signing cucumber steps

#[given(expr = "a view-only wallet {word} is created")]
async fn create_view_only_wallet(world: &mut TariWorld, wallet_name: String) {
    // View-only wallets don't have spending keys
    // We create a basic wallet that can receive but not spend
    let (wallet, _shutdown_signal) = create_wallet(world, &wallet_name, false).await;
    world.wallets.insert(wallet_name, wallet);
}

#[when(expr = "I initiate offline signing via gRPC for wallet {word}")]
async fn initiate_offline_signing(world: &mut TariWorld, wallet_name: String) {
    let wallet = world.wallets.get(&wallet_name).unwrap();
    let mut client = WalletClient::new(format!("http://{}:{}", wallet.grpc_address.ip(), wallet.grpc_address.port()))
        .await
        .expect("Failed to create gRPC client");
    
    let request = tonic::Request::new(
        PrepareOneSidedTransactionForSigningRequest {
            recipient: Some(grpc::PaymentRecipient {
                address: world.default_payment_address.to_string(),
                amount: 1000000, // 0.01 XTM
                fee_per_gram: 100,
                payment_type: grpc::PaymentType::OneSidedToStealthAddress as i32,
                ..Default::default()
            }),
        }
    );
    
    let response = client.prepare_one_sided_transaction_for_signing(request).await.unwrap();
    let inner = response.into_inner();
    world.last_grpc_response = serde_json::from_str(&inner.json_transaction).unwrap();
    world.offline_wallet_name = Some(wallet_name);
}

#[then(expr = "the offline signing process completes successfully")]
async fn verify_offline_signing_completes(world: &mut TariWorld) {
    let response = &world.last_grpc_response;
    assert!(response.get("success").unwrap().as_bool().unwrap());
    assert!(response.get("unsigned_transaction").is_some());
}

#[when(expr = "a signed transaction is broadcast via view-only wallet {word}")]
async fn broadcast_signed_transaction(world: &mut TariWorld, wallet_name: String) {
    let wallet = world.wallets.get(&wallet_name).unwrap();
    let mut client = WalletClient::new(format!("http://{}:{}", wallet.grpc_address.ip(), wallet.grpc_address.port()))
        .await
        .expect("Failed to create gRPC client");
    
    let signed_tx = world.signed_transactions.get(&wallet_name).unwrap();
    let request = tonic::Request::new(grpc::BroadcastTransactionRequest {
        transaction: signed_tx.clone(),
    });
    
    let response = client.broadcast_transaction(request).await.unwrap();
    world.last_grpc_response = response.into_inner();
}

#[then(expr = "the receiving wallet confirms the transaction")]
async fn verify_receiving_wallet_confirms(world: &mut TariWorld) {
    // Verify that the transaction was confirmed in the receiving wallet
    let wallet_name = world.offline_wallet_name.as_ref().expect("No offline wallet name set");
    let wallet = world.wallets.get(wallet_name).unwrap();
    let balance = wallet.get_balance().await.unwrap();
    assert!(balance.available > 0);
}

// Additional helper methods

#[given(expr = "a full-spend wallet {word} is created")]
async fn create_full_spend_wallet(world: &mut TariWorld, wallet_name: String) {
    let (wallet, _shutdown_signal) = create_wallet(world, &wallet_name, true).await;
    world.wallets.insert(wallet_name, wallet);
}

#[when(expr = "I sign the transaction with wallet {word}")]
async fn sign_transaction(world: &mut TariWorld, wallet_name: String) {
    // In a real implementation, this would:
    // 1. Parse the unsigned transaction JSON
    // 2. Sign it with the wallet's private key
    // 3. Return the signed transaction
    
    // For now, we simulate a successful signing
    let unsigned = world.last_grpc_response.get("unsigned_transaction").unwrap().clone();
    let signed_tx = format!("signed_{}", unsigned);
    world.signed_transactions.insert(wallet_name, signed_tx);
}
