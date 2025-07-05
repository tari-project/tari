// Copyright 2019. The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! # Common Test Utilities
//!
//! This module provides shared utilities and fixtures for callback testing.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use minotari_wallet_ffi::ffi::{
    callback_signatures::CallbackCategory,
    callback_categories::CallbackCategorizer,
};

use crate::callback_harness::{CallbackTestHarness, CallbackInvocation};

/// Test configuration for callback tests
#[derive(Debug, Clone)]
pub struct CallbackTestConfig {
    pub temp_dir: PathBuf,
    pub log_level: u32,
    pub network: String,
    pub timeout: Duration,
}

impl Default for CallbackTestConfig {
    fn default() -> Self {
        Self {
            temp_dir: std::env::temp_dir().join("tari_callback_tests"),
            log_level: 1,
            network: "nextnet".to_string(),
            timeout: Duration::from_secs(30),
        }
    }
}

/// Test fixture for callback testing
pub struct CallbackTestFixture {
    pub config: CallbackTestConfig,
    pub harness: CallbackTestHarness,
    pub categorizer: CallbackCategorizer,
}

impl CallbackTestFixture {
    /// Create a new test fixture
    pub fn new() -> Self {
        let config = CallbackTestConfig::default();
        
        // Ensure temp directory exists
        if !config.temp_dir.exists() {
            std::fs::create_dir_all(&config.temp_dir).ok();
        }
        
        Self {
            config,
            harness: CallbackTestHarness::new(),
            categorizer: CallbackCategorizer::new(),
        }
    }
    
    /// Create fixture with custom config
    pub fn with_config(config: CallbackTestConfig) -> Self {
        Self {
            harness: CallbackTestHarness::new(),
            categorizer: CallbackCategorizer::new(),
            config,
        }
    }
    
    /// Set up test environment
    pub fn setup(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Clear any existing callback records
        self.harness.clear_invocation_records();
        
        // Ensure temp directory is clean
        if self.config.temp_dir.exists() {
            std::fs::remove_dir_all(&self.config.temp_dir)?;
        }
        std::fs::create_dir_all(&self.config.temp_dir)?;
        
        Ok(())
    }
    
    /// Clean up test environment
    pub fn cleanup(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Clear callback records
        self.harness.clear_invocation_records();
        
        // Clean up temp directory
        if self.config.temp_dir.exists() {
            std::fs::remove_dir_all(&self.config.temp_dir)?;
        }
        
        Ok(())
    }
    
