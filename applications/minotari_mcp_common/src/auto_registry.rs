//! Tool Registry Auto-Discovery System
//!
//! This module provides automatic tool discovery and registration based on
//! configuration and permissions, replacing manual tool registration.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use serde_json::json;
use tokio::sync::RwLock;

use crate::{
    grpc_discovery::{base_node_methods, wallet_methods, GrpcMethodCategory, GrpcMethodInfo, ServiceDiscovery},
    grpc_error_mapper::GrpcErrorMapper,
    grpc_executor::GrpcExecutor,
    schema_generator::SchemaGenerator,
    tool_metadata::{ToolCategory, ToolMetadata, ToolMetadataRegistry, ToolRiskLevel},
    McpResult,
    McpTool,
};

/// Configuration for auto-discovery
#[derive(Debug, Clone)]
pub struct AutoDiscoveryConfig {
    /// Whether to enable auto-discovery
    pub enabled: bool,
    /// Allowed gRPC methods (if empty, all are allowed)
    pub allowed_methods: HashSet<String>,
    /// Whether control operations are enabled
    pub control_enabled: bool,
    /// Server type (node, wallet, etc.)
    pub server_type: ServerType,
    /// Rate limiting configuration
    pub rate_limits: HashMap<String, u32>,
    /// Tool-specific overrides
    pub tool_overrides: HashMap<String, ToolOverride>,
}

/// Server type for context-specific discovery
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerType {
    Node,
    Wallet,
    Miner,
    Proxy,
}

impl ServerType {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Node => "base_node",
            Self::Wallet => "wallet",
            Self::Miner => "miner",
            Self::Proxy => "merge_mining_proxy",
        }
    }
}

/// Override configuration for specific tools
#[derive(Debug, Clone)]
pub struct ToolOverride {
    /// Override display name
    pub display_name: Option<String>,
    /// Override description
    pub description: Option<String>,
    /// Override category
    pub category: Option<ToolCategory>,
    /// Override risk level
    pub risk_level: Option<ToolRiskLevel>,
    /// Additional tags
    pub additional_tags: Vec<String>,
    /// Whether to disable this tool
    pub disabled: bool,
}

/// Auto-discovery registry
pub struct AutoDiscoveryRegistry {
    /// Configuration
    config: AutoDiscoveryConfig,
    /// Service discovery
    #[allow(dead_code)]
    service_discovery: Arc<ServiceDiscovery>,
    /// Tool metadata registry
    metadata_registry: Arc<RwLock<ToolMetadataRegistry>>,
    /// Schema generator
    #[allow(dead_code)]
    schema_generator: Arc<SchemaGenerator>,
    /// Error mapper
    error_mapper: Arc<GrpcErrorMapper>,
    /// gRPC executor for real method execution
    grpc_executor: Option<Arc<GrpcExecutor>>,
    /// Generated tools cache
    tools_cache: Arc<RwLock<HashMap<String, Arc<dyn McpTool>>>>,
    /// Tool status tracking
    tool_status: Arc<RwLock<HashMap<String, ToolStatus>>>,
}

/// Status of a tool in the registry
#[derive(Debug, Clone)]
pub struct ToolStatus {
    /// Whether the tool is enabled
    pub enabled: bool,
    /// Registration timestamp
    pub registered_at: std::time::SystemTime,
    /// Last used timestamp
    pub last_used: Option<std::time::SystemTime>,
    /// Usage count
    pub usage_count: u64,
    /// Error count
    pub error_count: u64,
    /// Last error
    pub last_error: Option<String>,
}

