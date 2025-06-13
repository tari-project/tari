//! Tool Metadata and Categorization System
//!
//! This module provides comprehensive metadata management for MCP tools including
//! categorization, risk levels, documentation generation, and enhanced discovery.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::grpc_discovery::GrpcMethodCategory;

/// Risk levels for MCP tools
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolRiskLevel {
    /// Safe operations with no side effects (read-only)
    Safe,
    /// Operations with minimal risk (balance checks, status queries)
    Low,
    /// Operations with moderate risk (small transfers, cancellations)
    Medium,
    /// Operations with significant risk (large transfers, irreversible actions)
    High,
    /// Operations with critical risk (system changes, administrative actions)
    Critical,
}

impl ToolRiskLevel {
    /// Get a human-readable description of the risk level
    pub fn description(&self) -> &'static str {
        match self {
            Self::Safe => "Read-only operation with no side effects",
            Self::Low => "Minimal risk operation with reversible effects",
            Self::Medium => "Moderate risk operation requiring user awareness",
            Self::High => "High risk operation with significant financial impact",
            Self::Critical => "Critical operation with irreversible system effects",
        }
    }
    
    /// Get recommended user interaction level
    pub fn interaction_level(&self) -> &'static str {
        match self {
            Self::Safe => "No confirmation required",
            Self::Low => "Optional confirmation recommended",
            Self::Medium => "User confirmation required",
            Self::High => "Explicit user approval with amount verification",
            Self::Critical => "Multi-step verification with delay period",
        }
    }
    
    /// Get color coding for UI display
    pub fn color_code(&self) -> &'static str {
        match self {
            Self::Safe => "#28a745",      // Green
            Self::Low => "#17a2b8",       // Blue
            Self::Medium => "#ffc107",    // Yellow
            Self::High => "#fd7e14",      // Orange
            Self::Critical => "#dc3545",  // Red
        }
    }
}

/// Tool categories for organization and discovery
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolCategory {
    // Node categories
    Blockchain,
    Mining,
    Network,
    Mempool,
    Validation,
    Status,
    
    // Wallet categories
    Balance,
    Transaction,
    Address,
    AtomicSwap,
    Recovery,
    
    // Common categories
    System,
    Diagnostic,
    Analysis,
    Monitoring,
}

impl ToolCategory {
    /// Get a human-readable display name
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Blockchain => "Blockchain Operations",
            Self::Mining => "Mining & Block Creation",
            Self::Network => "Network & Connectivity",
            Self::Mempool => "Mempool & Transactions",
            Self::Validation => "Validation & Verification",
            Self::Status => "Status & Information",
            Self::Balance => "Balance Management",
            Self::Transaction => "Transaction Operations",
            Self::Address => "Address Management",
            Self::AtomicSwap => "Atomic Swaps",
            Self::Recovery => "Recovery & Maintenance",
            Self::System => "System Operations",
            Self::Diagnostic => "Diagnostics & Troubleshooting",
            Self::Analysis => "Analysis & Insights",
            Self::Monitoring => "Monitoring & Alerts",
        }
    }
    
    /// Get a description of the category
    pub fn description(&self) -> &'static str {
        match self {
            Self::Blockchain => "Tools for querying blockchain data, blocks, and headers",
            Self::Mining => "Tools for mining operations, block templates, and PoW",
            Self::Network => "Tools for network status, peer management, and connectivity",
            Self::Mempool => "Tools for mempool analysis, transaction status, and fees",
            Self::Validation => "Tools for validating transactions, blocks, and UTXOs",
            Self::Status => "Tools for system status, sync progress, and health checks",
            Self::Balance => "Tools for balance queries, UTXO management, and liquidity",
            Self::Transaction => "Tools for creating, managing, and analyzing transactions",
            Self::Address => "Tools for address generation, validation, and conversion",
            Self::AtomicSwap => "Tools for atomic swap operations and HTLC management",
            Self::Recovery => "Tools for wallet recovery, validation, and maintenance",
            Self::System => "Tools for system information and configuration",
            Self::Diagnostic => "Tools for troubleshooting and problem diagnosis",
            Self::Analysis => "Tools for data analysis, insights, and reporting",
            Self::Monitoring => "Tools for monitoring, alerting, and health tracking",
        }
    }
    
    /// Get icon or emoji for UI display
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Blockchain => "🔗",
            Self::Mining => "⛏️",
            Self::Network => "🌐",
            Self::Mempool => "📋",
            Self::Validation => "✅",
            Self::Status => "📊",
            Self::Balance => "💰",
            Self::Transaction => "💸",
            Self::Address => "📍",
            Self::AtomicSwap => "🔄",
            Self::Recovery => "🔧",
            Self::System => "⚙️",
            Self::Diagnostic => "🔍",
            Self::Analysis => "📈",
            Self::Monitoring => "👀",
        }
    }
}

