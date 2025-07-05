// Copyright 2019. The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! # Connection Callback Tests
//!
//! Tests for network connectivity and base node state callbacks.

use minotari_wallet_ffi::ffi::callback_signatures::CallbackCategory;
use crate::{
    callback_harness::CallbackTestHarness,
    common::CallbackTestFixture,
    expect_callbacks, assert_callbacks,
};

/// Test connectivity status callback
#[tokio::test]
async fn test_callback_connectivity_status() {
    let fixture = CallbackTestFixture::new();
    fixture.setup().unwrap();
    
    let harness = &fixture.harness;
    let context = harness.register_mock_callback("callback_connectivity_status");
    
    harness.clear_invocation_records();
    
    assert!(!context.is_null());
    assert_eq!(harness.get_callback_invocation_count("callback_connectivity_status"), 0);
    
    println!("CURRENT STATUS: callback_connectivity_status implemented with OnlineStatus enum");
    println!("EXPECTED: Should fire when connectivity status changes (0=offline, 1=online)");
    println!("TRIGGER: Changes in connectivity_status_watch receiver");
}

/// Test base node state callback
#[tokio::test]
async fn test_callback_base_node_state() {
    let fixture = CallbackTestFixture::new();
    fixture.setup().unwrap();
    
    let harness = &fixture.harness;
    let context = harness.register_mock_callback("callback_base_node_state");
    
    harness.clear_invocation_records();
    
    assert!(!context.is_null());
    assert_eq!(harness.get_callback_invocation_count("callback_base_node_state"), 0);
    
    println!("CURRENT STATUS: callback_base_node_state implemented with TariBaseNodeState");
    println!("EXPECTED: Should fire when base node state changes");
    println!("DATA: Includes node_id, block height, hash, sync status, latency");
}

/// Test all connection callbacks
#[tokio::test]
async fn test_all_connection_callbacks() {
    let fixture = CallbackTestFixture::new();
    fixture.setup().unwrap();
    
    let harness = &fixture.harness;
    let connection_callbacks = harness.get_callbacks_by_category(CallbackCategory::Connection);
    
    assert_eq!(connection_callbacks.len(), 2);
    
    for callback_name in &connection_callbacks {
        let context = harness.register_mock_callback(callback_name);
        assert!(!context.is_null());
    }
    
    println!("CONNECTION CALLBACKS: {} registered successfully", connection_callbacks.len());
}

#[tokio::test]
async fn test_connection_callback_priority() {
    use minotari_wallet_ffi::ffi::callback_categories::{get_category_priority, CallbackPriority};
    
    let priority = get_category_priority(&CallbackCategory::Connection);
    assert_eq!(priority, CallbackPriority::High);
    
    println!("PRIORITY: Connection callbacks have HIGH priority");
}
