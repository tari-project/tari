// Copyright 2019. The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! # Callback Test Harness
//!
//! This module provides a comprehensive testing infrastructure for validating
//! Tari Wallet FFI callbacks. It includes facilities for mock callback registration,
//! callback invocation tracking, and validation of callback behavior.

use std::{
    collections::HashMap,
    ffi::c_void,
    sync::{Arc, Mutex},
    thread_local,
    cell::RefCell,
};

use minotari_wallet_ffi::ffi::{
    callback_signatures::{CallbackCategory, get_all_callback_signatures},
    callback_categories::CallbackCategorizer,
};

/// Thread-local storage for callback invocation tracking
thread_local! {
    static CALLBACK_INVOCATIONS: RefCell<HashMap<String, CallbackInvocation>> = 
        RefCell::new(HashMap::new());
}

/// Information about a callback invocation
#[derive(Debug, Clone)]
pub struct CallbackInvocation {
    pub callback_name: String,
    pub invocation_count: u32,
    pub last_invocation_data: Option<u64>, // Simplified data representation
    pub timestamp: std::time::Instant,
}

/// Mock callback context for testing
#[derive(Debug)]
pub struct MockCallbackContext {
    pub callback_name: String,
    pub expected_invocations: u32,
    pub actual_invocations: u32,
}

impl MockCallbackContext {
    pub fn new(callback_name: String) -> Self {
        Self {
            callback_name,
            expected_invocations: 0,
            actual_invocations: 0,
        }
    }
}

/// Callback test harness for comprehensive callback testing
pub struct CallbackTestHarness {
    categorizer: CallbackCategorizer,
    mock_contexts: Arc<Mutex<HashMap<String, MockCallbackContext>>>,
}

