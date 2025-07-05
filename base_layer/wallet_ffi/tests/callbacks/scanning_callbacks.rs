// Copyright 2019. The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! # Scanning Callback Tests
//!
//! Tests for blockchain scanning and UTXO discovery callbacks.

use minotari_wallet_ffi::ffi::callback_signatures::CallbackCategory;
use crate::{
    callback_harness::CallbackTestHarness,
    common::CallbackTestFixture,
};

/// Test wallet scanned height callback
#[tokio::test]
async fn test_callback_wallet_scanned_height() {
    let fixture = CallbackTestFixture::new();
    fixture.setup().unwrap();
    
    let harness = &fixture.harness;
    let context = harness.register_mock_callback("callback_wallet_scanned_height");
    
    harness.clear_invocation_records();
    
    assert!(!context.is_null());
    assert_eq!(harness.get_callback_invocation_count("callback_wallet_scanned_height"), 0);
    
    println!("CURRENT STATUS: callback_wallet_scanned_height implemented");
    println!("EXPECTED: Should fire when wallet scan height updated");
    println!("TRIGGER: UTXO scanner progress and completion events");
}

/// Test all scanning callbacks
#[tokio::test]
async fn test_all_scanning_callbacks() {
    let fixture = CallbackTestFixture::new();
    fixture.setup().unwrap();
    
    let harness = &fixture.harness;
    let scanning_callbacks = harness.get_callbacks_by_category(CallbackCategory::Scanning);
    
    assert_eq!(scanning_callbacks.len(), 1);
    
    for callback_name in &scanning_callbacks {
        let context = harness.register_mock_callback(callback_name);
        assert!(!context.is_null());
    }
    
    println!("SCANNING CALLBACKS: {} registered successfully", scanning_callbacks.len());
}

/// Test scanning callback priority
#[tokio::test]
async fn test_scanning_callback_priority() {
    use minotari_wallet_ffi::ffi::callback_categories::{get_category_priority, CallbackPriority};
    
    let priority = get_category_priority(&CallbackCategory::Scanning);
    assert_eq!(priority, CallbackPriority::Low);
    
    println!("PRIORITY: Scanning callbacks have LOW priority (nice to have)");
}
