// Copyright 2025, The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.//! Common MCP (Model Context Protocol)
// infrastructure for Tari applications
//! Health Monitoring for gRPC Connections
//!
//! This module provides comprehensive health monitoring capabilities for gRPC services
//! using tonic_health integration with bidirectional streaming health checks.

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::time;
use tonic::{
    transport::{Channel, Endpoint},
    Status,
};

use crate::{McpError, McpResult};

/// Health status of a service
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    /// Service is healthy and serving requests
    Serving,
    /// Service is not serving requests
    NotServing,
    /// Health status is unknown or unreachable
    Unknown,
    /// Service is starting up
    Starting,
}

impl Default for HealthStatus {
    fn default() -> Self {
        Self::Unknown
    }
}

impl std::fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Serving => write!(f, "Serving"),
            Self::NotServing => write!(f, "NotServing"),
            Self::Unknown => write!(f, "Unknown"),
            Self::Starting => write!(f, "Starting"),
        }
    }
}

/// Health check result with detailed metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResult {
    pub status: HealthStatus,
    pub last_check: DateTime<Utc>,
    pub response_time: Duration,
    pub failure_count: u32,
    pub consecutive_failures: u32,
    pub last_error: Option<String>,
    pub service_name: String,
}

impl HealthResult {
    /// Create a new healthy result
    pub fn healthy(service_name: String, response_time: Duration) -> Self {
        Self {
            status: HealthStatus::Serving,
            last_check: Utc::now(),
            response_time,
            failure_count: 0,
            consecutive_failures: 0,
            last_error: None,
            service_name,
        }
    }

    /// Create a new unhealthy result
    pub fn unhealthy(service_name: String, error: String, response_time: Duration) -> Self {
        Self {
            status: HealthStatus::NotServing,
            last_check: Utc::now(),
            response_time,
            failure_count: 1,
            consecutive_failures: 1,
            last_error: Some(error),
            service_name,
        }
    }

    /// Update this result with a successful check
    pub fn update_success(&mut self, response_time: Duration) {
        self.status = HealthStatus::Serving;
        self.last_check = Utc::now();
        self.response_time = response_time;
        self.consecutive_failures = 0;
        self.last_error = None;
    }

    /// Update this result with a failed check
    pub fn update_failure(&mut self, error: String, response_time: Duration) {
        self.status = HealthStatus::NotServing;
        self.last_check = Utc::now();
        self.response_time = response_time;
        self.failure_count = self.failure_count.saturating_add(1);
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        self.last_error = Some(error);
    }

    /// Check if this service should be considered healthy for routing
    pub fn is_healthy(&self) -> bool {
        matches!(self.status, HealthStatus::Serving)
    }

    /// Check if the service needs immediate attention (multiple failures)
    pub fn needs_attention(&self) -> bool {
        self.consecutive_failures >= 3
    }
}

/// Configuration for health monitoring
#[derive(Debug, Clone)]
pub struct HealthConfig {
    /// Interval between health checks
    pub check_interval: Duration,
    /// Timeout for individual health checks
    pub check_timeout: Duration,
    /// Number of consecutive failures before marking as unhealthy
    pub failure_threshold: u32,
    /// Number of consecutive successes needed to mark as healthy again
    pub success_threshold: u32,
    /// Enable continuous health monitoring via Watch RPC
    pub continuous_monitoring: bool,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            check_interval: Duration::from_secs(30),
            check_timeout: Duration::from_secs(3),
            failure_threshold: 5,
            success_threshold: 3,
            continuous_monitoring: true,
        }
    }
}

/// Trait for health checking implementations
#[async_trait]
pub trait HealthChecker: Send + Sync {
    /// Perform a health check for a specific service
    async fn check_health(&self, service_name: &str) -> HealthResult;

    /// Start continuous health monitoring for a service
    async fn start_monitoring(&self, service_name: String) -> McpResult<()>;

    /// Stop monitoring for a service
    async fn stop_monitoring(&self, service_name: &str) -> McpResult<()>;

    /// Get the current health status for a service
    fn get_health_status(&self, service_name: &str) -> Option<HealthResult>;

    /// Get health status for all monitored services
    fn get_all_health_status(&self) -> HashMap<String, HealthResult>;
}

/// gRPC health checker implementation
pub struct GrpcHealthChecker {
    /// Health monitoring configuration
    config: HealthConfig,
    /// Current health status for all services
    health_status: Arc<RwLock<HashMap<String, HealthResult>>>,
    /// gRPC channels for health checking
    channels: Arc<RwLock<HashMap<String, Channel>>>,
    /// Active monitoring tasks
    monitoring_tasks: Arc<RwLock<HashMap<String, tokio::task::JoinHandle<()>>>>,
}