impl AutoDiscoveryRegistry {
    /// Create a new auto-discovery registry
    pub fn new(
        config: AutoDiscoveryConfig,
        service_discovery: Arc<ServiceDiscovery>,
        schema_generator: Arc<SchemaGenerator>,
        error_mapper: Arc<GrpcErrorMapper>,
    ) -> Self {
        Self {
            config,
            service_discovery,
            metadata_registry: Arc::new(RwLock::new(ToolMetadataRegistry::new())),
            schema_generator,
            error_mapper,
            grpc_executor: None,
            tools_cache: Arc::new(RwLock::new(HashMap::new())),
            tool_status: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a new auto-discovery registry with gRPC executor
    pub fn new_with_executor(
        config: AutoDiscoveryConfig,
        service_discovery: Arc<ServiceDiscovery>,
        schema_generator: Arc<SchemaGenerator>,
        error_mapper: Arc<GrpcErrorMapper>,
        grpc_executor: Arc<GrpcExecutor>,
    ) -> Self {
        Self {
            config,
            service_discovery,
            metadata_registry: Arc::new(RwLock::new(ToolMetadataRegistry::new())),
            schema_generator,
            error_mapper,
            grpc_executor: Some(grpc_executor),
            tools_cache: Arc::new(RwLock::new(HashMap::new())),
            tool_status: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Initialize and discover all available tools
    pub async fn initialize(&self) -> McpResult<()> {
        if !self.config.enabled {
            return Ok(());
        }

        // Get all methods for this server type
        let all_methods = match self.config.server_type {
            ServerType::Node => base_node_methods(),
            ServerType::Wallet => wallet_methods(),
            _ => Vec::new(),
        };

        // Convert to our ServiceDiscovery structure
        let methods = all_methods;

        // Filter methods based on configuration
        let allowed_methods = self.filter_allowed_methods(&methods);

        // Generate tools for each method
        for method in allowed_methods {
            if let Err(e) = self.generate_tool_from_method(&method).await {
                log::warn!("Failed to generate tool for method {}: {}", method.name, e);
            }
        }

        log::info!(
            "Auto-discovery completed: {} tools registered",
            self.tools_cache.read().await.len()
        );

        Ok(())
    }

    /// Filter methods based on configuration
    fn filter_allowed_methods(&self, methods: &[GrpcMethodInfo]) -> Vec<GrpcMethodInfo> {
        methods
            .iter()
            .filter(|method| {
                // Check if method is allowed
                if !self.config.allowed_methods.is_empty() {
                    if !self.config.allowed_methods.contains(&method.name) &&
                        !self
                            .config
                            .allowed_methods
                            .contains(&format!("{}/*", self.config.server_type.name()))
                    {
                        return false;
                    }
                }

                // Check control operations
                if method.is_control_operation && !self.config.control_enabled {
                    return false;
                }

                // Check tool overrides
                if let Some(override_config) = self.config.tool_overrides.get(&method.name) {
                    if override_config.disabled {
                        return false;
                    }
                }

                true
            })
            .cloned()
            .collect()
    }

    /// Generate a tool from a gRPC method
    async fn generate_tool_from_method(&self, method: &GrpcMethodInfo) -> McpResult<()> {
        // Generate tool metadata
        let metadata = self.create_tool_metadata(method).await?;

        // Create the actual tool implementation
        let tool = self.create_dynamic_tool(method, &metadata).await?;

        // Register in metadata registry
        self.metadata_registry.write().await.add_tool(metadata);

        // Cache the tool
        self.tools_cache.write().await.insert(method.name.clone(), tool);

        // Initialize status tracking
        let status = ToolStatus {
            enabled: true,
            registered_at: std::time::SystemTime::now(),
            last_used: None,
            usage_count: 0,
            error_count: 0,
            last_error: None,
        };
        self.tool_status.write().await.insert(method.name.clone(), status);

        Ok(())
    }

    /// Create tool metadata from gRPC method info
    async fn create_tool_metadata(&self, method: &GrpcMethodInfo) -> McpResult<ToolMetadata> {
        let tool_name = method.name.clone();
        let display_name = self.generate_display_name(&tool_name);
        let category = ToolCategory::from(method.category);
        let risk_level = self.determine_risk_level(method);

        // Apply overrides if configured
        let (final_display_name, final_description, final_category, final_risk_level, additional_tags) =
            if let Some(override_config) = self.config.tool_overrides.get(&tool_name) {
                (
                    override_config.display_name.clone().unwrap_or(display_name),
                    override_config
                        .description
                        .clone()
                        .unwrap_or_else(|| method.description.clone()),
                    override_config.category.unwrap_or(category),
                    override_config.risk_level.unwrap_or(risk_level),
                    override_config.additional_tags.clone(),
                )
            } else {
                (
                    display_name,
                    method.description.clone(),
                    category,
                    risk_level,
                    Vec::new(),
                )
            };

        let mut tags = vec![
            category.display_name().to_lowercase().replace(' ', "_"),
            format!("{:?}", risk_level).to_lowercase(),
            if method.is_control_operation {
                "control"
            } else {
                "read_only"
            }
            .to_string(),
        ];
        tags.extend(additional_tags);

        // Generate parameter documentation from schema
        let parameters = Vec::new(); // TODO: Implement parameter extraction from schema

        // Generate examples
        let examples = self.generate_tool_examples(method)?;

        Ok(ToolMetadata {
            name: tool_name,
            display_name: final_display_name,
            description: final_description,
            category: final_category,
            risk_level: final_risk_level,
            is_control_operation: method.is_control_operation,
            is_streaming: method.is_streaming,
            tags,
            examples,
            parameters,
            response_format: Some(method.output_schema.to_string()),
            related_tools: self.find_related_tools(method).await,
            required_permissions: if method.is_control_operation {
                vec!["mcp_control_enabled".to_string()]
            } else {
                vec![]
            },
            estimated_duration: self.estimate_duration(method),
            rate_limit: self
                .config
                .rate_limits
                .get(&method.name)
                .map(|limit| crate::tool_metadata::RateLimit {
                    requests_per_minute: *limit,
                    requests_per_hour: limit * 60,
                    burst_limit: (*limit).max(10),
                }),
            version: "1.0.0".to_string(),
            added_in_version: "1.0.0".to_string(),
            deprecation: None,
        })
    }

    /// Create a dynamic tool implementation
    async fn create_dynamic_tool(
        &self,
        method: &GrpcMethodInfo,
        metadata: &ToolMetadata,
    ) -> McpResult<Arc<dyn McpTool>> {
        Ok(Arc::new(DynamicGrpcTool::new(
            method.clone(),
            metadata.clone(),
            self.error_mapper.clone(),
            self.grpc_executor.clone(),
        )))
    }

    /// Generate display name from tool name
    fn generate_display_name(&self, name: &str) -> String {
        name.replace('_', " ")
            .split_whitespace()
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

    /// Determine risk level based on method characteristics
    fn determine_risk_level(&self, method: &GrpcMethodInfo) -> ToolRiskLevel {
        if !method.is_control_operation {
            return ToolRiskLevel::Safe;
        }

        // Check method name patterns for risk assessment
        let name_lower = method.name.to_lowercase();

        if name_lower.contains("cancel") || name_lower.contains("stop") {
            ToolRiskLevel::Low
        } else if name_lower.contains("send") || name_lower.contains("transfer") {
            if name_lower.contains("small") || name_lower.contains("test") {
                ToolRiskLevel::Medium
            } else {
                ToolRiskLevel::High
            }
        } else if name_lower.contains("burn") || name_lower.contains("destroy") {
            ToolRiskLevel::Critical
        } else if name_lower.contains("create") || name_lower.contains("generate") {
            ToolRiskLevel::Medium
        } else {
            ToolRiskLevel::Medium
        }
    }

    /// Generate example usage for a tool
    fn generate_tool_examples(&self, method: &GrpcMethodInfo) -> McpResult<Vec<crate::tool_metadata::ToolExample>> {
        // Generate basic example based on method type
        let example = match method.category {
            GrpcMethodCategory::Balance => crate::tool_metadata::ToolExample {
                title: "Check wallet balance".to_string(),
                description: "Get current wallet balance information".to_string(),
                parameters: json!({}),
                expected_response: Some(json!({
                    "balance": "1000.000000",
                    "available_balance": "950.000000"
                })),
                scenario: "Regular balance check for transaction planning".to_string(),
            },
            GrpcMethodCategory::Transaction => crate::tool_metadata::ToolExample {
                title: "Query transaction".to_string(),
                description: "Get details of a specific transaction".to_string(),
                parameters: json!({
                    "transaction_id": "example_tx_id_123"
                }),
                expected_response: Some(json!({
                    "transaction": {
                        "id": "example_tx_id_123",
                        "status": "completed",
                        "amount": "100.000000"
                    }
                })),
                scenario: "Check status of a pending transaction".to_string(),
            },
            _ => crate::tool_metadata::ToolExample {
                title: format!("Use {}", method.name),
                description: method.description.clone(),
                parameters: json!({}),
                expected_response: None,
                scenario: "General usage scenario".to_string(),
            },
        };

        Ok(vec![example])
    }

    /// Find related tools for cross-referencing
    async fn find_related_tools(&self, method: &GrpcMethodInfo) -> Vec<String> {
        // Find tools in the same category
        let registry = self.metadata_registry.read().await;
        let category_tools = registry.get_by_category(ToolCategory::from(method.category));

        category_tools
            .iter()
            .filter(|meta| meta.name != method.name)
            .take(3) // Limit to 3 related tools
            .map(|meta| meta.name.clone())
            .collect()
    }

    /// Estimate operation duration
    fn estimate_duration(&self, method: &GrpcMethodInfo) -> Option<String> {
        match method.category {
            GrpcMethodCategory::Balance | GrpcMethodCategory::Status => Some("< 1 second".to_string()),
            GrpcMethodCategory::Transaction if method.is_control_operation => Some("5-30 seconds".to_string()),
            GrpcMethodCategory::Blockchain => Some("1-5 seconds".to_string()),
            GrpcMethodCategory::Mining => Some("1-10 seconds".to_string()),
            _ => Some("1-5 seconds".to_string()),
        }
    }

    /// Get all registered tools
    pub async fn get_tools(&self) -> HashMap<String, Arc<dyn McpTool>> {
        self.tools_cache.read().await.clone()
    }

    /// Get healthy tools only (filters out tools for unhealthy services)
    pub async fn get_healthy_tools(&self) -> HashMap<String, Arc<dyn McpTool>> {
        let tools_cache = self.tools_cache.read().await;
        let tool_status = self.tool_status.read().await;

        // Check if we have health monitoring via the executor
        if let Some(ref executor) = self.grpc_executor {
            let status = executor.get_status();

            // If health monitoring is enabled, filter based on health
            if status.health_monitoring_enabled {
                if let Some(is_healthy) = status.is_healthy() {
                    if !is_healthy {
                        log::warn!("Service is unhealthy, returning empty tool list");
                        return HashMap::new();
                    }
                }
            }
        }

        // Return all tools if service is healthy or no health monitoring
        tools_cache
            .iter()
            .filter(|(name, _tool)| tool_status.get(*name).map(|status| status.enabled).unwrap_or(true))
            .map(|(name, tool)| (name.clone(), tool.clone()))
            .collect()
    }

    /// Get tool by name
    pub async fn get_tool(&self, name: &str) -> Option<Arc<dyn McpTool>> {
        self.tools_cache.read().await.get(name).cloned()
    }

    /// Get tool metadata
    pub async fn get_metadata(&self, name: &str) -> Option<ToolMetadata> {
        self.metadata_registry.read().await.get_tool(name).cloned()
    }

    /// Get registry statistics
    pub async fn get_statistics(&self) -> RegistryStatistics {
        let tools = self.tools_cache.read().await;
        let metadata = self.metadata_registry.read().await;
        let status = self.tool_status.read().await;

        let total_tools = tools.len();
        let enabled_tools = status.values().filter(|s| s.enabled).count();
        let control_tools = metadata.get_by_risk_level(ToolRiskLevel::High).len() +
            metadata.get_by_risk_level(ToolRiskLevel::Critical).len();
        let total_usage = status.values().map(|s| s.usage_count).sum();
        let total_errors = status.values().map(|s| s.error_count).sum();

        RegistryStatistics {
            total_tools,
            enabled_tools,
            control_tools,
            total_usage,
            total_errors,
            categories: metadata.get_category_summary(),
            risk_distribution: metadata.get_risk_distribution(),
        }
    }

    /// Get detailed health status for the registry
    pub async fn get_health_status(&self) -> Option<crate::grpc_executor::DetailedExecutorStatus> {
        self.grpc_executor
            .as_ref()
            .map(|executor| executor.get_detailed_status())
    }

    /// Check if the registry's backend service is healthy
    pub async fn is_service_healthy(&self) -> bool {
        if let Some(ref executor) = self.grpc_executor {
            let status = executor.get_status();
            if status.health_monitoring_enabled {
                status.is_healthy().unwrap_or(false)
            } else {
                status.is_ready()
            }
        } else {
            true // No executor means no health checking, assume healthy
        }
    }

    /// Update tool usage statistics
    pub async fn record_usage(&self, tool_name: &str, success: bool, error: Option<String>) {
        if let Some(status) = self.tool_status.write().await.get_mut(tool_name) {
            status.last_used = Some(std::time::SystemTime::now());
            status.usage_count += 1;

            if !success {
                status.error_count += 1;
                status.last_error = error;
            }
        }
    }
}

/// Registry statistics
#[derive(Debug, Clone)]
pub struct RegistryStatistics {
    pub total_tools: usize,
    pub enabled_tools: usize,
    pub control_tools: usize,
    pub total_usage: u64,
    pub total_errors: u64,
    pub categories: Vec<(ToolCategory, usize)>,
    pub risk_distribution: Vec<(ToolRiskLevel, usize)>,
}

/// Dynamic gRPC tool implementation
pub struct DynamicGrpcTool {
    method_info: GrpcMethodInfo,
    metadata: ToolMetadata,
    _error_mapper: Arc<GrpcErrorMapper>,
    grpc_executor: Option<Arc<GrpcExecutor>>,
}

impl DynamicGrpcTool {
    pub fn new(
        method_info: GrpcMethodInfo,
        metadata: ToolMetadata,
        error_mapper: Arc<GrpcErrorMapper>,
        grpc_executor: Option<Arc<GrpcExecutor>>,
    ) -> Self {
        Self {
            method_info,
            metadata,
            _error_mapper: error_mapper,
            grpc_executor,
        }
    }
}

#[async_trait::async_trait]
impl McpTool for DynamicGrpcTool {
    fn name(&self) -> &str {
        &self.metadata.name
    }

    fn description(&self) -> &str {
        &self.metadata.description
    }

    fn permission_level(&self) -> crate::security::PermissionLevel {
        if self.metadata.is_control_operation {
            crate::security::PermissionLevel::Control
        } else {
            crate::security::PermissionLevel::ReadOnly
        }
    }

    fn input_schema(&self) -> serde_json::Value {
        self.method_info.input_schema.clone()
    }

    async fn execute(&self, args: serde_json::Value) -> McpResult<serde_json::Value> {
        log::debug!(
            "Executing dynamic gRPC tool: {} with args: {}",
            self.metadata.name,
            args
        );

        match &self.grpc_executor {
            Some(executor) => {
                // Use real gRPC execution
                if executor.can_execute(&self.method_info) {
                    let result = executor.execute_method(&self.method_info, args).await;

                    // Record usage statistics
                    match &result {
                        Ok(_) => log::debug!("Successfully executed gRPC method: {}", self.method_info.name),
                        Err(e) => log::warn!("Failed to execute gRPC method {}: {}", self.method_info.name, e),
                    }

                    result
                } else {
                    Err(crate::McpError::server_error(format!(
                        "Executor cannot handle method: {} (category: {:?})",
                        self.method_info.name, self.method_info.category
                    )))
                }
            },
            None => {
                // Fallback to placeholder response when no executor is available
                log::warn!(
                    "No gRPC executor available for tool: {}, returning placeholder",
                    self.metadata.name
                );
                Ok(json!({
                    "tool": self.metadata.name,
                    "method": self.method_info.name,
                    "status": "success",
                    "note": "This is a dynamically generated tool placeholder - no gRPC executor configured",
                    "parameters": args,
                }))
            },
        }
    }
}

impl Default for AutoDiscoveryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            allowed_methods: HashSet::new(),
            control_enabled: false,
            server_type: ServerType::Node,
            rate_limits: HashMap::new(),
            tool_overrides: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_auto_discovery_registry() {
        let config = AutoDiscoveryConfig::default();
        let service_discovery = Arc::new(ServiceDiscovery::new());
        let schema_generator = Arc::new(SchemaGenerator::new(&service_discovery));
        let error_mapper = Arc::new(GrpcErrorMapper::new());

        let registry = AutoDiscoveryRegistry::new(config, service_discovery, schema_generator, error_mapper);

        // Test basic functionality
        let stats = registry.get_statistics().await;
        assert_eq!(stats.total_tools, 0);
    }

    #[test]
    fn test_risk_level_determination() {
        let config = AutoDiscoveryConfig::default();
        let service_discovery = Arc::new(ServiceDiscovery::new());
        let schema_generator = Arc::new(SchemaGenerator::new(&service_discovery));
        let error_mapper = Arc::new(GrpcErrorMapper::new());

        let registry = AutoDiscoveryRegistry::new(config, service_discovery, schema_generator, error_mapper);

        let safe_method = GrpcMethodInfo {
            name: "get_balance".to_string(),
            service: "Wallet".to_string(),
            full_name: "tari.rpc.Wallet/GetBalance".to_string(),
            description: "Get balance".to_string(),
            category: GrpcMethodCategory::Balance,
            is_control_operation: false,
            is_streaming: false,
            input_schema: json!({}),
            output_schema: json!({}),
        };

        assert_eq!(registry.determine_risk_level(&safe_method), ToolRiskLevel::Safe);
    }
}
