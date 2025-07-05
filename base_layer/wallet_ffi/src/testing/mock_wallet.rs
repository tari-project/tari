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
use crate::event_bridge::{EventBridge, types::{WalletEvent, EventType, EventData, ConnectivityState}};

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
    event_bridge: Option<EventBridge>,
    wallet_id: u64,
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
        use rand::Rng;
        let wallet_id = rand::thread_rng().gen::<u64>();
        
        Self {
            config,
            callbacks: Arc::new(Mutex::new(HashMap::new())),
            mock_balance: Balance::zero(),
            mock_address: None,
            is_running: false,
            event_bridge: None,
            wallet_id,
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
        
        // Initialize event bridge if callbacks are enabled
        // Only create event bridge in async contexts (tests will handle separately)
        if self.config.enable_callbacks {
            // Check if we're in a tokio runtime context
            if tokio::runtime::Handle::try_current().is_ok() {
                self.event_bridge = Some(EventBridge::new(self.wallet_id));
            }
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
        
        // Clear event bridge
        self.event_bridge = None;
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
    
    /// Simulate receiving a transaction (through event bridge)
    pub async fn simulate_received_transaction(&self, tx_id: u64, amount: u64, sender: &str) -> Result<(), String> {
        if !self.is_running {
            return Err("Mock wallet is not running".to_string());
        }
        
        // Send through event bridge if available
        if let Some(ref bridge) = self.event_bridge {
            let event = WalletEvent::new(
                EventType::TransactionReceived,
                self.wallet_id,
                EventData::TransactionReceived {
                    tx_id,
                    amount,
                    sender_address: sender.to_string(),
                    message: Some("Mock transaction".to_string()),
                },
            );
            
            bridge.send_event(event).await
                .map_err(|e| format!("Failed to send event: {}", e))?;
        }
        
        // Also simulate the callback invocation for backwards compatibility
        self.simulate_callback_invocation("callback_received_transaction")
    }
    
    /// Simulate receiving a transaction (sync version for backwards compatibility)
    pub fn simulate_received_transaction_sync(&self) -> Result<(), String> {
        if !self.is_running {
            return Err("Mock wallet is not running".to_string());
        }
        
        // Simulate the callback invocation
        self.simulate_callback_invocation("callback_received_transaction")
    }
    
    /// Simulate transaction broadcast (through event bridge)
    pub async fn simulate_transaction_broadcast(&self, tx_id: u64) -> Result<(), String> {
        if !self.is_running {
            return Err("Mock wallet is not running".to_string());
        }
        
        // Send through event bridge if available
        if let Some(ref bridge) = self.event_bridge {
            let event = WalletEvent::new(
                EventType::TransactionBroadcast,
                self.wallet_id,
                EventData::TransactionBroadcast {
                    tx_id,
                    amount: 1000000, // Mock amount
                    fee: 100, // Mock fee
                },
            );
            
            bridge.send_event(event).await
                .map_err(|e| format!("Failed to send event: {}", e))?;
        }
        
        self.simulate_callback_invocation("callback_transaction_broadcast")
    }
    
    /// Simulate transaction broadcast (sync version for backwards compatibility)
    pub fn simulate_transaction_broadcast_sync(&self) -> Result<(), String> {
        if !self.is_running {
            return Err("Mock wallet is not running".to_string());
        }
        
        self.simulate_callback_invocation("callback_transaction_broadcast")
    }
    
    /// Simulate transaction mined (through event bridge)
    pub async fn simulate_transaction_mined(&self, tx_id: u64, amount: u64, block_height: Option<u64>) -> Result<(), String> {
        if !self.is_running {
            return Err("Mock wallet is not running".to_string());
        }
        
        // Send through event bridge if available
        if let Some(ref bridge) = self.event_bridge {
            let event = WalletEvent::new(
                EventType::TransactionMined,
                self.wallet_id,
                EventData::TransactionMined {
                    tx_id,
                    amount,
                    block_height,
                },
            );
            
            bridge.send_event(event).await
                .map_err(|e| format!("Failed to send event: {}", e))?;
        }
        
        self.simulate_callback_invocation("callback_transaction_mined")
    }
    
    /// Simulate transaction mined (sync version for backwards compatibility)
    pub fn simulate_transaction_mined_sync(&self) -> Result<(), String> {
        if !self.is_running {
            return Err("Mock wallet is not running".to_string());
        }
        
        self.simulate_callback_invocation("callback_transaction_mined")
    }
    
    /// Simulate connectivity status change (through event bridge)
    pub async fn simulate_connectivity_change(&self, online: bool) -> Result<(), String> {
        if !self.is_running {
            return Err("Mock wallet is not running".to_string());
        }
        
        // Send through event bridge if available
        if let Some(ref bridge) = self.event_bridge {
            let status = if online { ConnectivityState::Connected } else { ConnectivityState::Disconnected };
            let event = WalletEvent::new(
                EventType::ConnectivityStatus,
                self.wallet_id,
                EventData::ConnectivityStatus {
                    status,
                    peer_count: if online { 5 } else { 0 },
                },
            );
            
            bridge.send_event(event).await
                .map_err(|e| format!("Failed to send event: {}", e))?;
        }
        
        self.simulate_callback_invocation("callback_connectivity_status")
    }
    
    /// Simulate connectivity status change (sync version for backwards compatibility)
    pub fn simulate_connectivity_change_sync(&self, _online: bool) -> Result<(), String> {
        if !self.is_running {
            return Err("Mock wallet is not running".to_string());
        }
        
        self.simulate_callback_invocation("callback_connectivity_status")
    }
    
    /// Get a reference to the event bridge for testing
    pub fn get_event_bridge(&self) -> Option<&EventBridge> {
        self.event_bridge.as_ref()
    }
    
    /// Initialize event bridge for async testing (call this in tokio tests)
    pub fn init_event_bridge(&mut self) -> Result<(), String> {
        if !self.is_running {
            return Err("Mock wallet is not running".to_string());
        }
        
        if self.event_bridge.is_none() {
            self.event_bridge = Some(EventBridge::new(self.wallet_id));
        }
        
        Ok(())
    }
    
    /// Register an event callback through the event bridge
    pub async fn register_event_callback<F>(&self, event_type: EventType, callback_name: String, callback: F) -> Result<(), String>
    where
        F: Fn(&WalletEvent) -> Result<(), Box<dyn std::error::Error + Send + Sync>> + Send + Sync + 'static,
    {
        if let Some(ref bridge) = self.event_bridge {
            bridge.dispatcher()
                .register_callback(event_type, callback_name, callback)
                .await;
            Ok(())
        } else {
            Err("Event bridge not available".to_string())
        }
    }
    
    /// Simulate balance updated event through event bridge
    pub async fn simulate_balance_updated(&mut self, balance: Balance) -> Result<(), String> {
        if !self.is_running {
            return Err("Mock wallet is not running".to_string());
        }
        
        // Update internal balance
        self.mock_balance = balance.clone();
        
        // Send through event bridge if available
        if let Some(ref bridge) = self.event_bridge {
            let event = WalletEvent::new(
                EventType::BalanceUpdated,
                self.wallet_id,
                EventData::BalanceUpdated {
                    available: balance.available_balance.as_u64(),
                    pending_incoming: balance.pending_incoming_balance.as_u64(),
                    pending_outgoing: balance.pending_outgoing_balance.as_u64(),
                    timelocked: balance.time_locked_balance.map(|t| t.as_u64()),
                },
            );
            
            bridge.send_event(event).await
                .map_err(|e| format!("Failed to send event: {}", e))?;
        }
        
        // Also simulate the callback invocation for backwards compatibility
        self.simulate_callback_invocation("callback_balance_updated").ok();
        
        Ok(())
    }
    
    /// Get event bridge statistics
    pub async fn get_event_bridge_stats(&self) -> Option<crate::event_bridge::dispatcher::DispatcherStats> {
        if let Some(ref bridge) = self.event_bridge {
            Some(bridge.get_stats().await)
        } else {
            None
        }
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
        assert!(wallet.simulate_received_transaction_sync().is_ok());
        assert!(wallet.simulate_transaction_broadcast_sync().is_ok());
        assert!(wallet.simulate_transaction_mined_sync().is_ok());
        
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
    
    #[tokio::test]
    async fn test_event_bridge_integration() {
        let mut wallet = MockWallet::default();
        assert!(wallet.start().is_ok());
        
        // Initialize event bridge for async testing
        assert!(wallet.init_event_bridge().is_ok());
        
        // Verify event bridge is created
        assert!(wallet.get_event_bridge().is_some());
        
        // Test sending events through event bridge
        assert!(wallet.simulate_received_transaction(123, 1000000, "test_sender").await.is_ok());
        assert!(wallet.simulate_transaction_broadcast(123).await.is_ok());
        assert!(wallet.simulate_transaction_mined(123, 1000000, Some(12345)).await.is_ok());
        assert!(wallet.simulate_connectivity_change(true).await.is_ok());
        
        // Test balance update through event bridge
        let new_balance = Balance {
            available_balance: 2000000.into(),
            time_locked_balance: None,
            pending_incoming_balance: 100000.into(),
            pending_outgoing_balance: 50000.into(),
        };
        assert!(wallet.simulate_balance_updated(new_balance.clone()).await.is_ok());
        assert_eq!(wallet.get_balance().available_balance, new_balance.available_balance);
        
        // Check event bridge statistics
        let stats = wallet.get_event_bridge_stats().await;
        assert!(stats.is_some());
        if let Some(stats) = stats {
            assert!(stats.events_processed > 0);
        }
    }
}