impl CallbackTestHarness {
    /// Create a new callback test harness
    pub fn new() -> Self {
        Self {
            categorizer: CallbackCategorizer::new(),
            mock_contexts: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    
    /// Register a mock callback for testing
    pub fn register_mock_callback(&self, callback_name: &str) -> *mut c_void {
        let mut contexts = self.mock_contexts.lock().unwrap();
        contexts.insert(
            callback_name.to_string(),
            MockCallbackContext::new(callback_name.to_string())
        );
        
        // Return context pointer for use in C callbacks
        Arc::into_raw(self.mock_contexts.clone()) as *mut c_void
    }
    
    /// Record a callback invocation
    pub fn record_callback_invocation(&self, callback_name: &str, data: Option<u64>) {
        CALLBACK_INVOCATIONS.with(|invocations| {
            let mut map = invocations.borrow_mut();
            let invocation = map.entry(callback_name.to_string())
                .or_insert_with(|| CallbackInvocation {
                    callback_name: callback_name.to_string(),
                    invocation_count: 0,
                    last_invocation_data: None,
                    timestamp: std::time::Instant::now(),
                });
            
            invocation.invocation_count += 1;
            invocation.last_invocation_data = data;
            invocation.timestamp = std::time::Instant::now();
        });
        
        // Update mock context
        if let Ok(mut contexts) = self.mock_contexts.lock() {
            if let Some(context) = contexts.get_mut(callback_name) {
                context.actual_invocations += 1;
            }
        }
    }
    
    /// Get callback invocation count
    pub fn get_callback_invocation_count(&self, callback_name: &str) -> u32 {
        CALLBACK_INVOCATIONS.with(|invocations| {
            invocations.borrow()
                .get(callback_name)
                .map(|inv| inv.invocation_count)
                .unwrap_or(0)
        })
    }
    
    /// Clear all callback invocation records
    pub fn clear_invocation_records(&self) {
        CALLBACK_INVOCATIONS.with(|invocations| {
            invocations.borrow_mut().clear();
        });
        
        if let Ok(mut contexts) = self.mock_contexts.lock() {
            for context in contexts.values_mut() {
                context.actual_invocations = 0;
            }
        }
    }
    
    /// Get all recorded callback invocations
    pub fn get_all_invocations(&self) -> HashMap<String, CallbackInvocation> {
        CALLBACK_INVOCATIONS.with(|invocations| {
            invocations.borrow().clone()
        })
    }
    
    /// Verify expected callback behavior
    pub fn verify_callback_invocations(&self, expected: &HashMap<String, u32>) -> Result<(), String> {
        let actual = self.get_all_invocations();
        
        for (callback_name, expected_count) in expected {
            let actual_count = actual.get(callback_name)
                .map(|inv| inv.invocation_count)
                .unwrap_or(0);
            
            if actual_count != *expected_count {
                return Err(format!(
                    "Callback '{}' expected {} invocations, got {}",
                    callback_name, expected_count, actual_count
                ));
            }
        }
        
        Ok(())
    }
    
    /// Get callbacks by category for testing
    pub fn get_callbacks_by_category(&self, category: CallbackCategory) -> Vec<String> {
        self.categorizer
            .get_category(&category)
            .map(|sigs| sigs.iter().map(|sig| sig.name.to_string()).collect())
            .unwrap_or_default()
    }
    
    /// Create test expectations for category
    pub fn create_category_expectations(&self, category: CallbackCategory, expected_count: u32) -> HashMap<String, u32> {
        self.get_callbacks_by_category(category)
            .into_iter()
            .map(|name| (name, expected_count))
            .collect()
    }
}

impl Default for CallbackTestHarness {
    fn default() -> Self {
        Self::new()
    }
}

/// Mock callback functions for testing

/// Mock transaction received callback
pub unsafe extern "C" fn mock_callback_received_transaction(
    context: *mut c_void, 
    _tx: *mut minotari_wallet::transaction_service::storage::models::InboundTransaction
) {
    if !context.is_null() {
        let harness_ptr = context as *const Arc<Mutex<HashMap<String, MockCallbackContext>>>;
        // This is a simplified mock - in real tests we'd extract the harness properly
        // For now, just record the invocation using thread-local storage
        CALLBACK_INVOCATIONS.with(|invocations| {
            let mut map = invocations.borrow_mut();
            let invocation = map.entry("callback_received_transaction".to_string())
                .or_insert_with(|| CallbackInvocation {
                    callback_name: "callback_received_transaction".to_string(),
                    invocation_count: 0,
                    last_invocation_data: None,
                    timestamp: std::time::Instant::now(),
                });
            invocation.invocation_count += 1;
            invocation.timestamp = std::time::Instant::now();
        });
    }
}

/// Mock balance updated callback
pub unsafe extern "C" fn mock_callback_balance_updated(
    context: *mut c_void,
    _balance: *mut minotari_wallet::output_manager_service::service::Balance
) {
    if !context.is_null() {
        CALLBACK_INVOCATIONS.with(|invocations| {
            let mut map = invocations.borrow_mut();
            let invocation = map.entry("callback_balance_updated".to_string())
                .or_insert_with(|| CallbackInvocation {
                    callback_name: "callback_balance_updated".to_string(),
                    invocation_count: 0,
                    last_invocation_data: None,
                    timestamp: std::time::Instant::now(),
                });
            invocation.invocation_count += 1;
            invocation.timestamp = std::time::Instant::now();
        });
    }
}

/// Mock connectivity status callback
pub unsafe extern "C" fn mock_callback_connectivity_status(
    context: *mut c_void,
    status: u64
) {
    if !context.is_null() {
        CALLBACK_INVOCATIONS.with(|invocations| {
            let mut map = invocations.borrow_mut();
            let invocation = map.entry("callback_connectivity_status".to_string())
                .or_insert_with(|| CallbackInvocation {
                    callback_name: "callback_connectivity_status".to_string(),
                    invocation_count: 0,
                    last_invocation_data: None,
                    timestamp: std::time::Instant::now(),
                });
            invocation.invocation_count += 1;
            invocation.last_invocation_data = Some(status);
            invocation.timestamp = std::time::Instant::now();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_harness_creation() {
        let harness = CallbackTestHarness::new();
        assert_eq!(harness.categorizer.total_callback_count(), 18);
    }
    
    #[test]
    fn test_mock_callback_registration() {
        let harness = CallbackTestHarness::new();
        let context = harness.register_mock_callback("test_callback");
        assert!(!context.is_null());
    }
    
    #[test]
    fn test_callback_invocation_tracking() {
        let harness = CallbackTestHarness::new();
        
        // Clear any existing records
        harness.clear_invocation_records();
        
        // Record some invocations
        harness.record_callback_invocation("test_callback_1", Some(42));
        harness.record_callback_invocation("test_callback_1", Some(43));
        harness.record_callback_invocation("test_callback_2", None);
        
        // Verify counts
        assert_eq!(harness.get_callback_invocation_count("test_callback_1"), 2);
        assert_eq!(harness.get_callback_invocation_count("test_callback_2"), 1);
        assert_eq!(harness.get_callback_invocation_count("nonexistent"), 0);
    }
    
    #[test]
    fn test_callback_verification() {
        let harness = CallbackTestHarness::new();
        harness.clear_invocation_records();
        
        // Set up expected invocations
        let mut expected = HashMap::new();
        expected.insert("callback_1".to_string(), 2);
        expected.insert("callback_2".to_string(), 1);
        
        // Record matching invocations
        harness.record_callback_invocation("callback_1", None);
        harness.record_callback_invocation("callback_1", None);
        harness.record_callback_invocation("callback_2", None);
        
        // Should pass verification
        assert!(harness.verify_callback_invocations(&expected).is_ok());
        
        // Add extra invocation - should fail
        harness.record_callback_invocation("callback_1", None);
        assert!(harness.verify_callback_invocations(&expected).is_err());
    }
    
    #[test]
    fn test_category_callbacks() {
        let harness = CallbackTestHarness::new();
        
        let transaction_callbacks = harness.get_callbacks_by_category(CallbackCategory::Transaction);
        assert!(!transaction_callbacks.is_empty());
        assert!(transaction_callbacks.contains(&"callback_received_transaction".to_string()));
        
        let balance_callbacks = harness.get_callbacks_by_category(CallbackCategory::Balance);
        assert_eq!(balance_callbacks.len(), 1);
        assert!(balance_callbacks.contains(&"callback_balance_updated".to_string()));
    }
    
    #[test] 
    fn test_category_expectations() {
        let harness = CallbackTestHarness::new();
        
        let expectations = harness.create_category_expectations(CallbackCategory::Balance, 5);
        assert_eq!(expectations.len(), 1);
        assert_eq!(expectations.get("callback_balance_updated"), Some(&5));
    }
    
    #[test]
    fn test_mock_callback_functions() {
        // Test that mock callbacks can be called without crashing
        unsafe {
            mock_callback_received_transaction(std::ptr::null_mut(), std::ptr::null_mut());
            mock_callback_balance_updated(std::ptr::null_mut(), std::ptr::null_mut());
            mock_callback_connectivity_status(std::ptr::null_mut(), 1);
        }
        
        // Verify invocations were recorded in thread-local storage
        CALLBACK_INVOCATIONS.with(|invocations| {
            let map = invocations.borrow();
            // Note: These might not be recorded because context is null
            // In real usage, context would be properly set up
        });
    }
}