    /// Wait for callback invocations with timeout
    pub fn wait_for_callbacks(
        &self,
        expected: &HashMap<String, u32>,
        timeout: Duration,
    ) -> Result<(), String> {
        let start = std::time::Instant::now();
        
        loop {
            if start.elapsed() > timeout {
                return Err("Timeout waiting for callbacks".to_string());
            }
            
            if self.harness.verify_callback_invocations(expected).is_ok() {
                return Ok(());
            }
            
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    
    /// Create mock expectations for testing
    pub fn create_test_expectations(&self) -> HashMap<String, u32> {
        let mut expectations = HashMap::new();
        
        // For Phase 1 testing, we expect most callbacks to NOT be invoked
        // since we're documenting current behavior
        for signature in minotari_wallet_ffi::ffi::callback_signatures::get_all_callback_signatures() {
            expectations.insert(signature.name.to_string(), 0);
        }
        
        expectations
    }
    
    /// Get comprehensive test report
    pub fn generate_test_report(&self) -> CallbackTestReport {
        let invocations = self.harness.get_all_invocations();
        let categorizer = &self.categorizer;
        
        let mut category_stats = HashMap::new();
        
        for category in categorizer.get_all_categories() {
            let callbacks = categorizer.get_category(&category).unwrap();
            let total_callbacks = callbacks.len();
            let invoked_callbacks = callbacks.iter()
                .filter(|sig| invocations.contains_key(sig.name))
                .count();
            
            category_stats.insert(category, CategoryStats {
                total_callbacks,
                invoked_callbacks,
                total_invocations: callbacks.iter()
                    .map(|sig| invocations.get(sig.name).map(|inv| inv.invocation_count).unwrap_or(0))
                    .sum(),
            });
        }
        
        CallbackTestReport {
            total_callbacks: categorizer.total_callback_count(),
            total_invocations: invocations.values().map(|inv| inv.invocation_count).sum(),
            category_stats,
            individual_invocations: invocations,
        }
    }
}

impl Default for CallbackTestFixture {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for CallbackTestFixture {
    fn drop(&mut self) {
        // Clean up on drop
        self.cleanup().ok();
    }
}

/// Statistics for a callback category
#[derive(Debug, Clone)]
pub struct CategoryStats {
    pub total_callbacks: usize,
    pub invoked_callbacks: usize,
    pub total_invocations: u32,
}

/// Comprehensive test report
#[derive(Debug)]
pub struct CallbackTestReport {
    pub total_callbacks: usize,
    pub total_invocations: u32,
    pub category_stats: HashMap<CallbackCategory, CategoryStats>,
    pub individual_invocations: HashMap<String, CallbackInvocation>,
}

impl CallbackTestReport {
    /// Print formatted report
    pub fn print_report(&self) {
        println!("=== Callback Test Report ===");
        println!("Total Callbacks: {}", self.total_callbacks);
        println!("Total Invocations: {}", self.total_invocations);
        println!();
        
        println!("Category Breakdown:");
        for (category, stats) in &self.category_stats {
            println!(
                "  {:?}: {}/{} callbacks invoked, {} total invocations",
                category, stats.invoked_callbacks, stats.total_callbacks, stats.total_invocations
            );
        }
        println!();
        
        if !self.individual_invocations.is_empty() {
            println!("Individual Callback Invocations:");
            for (name, invocation) in &self.individual_invocations {
                println!(
                    "  {}: {} invocations (last data: {:?})",
                    name, invocation.invocation_count, invocation.last_invocation_data
                );
            }
        } else {
            println!("No callbacks were invoked during testing.");
        }
    }
    
    /// Check if report indicates all callbacks are working
    pub fn all_callbacks_functional(&self) -> bool {
        self.category_stats.values()
            .all(|stats| stats.invoked_callbacks == stats.total_callbacks)
    }
    
    /// Get list of non-functional callbacks
    pub fn get_non_functional_callbacks(&self) -> Vec<String> {
        minotari_wallet_ffi::ffi::callback_signatures::get_all_callback_signatures()
            .into_iter()
            .filter(|sig| !self.individual_invocations.contains_key(sig.name))
            .map(|sig| sig.name.to_string())
            .collect()
    }
}

/// Helper macros for callback testing

/// Create test expectations with specified counts
#[macro_export]
macro_rules! expect_callbacks {
    ($($callback:expr => $count:expr),* $(,)?) => {{
        let mut expectations = std::collections::HashMap::new();
        $(
            expectations.insert($callback.to_string(), $count);
        )*
        expectations
    }};
}

/// Assert callback invocations match expectations
#[macro_export]
macro_rules! assert_callbacks {
    ($harness:expr, $($callback:expr => $count:expr),* $(,)?) => {{
        let expectations = expect_callbacks!($($callback => $count),*);
        match $harness.verify_callback_invocations(&expectations) {
            Ok(()) => {},
            Err(msg) => panic!("Callback assertion failed: {}", msg),
        }
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixture_creation() {
        let fixture = CallbackTestFixture::new();
        assert_eq!(fixture.categorizer.total_callback_count(), 18);
    }
    
    #[test]
    fn test_fixture_setup_and_cleanup() {
        let fixture = CallbackTestFixture::new();
        
        // Setup should succeed
        assert!(fixture.setup().is_ok());
        assert!(fixture.config.temp_dir.exists());
        
        // Cleanup should succeed
        assert!(fixture.cleanup().is_ok());
    }
    
    #[test]
    fn test_default_expectations() {
        let fixture = CallbackTestFixture::new();
        let expectations = fixture.create_test_expectations();
        
        // Should have expectations for all 18 callbacks
        assert_eq!(expectations.len(), 18);
        
        // All should expect 0 invocations (Phase 1 behavior)
        assert!(expectations.values().all(|&count| count == 0));
    }
    
    #[test]
    fn test_report_generation() {
        let fixture = CallbackTestFixture::new();
        fixture.setup().unwrap();
        
        // Generate report with no invocations
        let report = fixture.generate_test_report();
        assert_eq!(report.total_callbacks, 18);
        assert_eq!(report.total_invocations, 0);
        assert!(report.individual_invocations.is_empty());
        
        // Record some invocations
        fixture.harness.record_callback_invocation("callback_balance_updated", Some(100));
        
        let report = fixture.generate_test_report();
        assert_eq!(report.total_invocations, 1);
        assert!(!report.individual_invocations.is_empty());
    }
    
    #[test]
    fn test_macros() {
        use crate::{expect_callbacks, assert_callbacks};
        
        let expectations = expect_callbacks![
            "callback_1" => 2,
            "callback_2" => 0,
        ];
        
        assert_eq!(expectations.len(), 2);
        assert_eq!(expectations.get("callback_1"), Some(&2));
        assert_eq!(expectations.get("callback_2"), Some(&0));
        
        // Test assertion macro with harness
        let harness = CallbackTestHarness::new();
        harness.clear_invocation_records();
        
        // This should pass (expecting 0 invocations)
        assert_callbacks![harness, "nonexistent_callback" => 0];
    }
}
