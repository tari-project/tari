// Copyright 2019. The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! # Callback Categories
//! 
//! This module provides categorization and organization of wallet callbacks
//! by functional area, enabling better organization and management of the
//! callback system.

use crate::ffi::callback_signatures::{CallbackCategory, CallbackSignature, get_all_callback_signatures};
use std::collections::HashMap;

/// Callback categorization system for organizing callbacks by functional area
#[derive(Debug, Clone)]
pub struct CallbackCategorizer {
    categories: HashMap<CallbackCategory, Vec<CallbackSignature>>,
}

impl CallbackCategorizer {
    /// Create a new callback categorizer with all known callbacks
    pub fn new() -> Self {
        let mut categories = HashMap::new();
        
        // Initialize all categories
        categories.insert(CallbackCategory::Transaction, Vec::new());
        categories.insert(CallbackCategory::Balance, Vec::new());
        categories.insert(CallbackCategory::Connection, Vec::new());
        categories.insert(CallbackCategory::Communication, Vec::new());
        categories.insert(CallbackCategory::Scanning, Vec::new());
        categories.insert(CallbackCategory::Validation, Vec::new());
        
        // Categorize all callbacks
        for signature in get_all_callback_signatures() {
            if let Some(category_list) = categories.get_mut(&signature.category) {
                category_list.push(signature);
            }
        }
        
        Self { categories }
    }
    
    /// Get all callbacks in a specific category
    pub fn get_category(&self, category: &CallbackCategory) -> Option<&Vec<CallbackSignature>> {
        self.categories.get(category)
    }
    
    /// Get all available categories
    pub fn get_all_categories(&self) -> Vec<CallbackCategory> {
        self.categories.keys().cloned().collect()
    }
    
    /// Get callback count per category
    pub fn get_category_counts(&self) -> HashMap<CallbackCategory, usize> {
        self.categories
            .iter()
            .map(|(cat, sigs)| (cat.clone(), sigs.len()))
            .collect()
    }
    
    /// Find callback by name across all categories
    pub fn find_callback(&self, name: &str) -> Option<(CallbackCategory, &CallbackSignature)> {
        for (category, signatures) in &self.categories {
            for signature in signatures {
                if signature.name == name {
                    return Some((category.clone(), signature));
                }
            }
        }
        None
    }
    
    /// Get total callback count
    pub fn total_callback_count(&self) -> usize {
        self.categories.values().map(|sigs| sigs.len()).sum()
    }
}

impl Default for CallbackCategorizer {
    fn default() -> Self {
        Self::new()
    }
}

/// Category descriptions for documentation and UI display
pub fn get_category_description(category: &CallbackCategory) -> &'static str {
    match category {
        CallbackCategory::Transaction => {
            "Callbacks related to transaction lifecycle events including receiving, \
             broadcasting, mining, and cancellation"
        },
        CallbackCategory::Balance => {
            "Callbacks triggered when wallet balance information changes, including \
             available, pending, and time-locked balances"
        },
        CallbackCategory::Connection => {
            "Callbacks for network connectivity status and base node state changes, \
             monitoring connection health and synchronization"
        },
        CallbackCategory::Communication => {
            "Callbacks for peer communication events including contact liveness \
             updates and store-and-forward message reception"
        },
        CallbackCategory::Scanning => {
            "Callbacks related to blockchain scanning progress and UTXO discovery \
             operations"
        },
        CallbackCategory::Validation => {
            "Callbacks for transaction and TXO validation completion events, \
             providing validation results and error information"
        },
    }
}

/// Priority levels for different callback categories
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum CallbackPriority {
    Critical,  // Must work for basic wallet functionality
    High,      // Important for user experience
    Medium,    // Useful but not essential
    Low,       // Nice to have
}

/// Get priority level for each callback category
pub fn get_category_priority(category: &CallbackCategory) -> CallbackPriority {
    match category {
        CallbackCategory::Transaction => CallbackPriority::Critical,
        CallbackCategory::Balance => CallbackPriority::Critical,
        CallbackCategory::Connection => CallbackPriority::High,
        CallbackCategory::Validation => CallbackPriority::Medium,
        CallbackCategory::Communication => CallbackPriority::Medium,
        CallbackCategory::Scanning => CallbackPriority::Low,
    }
}

