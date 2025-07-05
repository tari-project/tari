// Copyright 2019. The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! # Mock Wallet for Callback Testing
//!
//! This module provides a minimal mock wallet implementation for testing
//! callback functionality without requiring a full wallet setup.

use std::{
    collections::HashMap,
    ffi::c_void,
    sync::{Arc, Mutex},
    path::PathBuf,
};

use tari_common_types::tari_address::TariAddress;
use minotari_wallet::output_manager_service::service::Balance;

/// Mock wallet configuration for testing
#[derive(Debug, Clone)]
pub struct MockWalletConfig {
    pub data_dir: PathBuf,
    pub network: String,
    pub log_level: u32,
    pub enable_callbacks: bool,
}

impl Default for MockWalletConfig {
    fn default() -> Self {
        Self {
            data_dir: std::env::temp_dir().join("tari_mock_wallet"),
            network: "nextnet".to_string(),
            log_level: 1,
            enable_callbacks: true,
        }
    }
}

/// Mock wallet implementation for callback testing
pub struct MockWallet {
    config: MockWalletConfig,
    callbacks: Arc<Mutex<HashMap<String, MockCallback>>>,
    mock_balance: Balance,
    mock_address: Option<TariAddress>,
    is_running: bool,
}

/// Mock callback registration
#[derive(Debug, Clone)]
pub struct MockCallback {
    pub name: String,
    pub context: *mut c_void,
    pub invocation_count: u32,
    pub last_invocation_time: Option<std::time::Instant>,
}

unsafe impl Send for MockCallback {}
unsafe impl Sync for MockCallback {}

impl MockWallet {
    /// Create a new mock wallet
    pub fn new(config: MockWalletConfig) -> Self {
        Self {
            config,
            callbacks: Arc::new(Mutex::new(HashMap::new())),
            mock_balance: Balance::zero(),
            mock_address: None,
            is_running: false,
        }
    }
    
    /// Create a mock wallet with default configuration
    pub fn default() -> Self {
        Self::new(MockWalletConfig::default())
    }
    
    /// Start the mock wallet
    pub fn start(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.is_running {
            return Err("Mock wallet is already running".into());
        }
        
        // Create data directory if it doesn't exist
        if !self.config.data_dir.exists() {
            std::fs::create_dir_all(&self.config.data_dir)?;
        }
        
        // Initialize mock balance
        self.mock_balance = Balance {
            available_balance: 1000000.into(),  // 1 XTR
            time_locked_balance: None,
            pending_incoming_balance: 50000.into(),  // 0.05 XTR
            pending_outgoing_balance: 25000.into(),  // 0.025 XTR
        };
        
        self.is_running = true;
        Ok(())
    }
    
    /// Stop the mock wallet
    pub fn stop(&mut self) {
        self.is_running = false;
        
        // Clear callbacks
        if let Ok(mut callbacks) = self.callbacks.lock() {
            callbacks.clear();
        }
    }
    
    /// Register a mock callback
    pub fn register_callback(&self, callback_name: &str, context: *mut c_void) -> Result<(), String> {
        if !self.is_running {
            return Err("Mock wallet is not running".to_string());
        }
        
        let callback = MockCallback {
            name: callback_name.to_string(),
            context,
            invocation_count: 0,
            last_invocation_time: None,
        };
        
        if let Ok(mut callbacks) = self.callbacks.lock() {
            callbacks.insert(callback_name.to_string(), callback);
            Ok(())
        } else {
            Err("Failed to acquire callback lock".to_string())
        }
    }
    
    /// Unregister a callback
    pub fn unregister_callback(&self, callback_name: &str) -> Result<(), String> {
        if let Ok(mut callbacks) = self.callbacks.lock() {
            callbacks.remove(callback_name);
            Ok(())
        } else {
            Err("Failed to acquire callback lock".to_string())
        }
    }
    
    /// Get registered callbacks
    pub fn get_registered_callbacks(&self) -> Vec<String> {
        if let Ok(callbacks) = self.callbacks.lock() {
            callbacks.keys().cloned().collect()
        } else {
            Vec::new()
        }
    }
    
