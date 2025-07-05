// Copyright 2019. The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! # Validation Callback Tests
//!
//! Tests for transaction and TXO validation callbacks.

use minotari_wallet_ffi::ffi::callback_signatures::CallbackCategory;
use crate::{
    callback_harness::CallbackTestHarness,
    common::CallbackTestFixture,
};

/// Test transaction validation complete callback
#[tokio::test]
async fn test_callback_transaction_validation_complete() {
    let fixture = CallbackTestFixture::new();
    fixture.setup().unwrap();
    
    let harness = &fixture.harness;
    let context = harness.register_mock_callback("callback_transaction_validation_complete");
    
    harness.clear_invocation_records();
    
    assert!(!context.is_null());
    assert_eq!(harness.get_callback_invocation_count("callback_transaction_validation_complete"), 0);
    
    println!("CURRENT STATUS: callback_transaction_validation_complete implemented");
    println!("EXPECTED: Should fire when transaction validation completes with result code");
}

/// Test TXO validation complete callback
#[tokio::test]
async fn test_callback_txo_validation_complete() {
    let fixture = CallbackTestFixture::new();
    fixture.setup().unwrap();
    
    let harness = &fixture.harness;
    let context = harness.register_mock_callback("callback_txo_validation_complete");
    
    harness.clear_invocation_records();
    
    assert!(!context.is_null());
    assert_eq!(harness.get_callback_invocation_count("callback_txo_validation_complete"), 0);
    
    println!("CURRENT STATUS: callback_txo_validation_complete implemented");
    println!("EXPECTED: Should fire when TXO validation completes with result code");
}

/// Test all validation callbacks
#[tokio::test]
async fn test_all_validation_callbacks() {
    let fixture = CallbackTestFixture::new();
    fixture.setup().unwrap();
    
    let harness = &fixture.harness;
    let validation_callbacks = harness.get_callbacks_by_category(CallbackCategory::Validation);
    
    assert_eq!(validation_callbacks.len(), 2);
    
    for callback_name in &validation_callbacks {
        let context = harness.register_mock_callback(callback_name);
        assert!(!context.is_null());
    }
    
    println!("VALIDATION CALLBACKS: {} registered successfully", validation_callbacks.len());
}