impl From<GrpcMethodCategory> for ToolCategory {
    fn from(grpc_category: GrpcMethodCategory) -> Self {
        match grpc_category {
            GrpcMethodCategory::Blockchain => Self::Blockchain,
            GrpcMethodCategory::Mining => Self::Mining,
            GrpcMethodCategory::Network => Self::Network,
            GrpcMethodCategory::Mempool => Self::Mempool,
            GrpcMethodCategory::Validation => Self::Validation,
            GrpcMethodCategory::Balance => Self::Balance,
            GrpcMethodCategory::Transaction => Self::Transaction,
            GrpcMethodCategory::Address => Self::Address,
            GrpcMethodCategory::AtomicSwap => Self::AtomicSwap,
            GrpcMethodCategory::Recovery => Self::Recovery,
            GrpcMethodCategory::System => Self::System,
            GrpcMethodCategory::Status => Self::Status,
        }
    }
}

/// Comprehensive metadata for an MCP tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolMetadata {
    /// Tool identifier (name)
    pub name: String,
    /// Human-readable display name
    pub display_name: String,
    /// Detailed description
    pub description: String,
    /// Tool category
    pub category: ToolCategory,
    /// Risk level
    pub risk_level: ToolRiskLevel,
    /// Whether this is a control operation
    pub is_control_operation: bool,
    /// Whether this tool uses streaming
    pub is_streaming: bool,
    /// Tags for discovery
    pub tags: Vec<String>,
    /// Usage examples
    pub examples: Vec<ToolExample>,
    /// Parameter documentation
    pub parameters: Vec<ParameterDoc>,
    /// Response format documentation
    pub response_format: Option<String>,
    /// Related tools
    pub related_tools: Vec<String>,
    /// Minimum required permissions
    pub required_permissions: Vec<String>,
    /// Estimated execution time
    pub estimated_duration: Option<String>,
    /// Rate limiting information
    pub rate_limit: Option<RateLimit>,
    /// Version information
    pub version: String,
    /// When this tool was added
    pub added_in_version: String,
    /// Deprecation information
    pub deprecation: Option<DeprecationInfo>,
}

/// Example usage of a tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExample {
    /// Example title
    pub title: String,
    /// Example description
    pub description: String,
    /// Example parameters
    pub parameters: serde_json::Value,
    /// Expected response (optional)
    pub expected_response: Option<serde_json::Value>,
    /// Use case scenario
    pub scenario: String,
}

/// Parameter documentation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterDoc {
    /// Parameter name
    pub name: String,
    /// Parameter type
    pub type_info: String,
    /// Whether parameter is required
    pub required: bool,
    /// Parameter description
    pub description: String,
    /// Default value if any
    pub default: Option<serde_json::Value>,
    /// Valid value range or options
    pub constraints: Option<String>,
    /// Example values
    pub examples: Vec<serde_json::Value>,
}

/// Rate limiting information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimit {
    /// Maximum requests per minute
    pub requests_per_minute: u32,
    /// Maximum requests per hour
    pub requests_per_hour: u32,
    /// Burst allowance
    pub burst_limit: u32,
}

/// Deprecation information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeprecationInfo {
    /// When tool was deprecated
    pub deprecated_in_version: String,
    /// When tool will be removed
    pub removal_version: Option<String>,
    /// Deprecation reason
    pub reason: String,
    /// Recommended replacement
    pub replacement: Option<String>,
}

/// Tool metadata registry
#[derive(Debug, Clone)]
pub struct ToolMetadataRegistry {
    /// All tool metadata indexed by name
    metadata: HashMap<String, ToolMetadata>,
    /// Tools grouped by category
    by_category: HashMap<ToolCategory, Vec<String>>,
    /// Tools grouped by risk level
    by_risk_level: HashMap<ToolRiskLevel, Vec<String>>,
    /// Tags index
    by_tags: HashMap<String, Vec<String>>,
}

impl ToolMetadataRegistry {
    /// Create a new metadata registry
    pub fn new() -> Self {
        Self {
            metadata: HashMap::new(),
            by_category: HashMap::new(),
            by_risk_level: HashMap::new(),
            by_tags: HashMap::new(),
        }
    }
    