    /// Check if callback is registered
    pub fn is_callback_registered(&self, callback_name: &str) -> bool {
        if let Ok(callbacks) = self.callbacks.lock() {
            callbacks.contains_key(callback_name)
        } else {
            false
        }
    }
    
    /// Simulate a callback invocation (for testing)
    pub fn simulate_callback_invocation(&self, callback_name: &str) -> Result<(), String> {
        if let Ok(mut callbacks) = self.callbacks.lock() {
            if let Some(callback) = callbacks.get_mut(callback_name) {
                callback.invocation_count += 1;
                callback.last_invocation_time = Some(std::time::Instant::now());
                Ok(())
            } else {
                Err(format!("Callback '{}' not registered", callback_name))
            }
        } else {
            Err("Failed to acquire callback lock".to_string())
        }
    }
    
    /// Get callback invocation count
    pub fn get_callback_invocation_count(&self, callback_name: &str) -> u32 {
        if let Ok(callbacks) = self.callbacks.lock() {
            callbacks.get(callback_name)
                .map(|cb| cb.invocation_count)
                .unwrap_or(0)
        } else {
            0
        }
    }
    
    /// Get mock balance
    pub fn get_balance(&self) -> Balance {
        self.mock_balance.clone()
    }
    
    /// Update mock balance (for testing balance change callbacks)
    pub fn update_balance(&mut self, new_balance: Balance) {
        self.mock_balance = new_balance;
        
        // Simulate balance update callback
        self.simulate_callback_invocation("callback_balance_updated").ok();
    }
    
    /// Simulate receiving a transaction
    pub fn simulate_received_transaction(&self) -> Result<(), String> {
        if !self.is_running {
            return Err("Mock wallet is not running".to_string());
        }
        
        // Simulate the callback invocation
        self.simulate_callback_invocation("callback_received_transaction")
    }
    
    /// Simulate transaction broadcast
    pub fn simulate_transaction_broadcast(&self) -> Result<(), String> {
        if !self.is_running {
            return Err("Mock wallet is not running".to_string());
        }
        
        self.simulate_callback_invocation("callback_transaction_broadcast")
    }
    
    /// Simulate transaction mined
    pub fn simulate_transaction_mined(&self) -> Result<(), String> {
        if !self.is_running {
            return Err("Mock wallet is not running".to_string());
        }
        
        self.simulate_callback_invocation("callback_transaction_mined")
    }
    
    /// Simulate connectivity status change
    pub fn simulate_connectivity_change(&self, online: bool) -> Result<(), String> {
        if !self.is_running {
            return Err("Mock wallet is not running".to_string());
        }
        
        self.simulate_callback_invocation("callback_connectivity_status")
    }
    
    /// Get comprehensive status for testing
    pub fn get_status(&self) -> MockWalletStatus {
        let registered_callbacks = self.get_registered_callbacks();
        let callback_stats = if let Ok(callbacks) = self.callbacks.lock() {
            callbacks.iter()
                .map(|(name, cb)| (name.clone(), cb.invocation_count))
                .collect()
        } else {
            HashMap::new()
        };
        
        MockWalletStatus {
            is_running: self.is_running,
            registered_callbacks,
            callback_invocation_counts: callback_stats,
            current_balance: self.mock_balance.clone(),
        }
    }
}

impl Drop for MockWallet {
    fn drop(&mut self) {
        self.stop();
        
        // Clean up data directory
        if self.config.data_dir.exists() {
            std::fs::remove_dir_all(&self.config.data_dir).ok();
        }
    }
}

/// Status information for mock wallet
#[derive(Debug, Clone)]
pub struct MockWalletStatus {
    pub is_running: bool,
    pub registered_callbacks: Vec<String>,
    pub callback_invocation_counts: HashMap<String, u32>,
    pub current_balance: Balance,
}

