//! Conversion Registry Factory
//!
//! This module provides factory functions for creating properly configured
//! ConversionRegistry instances with all method converters registered.

use crate::{
    parameter_converter::ConversionRegistry,
    method_implementations::{register_node_converters, register_wallet_converters},
    auto_registry::ServerType,
};
use std::sync::Arc;

/// Factory for creating conversion registries with appropriate method converters
pub struct ConversionRegistryFactory;

impl ConversionRegistryFactory {
    /// Create a conversion registry for node operations
    pub fn create_node_registry() -> Arc<ConversionRegistry> {
        let mut registry = ConversionRegistry::new();
        
        // Register all node method converters
        register_node_converters(&mut registry);
        
        Arc::new(registry)
    }
    
    /// Create a conversion registry for wallet operations
    pub fn create_wallet_registry() -> Arc<ConversionRegistry> {
        let mut registry = ConversionRegistry::new();
        
        // Register all wallet method converters
        register_wallet_converters(&mut registry);
        
        Arc::new(registry)
    }
    
    /// Create a conversion registry based on server type
    pub fn create_registry_for_server(server_type: ServerType) -> Arc<ConversionRegistry> {
        match server_type {
            ServerType::Node => Self::create_node_registry(),
            ServerType::Wallet => Self::create_wallet_registry(),
            ServerType::Miner | ServerType::Proxy => {
                // For now, these use node registry as they primarily interact with node
                Self::create_node_registry()
            }
        }
    }
    
    /// Create a combined registry with both node and wallet converters
    /// This is useful for applications that need to handle both types
    pub fn create_combined_registry() -> Arc<ConversionRegistry> {
        let mut registry = ConversionRegistry::new();
        
        // Register node converters
        register_node_converters(&mut registry);
        
        // Register wallet converters
        register_wallet_converters(&mut registry);
        
        Arc::new(registry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    
    #[test]
    fn test_node_registry_creation() {
        let registry = ConversionRegistryFactory::create_node_registry();
        
        // Test that node methods are registered
        assert!(registry.has_converter("GetTipInfo"));
        assert!(registry.has_converter("GetBlocks"));
        assert!(registry.has_converter("GetVersion"));
        assert!(registry.has_converter("GetPeers"));
        
        // Test that wallet methods are not registered
        assert!(!registry.has_converter("GetBalance"));
    }
    
    #[test]
    fn test_wallet_registry_creation() {
        let registry = ConversionRegistryFactory::create_wallet_registry();
        
        // Test that wallet methods are registered
        assert!(registry.has_converter("GetBalance"));
        
        // Test that node methods are not registered
        assert!(!registry.has_converter("GetTipInfo"));
    }
    
    #[test]
    fn test_server_type_registry_creation() {
        let node_registry = ConversionRegistryFactory::create_registry_for_server(ServerType::Node);
        assert!(node_registry.has_converter("GetTipInfo"));
        assert!(!node_registry.has_converter("GetBalance"));
        
        let wallet_registry = ConversionRegistryFactory::create_registry_for_server(ServerType::Wallet);
        assert!(wallet_registry.has_converter("GetBalance"));
        assert!(!wallet_registry.has_converter("GetTipInfo"));
    }
    
    #[test]
    fn test_combined_registry_creation() {
        let registry = ConversionRegistryFactory::create_combined_registry();
        
        // Test that both node and wallet methods are registered
        assert!(registry.has_converter("GetTipInfo"));
        assert!(registry.has_converter("GetBlocks"));
        assert!(registry.has_converter("GetBalance"));
    }
    
    #[tokio::test(flavor = "multi_thread")]
    async fn test_parameter_conversion() {
        let registry = ConversionRegistryFactory::create_node_registry();
        
        // Test GetTipInfo conversion (no parameters)
        let result = registry.convert("GetTipInfo", json!({}));
        assert!(result.is_ok());
        
        // Test GetBlocks conversion with heights
        let result = registry.convert("GetBlocks", json!({"heights": [100, 200]}));
        assert!(result.is_ok());
        
        // Test unknown method
        let result = registry.convert("UnknownMethod", json!({}));
        assert!(result.is_err());
    }
}