    /// Add tool metadata
    pub fn add_tool(&mut self, metadata: ToolMetadata) {
        let name = metadata.name.clone();
        
        // Add to category index
        self.by_category
            .entry(metadata.category)
            .or_insert_with(Vec::new)
            .push(name.clone());
        
        // Add to risk level index
        self.by_risk_level
            .entry(metadata.risk_level)
            .or_insert_with(Vec::new)
            .push(name.clone());
        
        // Add to tags index
        for tag in &metadata.tags {
            self.by_tags
                .entry(tag.clone())
                .or_insert_with(Vec::new)
                .push(name.clone());
        }
        
        // Store metadata
        self.metadata.insert(name, metadata);
    }
    
    /// Get tool metadata by name
    pub fn get_tool(&self, name: &str) -> Option<&ToolMetadata> {
        self.metadata.get(name)
    }
    
    /// Get all tools in a category
    pub fn get_by_category(&self, category: ToolCategory) -> Vec<&ToolMetadata> {
        self.by_category
            .get(&category)
            .map(|names| names.iter().filter_map(|name| self.metadata.get(name)).collect())
            .unwrap_or_default()
    }
    
    /// Get all tools with a specific risk level
    pub fn get_by_risk_level(&self, risk_level: ToolRiskLevel) -> Vec<&ToolMetadata> {
        self.by_risk_level
            .get(&risk_level)
            .map(|names| names.iter().filter_map(|name| self.metadata.get(name)).collect())
            .unwrap_or_default()
    }
    
    /// Get tools by tag
    pub fn get_by_tag(&self, tag: &str) -> Vec<&ToolMetadata> {
        self.by_tags
            .get(tag)
            .map(|names| names.iter().filter_map(|name| self.metadata.get(name)).collect())
            .unwrap_or_default()
    }
    
    /// Search tools by name or description
    pub fn search(&self, query: &str) -> Vec<&ToolMetadata> {
        let query_lower = query.to_lowercase();
        self.metadata
            .values()
            .filter(|metadata| {
                metadata.name.to_lowercase().contains(&query_lower) ||
                metadata.display_name.to_lowercase().contains(&query_lower) ||
                metadata.description.to_lowercase().contains(&query_lower) ||
                metadata.tags.iter().any(|tag| tag.to_lowercase().contains(&query_lower))
            })
            .collect()
    }
    
    /// Get all categories with tool counts
    pub fn get_category_summary(&self) -> Vec<(ToolCategory, usize)> {
        let mut summary: Vec<_> = self.by_category
            .iter()
            .map(|(category, tools)| (*category, tools.len()))
            .collect();
        summary.sort_by(|a, b| a.0.display_name().cmp(b.0.display_name()));
        summary
    }
    
    /// Get risk level distribution
    pub fn get_risk_distribution(&self) -> Vec<(ToolRiskLevel, usize)> {
        let mut distribution: Vec<_> = self.by_risk_level
            .iter()
            .map(|(risk, tools)| (*risk, tools.len()))
            .collect();
        distribution.sort_by_key(|(risk, _)| match risk {
            ToolRiskLevel::Safe => 0,
            ToolRiskLevel::Low => 1,
            ToolRiskLevel::Medium => 2,
            ToolRiskLevel::High => 3,
            ToolRiskLevel::Critical => 4,
        });
        distribution
    }
    
    /// Generate comprehensive documentation
    pub fn generate_documentation(&self) -> serde_json::Value {
        serde_json::json!({
            "tool_registry": {
                "total_tools": self.metadata.len(),
                "categories": self.get_category_summary().into_iter().map(|(cat, count)| {
                    serde_json::json!({
                        "name": cat.display_name(),
                        "description": cat.description(),
                        "icon": cat.icon(),
                        "tool_count": count,
                        "tools": self.get_by_category(cat).iter().map(|meta| {
                            serde_json::json!({
                                "name": meta.name,
                                "display_name": meta.display_name,
                                "description": meta.description,
                                "risk_level": meta.risk_level,
                                "is_control_operation": meta.is_control_operation,
                                "tags": meta.tags,
                            })
                        }).collect::<Vec<_>>()
                    })
                }).collect::<Vec<_>>(),
                "risk_distribution": self.get_risk_distribution().into_iter().map(|(risk, count)| {
                    serde_json::json!({
                        "risk_level": risk,
                        "description": risk.description(),
                        "interaction_level": risk.interaction_level(),
                        "color_code": risk.color_code(),
                        "tool_count": count,
                    })
                }).collect::<Vec<_>>(),
                "all_tools": self.metadata.values().collect::<Vec<_>>(),
            }
        })
    }
}