/// Implementation dependencies between callback categories
pub fn get_category_dependencies() -> HashMap<CallbackCategory, Vec<CallbackCategory>> {
    let mut deps = HashMap::new();
    
    // Balance callbacks depend on transaction callbacks
    deps.insert(
        CallbackCategory::Balance, 
        vec![CallbackCategory::Transaction]
    );
    
    // Validation callbacks depend on transactions
    deps.insert(
        CallbackCategory::Validation,
        vec![CallbackCategory::Transaction, CallbackCategory::Connection]
    );
    
    // Communication depends on connection
    deps.insert(
        CallbackCategory::Communication,
        vec![CallbackCategory::Connection]
    );
    
    // Scanning depends on connection
    deps.insert(
        CallbackCategory::Scanning,
        vec![CallbackCategory::Connection]
    );
    
    deps
}

/// Generate implementation priority matrix based on dependencies and priority
pub fn generate_implementation_priority_matrix() -> Vec<(CallbackCategory, CallbackPriority, Vec<CallbackCategory>)> {
    let categorizer = CallbackCategorizer::new();
    let dependencies = get_category_dependencies();
    
    categorizer
        .get_all_categories()
        .into_iter()
        .map(|category| {
            let priority = get_category_priority(&category);
            let deps = dependencies.get(&category).cloned().unwrap_or_default();
            (category, priority, deps)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_categorizer_creation() {
        let categorizer = CallbackCategorizer::new();
        assert_eq!(categorizer.total_callback_count(), 18);
        
        // Verify all categories exist
        for category in [
            CallbackCategory::Transaction,
            CallbackCategory::Balance,
            CallbackCategory::Connection,
            CallbackCategory::Communication,
            CallbackCategory::Scanning,
            CallbackCategory::Validation,
        ] {
            assert!(categorizer.get_category(&category).is_some());
        }
    }
    
    #[test]
    fn test_transaction_category_count() {
        let categorizer = CallbackCategorizer::new();
        let transaction_callbacks = categorizer.get_category(&CallbackCategory::Transaction).unwrap();
        
        // Should have 10 transaction-related callbacks
        assert_eq!(transaction_callbacks.len(), 10);
        
        // Verify some key transaction callbacks exist
        let callback_names: Vec<&str> = transaction_callbacks.iter()
            .map(|sig| sig.name)
            .collect();
        
        assert!(callback_names.contains(&"callback_received_transaction"));
        assert!(callback_names.contains(&"callback_transaction_mined"));
        assert!(callback_names.contains(&"callback_transaction_broadcast"));
    }
    
    #[test]
    fn test_balance_category() {
        let categorizer = CallbackCategorizer::new();
        let balance_callbacks = categorizer.get_category(&CallbackCategory::Balance).unwrap();
        
        assert_eq!(balance_callbacks.len(), 1);
        assert_eq!(balance_callbacks[0].name, "callback_balance_updated");
    }
    
    #[test]
    fn test_find_callback_by_name() {
        let categorizer = CallbackCategorizer::new();
        
        let result = categorizer.find_callback("callback_balance_updated");
        assert!(result.is_some());
        
        let (category, signature) = result.unwrap();
        assert_eq!(category, CallbackCategory::Balance);
        assert_eq!(signature.name, "callback_balance_updated");
    }
    
    #[test]
    fn test_category_priorities() {
        assert_eq!(get_category_priority(&CallbackCategory::Transaction), CallbackPriority::Critical);
        assert_eq!(get_category_priority(&CallbackCategory::Balance), CallbackPriority::Critical);
        assert_eq!(get_category_priority(&CallbackCategory::Connection), CallbackPriority::High);
    }
    
    #[test]
    fn test_category_dependencies() {
        let deps = get_category_dependencies();
        
        // Balance should depend on Transaction
        assert!(deps.get(&CallbackCategory::Balance).unwrap().contains(&CallbackCategory::Transaction));
        
        // Validation should depend on Transaction and Connection
        let validation_deps = deps.get(&CallbackCategory::Validation).unwrap();
        assert!(validation_deps.contains(&CallbackCategory::Transaction));
        assert!(validation_deps.contains(&CallbackCategory::Connection));
    }
    
    #[test]
    fn test_implementation_priority_matrix() {
        let matrix = generate_implementation_priority_matrix();
        assert_eq!(matrix.len(), 6); // All 6 categories
        
        // Find transaction category in matrix
        let transaction_entry = matrix.iter()
            .find(|(cat, _, _)| *cat == CallbackCategory::Transaction)
            .unwrap();
        
        assert_eq!(transaction_entry.1, CallbackPriority::Critical);
        assert!(transaction_entry.2.is_empty()); // No dependencies
    }
}