impl MockWalletStatus {
    /// Print formatted status
    pub fn print_status(&self) {
        println!("=== Mock Wallet Status ===");
        println!("Running: {}", self.is_running);
        println!("Registered Callbacks: {}", self.registered_callbacks.len());
        
        for callback in &self.registered_callbacks {
            let count = self.callback_invocation_counts.get(callback).unwrap_or(&0);
            println!("  {}: {} invocations", callback, count);
        }
        
        println!("Balance:");
        println!("  Available: {}", self.current_balance.available_balance);
        println!("  Pending In: {}", self.current_balance.pending_incoming_balance);
        println!("  Pending Out: {}", self.current_balance.pending_outgoing_balance);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_wallet_creation() {
        let wallet = MockWallet::default();
        assert!(!wallet.is_running);
        assert_eq!(wallet.get_registered_callbacks().len(), 0);
    }
    
    #[test]
    fn test_mock_wallet_lifecycle() {
        let mut wallet = MockWallet::default();
        
        // Start wallet
        assert!(wallet.start().is_ok());
        assert!(wallet.is_running);
        
        // Stop wallet
        wallet.stop();
        assert!(!wallet.is_running);
    }
    
    #[test]
    fn test_callback_registration() {
        let mut wallet = MockWallet::default();
        assert!(wallet.start().is_ok());
        
        let context = std::ptr::null_mut();
        
        // Register callback
        assert!(wallet.register_callback("test_callback", context).is_ok());
        assert!(wallet.is_callback_registered("test_callback"));
        
        // Unregister callback
        assert!(wallet.unregister_callback("test_callback").is_ok());
        assert!(!wallet.is_callback_registered("test_callback"));
    }
    
    #[test]
    fn test_callback_simulation() {
        let mut wallet = MockWallet::default();
        assert!(wallet.start().is_ok());
        
        let context = std::ptr::null_mut();
        
        // Register and simulate callback
        assert!(wallet.register_callback("test_callback", context).is_ok());
        assert_eq!(wallet.get_callback_invocation_count("test_callback"), 0);
        
        assert!(wallet.simulate_callback_invocation("test_callback").is_ok());
        assert_eq!(wallet.get_callback_invocation_count("test_callback"), 1);
        
        assert!(wallet.simulate_callback_invocation("test_callback").is_ok());
        assert_eq!(wallet.get_callback_invocation_count("test_callback"), 2);
    }
    
    #[test]
    fn test_transaction_simulations() {
        let mut wallet = MockWallet::default();
        assert!(wallet.start().is_ok());
        
        let context = std::ptr::null_mut();
        
        // Register transaction callbacks
        assert!(wallet.register_callback("callback_received_transaction", context).is_ok());
        assert!(wallet.register_callback("callback_transaction_broadcast", context).is_ok());
        assert!(wallet.register_callback("callback_transaction_mined", context).is_ok());
        
        // Simulate transaction flow
        assert!(wallet.simulate_received_transaction().is_ok());
        assert!(wallet.simulate_transaction_broadcast().is_ok());
        assert!(wallet.simulate_transaction_mined().is_ok());
        
        // Verify invocations
        assert_eq!(wallet.get_callback_invocation_count("callback_received_transaction"), 1);
        assert_eq!(wallet.get_callback_invocation_count("callback_transaction_broadcast"), 1);
        assert_eq!(wallet.get_callback_invocation_count("callback_transaction_mined"), 1);
    }
    
    #[test]
    fn test_balance_operations() {
        let mut wallet = MockWallet::default();
        assert!(wallet.start().is_ok());
        
        let initial_balance = wallet.get_balance();
        assert_eq!(initial_balance.available_balance, 1000000.into());
        
        // Update balance
        let new_balance = Balance {
            available_balance: 2000000.into(),
            time_locked_balance: None,
            pending_incoming_balance: 0.into(),
            pending_outgoing_balance: 0.into(),
        };
        
        wallet.update_balance(new_balance.clone());
        let updated_balance = wallet.get_balance();
        assert_eq!(updated_balance.available_balance, new_balance.available_balance);
    }
    
    #[test]
    fn test_wallet_status() {
        let mut wallet = MockWallet::default();
        assert!(wallet.start().is_ok());
        
        let context = std::ptr::null_mut();
        assert!(wallet.register_callback("test_callback", context).is_ok());
        assert!(wallet.simulate_callback_invocation("test_callback").is_ok());
        
        let status = wallet.get_status();
        assert!(status.is_running);
        assert_eq!(status.registered_callbacks.len(), 1);
        assert_eq!(status.callback_invocation_counts.get("test_callback"), Some(&1));
    }
}