impl Default for ToolMetadataRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Utility functions for creating tool metadata
pub mod builders {
    use super::*;
    
    /// Builder for creating tool metadata
    pub struct ToolMetadataBuilder {
        metadata: ToolMetadata,
    }
    
    impl ToolMetadataBuilder {
        pub fn new(name: &str) -> Self {
            Self {
                metadata: ToolMetadata {
                    name: name.to_string(),
                    display_name: name.replace('_', " ").to_title_case(),
                    description: String::new(),
                    category: ToolCategory::System,
                    risk_level: ToolRiskLevel::Safe,
                    is_control_operation: false,
                    is_streaming: false,
                    tags: Vec::new(),
                    examples: Vec::new(),
                    parameters: Vec::new(),
                    response_format: None,
                    related_tools: Vec::new(),
                    required_permissions: Vec::new(),
                    estimated_duration: None,
                    rate_limit: None,
                    version: "1.0.0".to_string(),
                    added_in_version: "1.0.0".to_string(),
                    deprecation: None,
                },
            }
        }
        
        pub fn display_name(mut self, display_name: &str) -> Self {
            self.metadata.display_name = display_name.to_string();
            self
        }
        
        pub fn description(mut self, description: &str) -> Self {
            self.metadata.description = description.to_string();
            self
        }
        
        pub fn category(mut self, category: ToolCategory) -> Self {
            self.metadata.category = category;
            self
        }
        
        pub fn risk_level(mut self, risk_level: ToolRiskLevel) -> Self {
            self.metadata.risk_level = risk_level;
            self
        }
        
        pub fn control_operation(mut self, is_control: bool) -> Self {
            self.metadata.is_control_operation = is_control;
            self
        }
        
        pub fn streaming(mut self, is_streaming: bool) -> Self {
            self.metadata.is_streaming = is_streaming;
            self
        }
        
        pub fn tags(mut self, tags: Vec<&str>) -> Self {
            self.metadata.tags = tags.into_iter().map(|s| s.to_string()).collect();
            self
        }
        
        pub fn add_example(mut self, example: ToolExample) -> Self {
            self.metadata.examples.push(example);
            self
        }
        
        pub fn add_parameter(mut self, param: ParameterDoc) -> Self {
            self.metadata.parameters.push(param);
            self
        }
        
        pub fn build(self) -> ToolMetadata {
            self.metadata
        }
    }
    
    trait ToTitleCase {
        fn to_title_case(&self) -> String;
    }
    
    impl ToTitleCase for str {
        fn to_title_case(&self) -> String {
            self.split_whitespace()
                .map(|word| {
                    let mut chars = word.chars();
                    match chars.next() {
                        None => String::new(),
                        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::builders::ToolMetadataBuilder;
    
    #[test]
    fn test_tool_metadata_builder() {
        let metadata = ToolMetadataBuilder::new("get_balance")
            .display_name("Get Balance")
            .description("Retrieves wallet balance information")
            .category(ToolCategory::Balance)
            .risk_level(ToolRiskLevel::Safe)
            .tags(vec!["balance", "wallet", "query"])
            .build();
        
        assert_eq!(metadata.name, "get_balance");
        assert_eq!(metadata.display_name, "Get Balance");
        assert_eq!(metadata.category, ToolCategory::Balance);
        assert_eq!(metadata.risk_level, ToolRiskLevel::Safe);
        assert_eq!(metadata.tags, vec!["balance", "wallet", "query"]);
    }
    
    #[test]
    fn test_tool_registry() {
        let mut registry = ToolMetadataRegistry::new();
        
        let metadata = ToolMetadataBuilder::new("test_tool")
            .category(ToolCategory::Balance)
            .risk_level(ToolRiskLevel::Low)
            .tags(vec!["test"])
            .build();
        
        registry.add_tool(metadata);
        
        assert!(registry.get_tool("test_tool").is_some());
        assert_eq!(registry.get_by_category(ToolCategory::Balance).len(), 1);
        assert_eq!(registry.get_by_risk_level(ToolRiskLevel::Low).len(), 1);
        assert_eq!(registry.get_by_tag("test").len(), 1);
    }
}