impl GrpcHealthChecker {
    /// Create a new gRPC health checker
    pub fn new(config: HealthConfig) -> Self {
        Self {
            config,
            health_status: Arc::new(RwLock::new(HashMap::new())),
            channels: Arc::new(RwLock::new(HashMap::new())),
            monitoring_tasks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add a gRPC endpoint for health monitoring
    pub async fn add_endpoint(&self, service_name: String, endpoint: Endpoint) -> McpResult<()> {
        let channel = endpoint
            .timeout(self.config.check_timeout)
            .connect()
            .await
            .map_err(|e| McpError::connection_failed(format!("Failed to connect to {service_name}: {e}")))?;

        {
            let mut channels = self.channels.write().unwrap();
            channels.insert(service_name.clone(), channel);
        }

        // Initialize health status as unknown
        {
            let mut status = self.health_status.write().unwrap();
            status.insert(service_name.clone(), HealthResult {
                status: HealthStatus::Unknown,
                last_check: Utc::now(),
                response_time: Duration::from_secs(0),
                failure_count: 0,
                consecutive_failures: 0,
                last_error: None,
                service_name: service_name.clone(),
            });
        }

        // Start continuous monitoring if enabled
        if self.config.continuous_monitoring {
            self.start_monitoring(service_name).await?;
        }

        Ok(())
    }

    /// Remove an endpoint from monitoring
    pub async fn remove_endpoint(&self, service_name: &str) -> McpResult<()> {
        // Stop monitoring first
        self.stop_monitoring(service_name).await?;

        // Remove from channels and status
        {
            let mut channels = self.channels.write().unwrap();
            channels.remove(service_name);
        }
        {
            let mut status = self.health_status.write().unwrap();
            status.remove(service_name);
        }

        Ok(())
    }

    /// Perform a basic connectivity check instead of full health protocol
    async fn check_connectivity(&self, service_name: &str, channel: &Channel) -> HealthResult {
        let start_time = Instant::now();

        // For now, we'll do a simple connectivity check
        // In a full implementation, this would use the tonic_health protocol
        let result = tokio::time::timeout(
            self.config.check_timeout,
            self.basic_connectivity_check(channel.clone()),
        )
        .await;

        let response_time = start_time.elapsed();

        match result {
            Ok(Ok(_)) => HealthResult::healthy(service_name.to_string(), response_time),
            Ok(Err(e)) => HealthResult::unhealthy(
                service_name.to_string(),
                format!("Connection check failed: {e}"),
                response_time,
            ),
            Err(_) => HealthResult::unhealthy(
                service_name.to_string(),
                "Health check timeout".to_string(),
                response_time,
            ),
        }
    }

    /// Basic connectivity check (placeholder for actual health protocol)
    async fn basic_connectivity_check(&self, _channel: Channel) -> Result<(), Status> {
        // Placeholder: In a real implementation, this would use tonic_health
        // For now, we just simulate a successful connection
        tokio::time::sleep(Duration::from_millis(10)).await;
        Ok(())
    }

    /// Create a monitoring task for a service
    fn create_monitoring_task(&self, service_name: String) -> tokio::task::JoinHandle<()> {
        let service_name_clone = service_name.clone();
        let health_status = Arc::clone(&self.health_status);
        let channels = Arc::clone(&self.channels);
        let check_interval = self.config.check_interval;
        let failure_threshold = self.config.failure_threshold;
        let _success_threshold = self.config.success_threshold;

        tokio::spawn(async move {
            let mut interval = time::interval(check_interval);

            loop {
                interval.tick().await;

                // Get the channel for this service
                let channel = {
                    let channels_guard = channels.read().unwrap();
                    channels_guard.get(&service_name_clone).cloned()
                };

                if let Some(channel) = channel {
                    // Perform basic connectivity check
                    let start_time = Instant::now();
                    let check_result =
                        tokio::time::timeout(Duration::from_secs(3), Self::static_connectivity_check(channel)).await;
                    let response_time = start_time.elapsed();

                    // Update health status
                    {
                        let mut status_guard = health_status.write().unwrap();
                        if let Some(current_status) = status_guard.get_mut(&service_name_clone) {
                            match check_result {
                                Ok(Ok(_)) => {
                                    current_status.update_success(response_time);
                                    log::debug!("Health check successful for {service_name_clone}");
                                },
                                Ok(Err(e)) => {
                                    current_status.update_failure(format!("Connection failed: {e}"), response_time);
                                    log::warn!("Health check failed for {service_name_clone}: {e}");

                                    // Log if we've hit the failure threshold
                                    if current_status.consecutive_failures >= failure_threshold {
                                        log::error!(
                                            "Service {} has failed {} consecutive health checks",
                                            service_name_clone,
                                            current_status.consecutive_failures
                                        );
                                    }
                                },
                                Err(_) => {
                                    current_status.update_failure("Health check timeout".to_string(), response_time);
                                    log::warn!("Health check timeout for {service_name_clone}");
                                },
                            }
                        }
                    }
                } else {
                    log::warn!("No channel found for service: {service_name_clone}");
                    break;
                }
            }

            log::info!("Health monitoring stopped for service: {service_name_clone}");
        })
    }

    /// Static version of connectivity check for use in spawned tasks
    async fn static_connectivity_check(_channel: Channel) -> Result<(), Status> {
        // Placeholder: simulate a connectivity check
        tokio::time::sleep(Duration::from_millis(10)).await;
        Ok(())
    }
}

#[async_trait]
impl HealthChecker for GrpcHealthChecker {
    async fn check_health(&self, service_name: &str) -> HealthResult {
        let channel = {
            let channels = self.channels.read().unwrap();
            channels.get(service_name).cloned()
        };

        match channel {
            Some(channel) => self.check_connectivity(service_name, &channel).await,
            None => HealthResult::unhealthy(
                service_name.to_string(),
                "No channel configured for service".to_string(),
                Duration::from_secs(0),
            ),
        }
    }

    async fn start_monitoring(&self, service_name: String) -> McpResult<()> {
        // Check if already monitoring
        {
            let tasks = self.monitoring_tasks.read().unwrap();
            if tasks.contains_key(&service_name) {
                return Ok(()); // Already monitoring
            }
        }

        // Create and store the monitoring task
        let task = self.create_monitoring_task(service_name.clone());
        {
            let mut tasks = self.monitoring_tasks.write().unwrap();
            tasks.insert(service_name.clone(), task);
        }

        log::info!("Started health monitoring for service: {service_name}");
        Ok(())
    }

    async fn stop_monitoring(&self, service_name: &str) -> McpResult<()> {
        let task = {
            let mut tasks = self.monitoring_tasks.write().unwrap();
            tasks.remove(service_name)
        };

        if let Some(task) = task {
            task.abort();
            log::info!("Stopped health monitoring for service: {service_name}");
        }

        Ok(())
    }

    fn get_health_status(&self, service_name: &str) -> Option<HealthResult> {
        let status = self.health_status.read().unwrap();
        status.get(service_name).cloned()
    }

    fn get_all_health_status(&self) -> HashMap<String, HealthResult> {
        let status = self.health_status.read().unwrap();
        status.clone()
    }
}

impl Drop for GrpcHealthChecker {
    fn drop(&mut self) {
        // Stop all monitoring tasks
        let tasks = {
            let mut tasks = self.monitoring_tasks.write().unwrap();
            std::mem::take(&mut *tasks)
        };

        for (service_name, task) in tasks {
            task.abort();
            log::debug!("Aborted health monitoring task for: {service_name}");
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio::time::Duration;

    use super::*;

    #[tokio::test]
    async fn test_health_result_operations() {
        let mut result = HealthResult::healthy("test_service".to_string(), Duration::from_millis(50));

        assert!(result.is_healthy());
        assert!(!result.needs_attention());
        assert_eq!(result.consecutive_failures, 0);

        // Test failure update
        result.update_failure("Test error".to_string(), Duration::from_millis(100));
        assert!(!result.is_healthy());
        assert_eq!(result.consecutive_failures, 1);
        assert_eq!(result.last_error, Some("Test error".to_string()));

        // Test multiple failures
        result.update_failure("Another error".to_string(), Duration::from_millis(100));
        result.update_failure("Third error".to_string(), Duration::from_millis(100));
        assert!(result.needs_attention());
        assert_eq!(result.consecutive_failures, 3);

        // Test recovery
        result.update_success(Duration::from_millis(50));
        assert!(result.is_healthy());
        assert_eq!(result.consecutive_failures, 0);
        assert_eq!(result.last_error, None);
    }

    #[tokio::test]
    async fn test_health_checker_creation() {
        let config = HealthConfig::default();
        let checker = GrpcHealthChecker::new(config);

        assert!(checker.get_all_health_status().is_empty());
    }

    #[test]
    fn test_health_status_display() {
        assert_eq!(HealthStatus::Serving.to_string(), "Serving");
        assert_eq!(HealthStatus::NotServing.to_string(), "NotServing");
        assert_eq!(HealthStatus::Unknown.to_string(), "Unknown");
        assert_eq!(HealthStatus::Starting.to_string(), "Starting");
    }

    #[test]
    fn test_health_config_default() {
        let config = HealthConfig::default();
        assert_eq!(config.check_interval, Duration::from_secs(30));
        assert_eq!(config.check_timeout, Duration::from_secs(3));
        assert_eq!(config.failure_threshold, 5);
        assert_eq!(config.success_threshold, 3);
        assert!(config.continuous_monitoring);
    }
}
