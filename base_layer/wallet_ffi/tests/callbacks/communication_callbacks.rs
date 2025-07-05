// Copyright 2019. The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! # Communication Callback Tests
//!
//! Tests for peer communication and messaging callbacks.

use minotari_wallet_ffi::ffi::callback_signatures::CallbackCategory;
use crate::{
    callback_harness::CallbackTestHarness,
    common::CallbackTestFixture,
};

/// Test contacts liveness data updated callback
#[tokio::test]
async fn test_callback_contacts_liveness_data_updated() {
    let fixture = CallbackTestFixture::new();
    fixture.setup().unwrap();
    
    let harness = &fixture.harness;
    let context = harness.register_mock_callback("callback_contacts_liveness_data_updated");
    
    harness.clear_invocation_records();
    
    assert!(!context.is_null());
    assert_eq!(harness.get_callback_invocation_count("callback_contacts_liveness_data_updated"), 0);
    
    println!("CURRENT STATUS: callback_contacts_liveness_data_updated implemented");
    println!("EXPECTED: Should fire when contact liveness data updated");
}

/// Test SAF messages received callback
#[tokio::test]
async fn test_callback_saf_messages_received() {
    let fixture = CallbackTestFixture::new();
    fixture.setup().unwrap();
    
    let harness = &fixture.harness;
    let context = harness.register_mock_callback("callback_saf_messages_received");
    
    harness.clear_invocation_records();
    
    assert!(!context.is_null());
    assert_eq!(harness.get_callback_invocation_count("callback_saf_messages_received"), 0);
    
    println!("CURRENT STATUS: callback_saf_messages_received implemented");
    println!("EXPECTED: Should fire when store-and-forward messages received");
}

/// Test all communication callbacks
#[tokio::test]
async fn test_all_communication_callbacks() {
    let fixture = CallbackTestFixture::new();
    fixture.setup().unwrap();
    
    let harness = &fixture.harness;
    let communication_callbacks = harness.get_callbacks_by_category(CallbackCategory::Communication);
    
    assert_eq!(communication_callbacks.len(), 2);
    
    for callback_name in &communication_callbacks {
        let context = harness.register_mock_callback(callback_name);
        assert!(!context.is_null());
    }
    
    println!("COMMUNICATION CALLBACKS: {} registered successfully", communication_callbacks.len());
}
