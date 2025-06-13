//! Connection Management and Circuit Breaker Implementation
//!
//! This module provides connection pooling, circuit breaker patterns, and connection
//! health management for gRPC services using Tower middleware.

use crate::{
    health_checker::{HealthChecker, HealthResult, GrpcHealthChecker, HealthConfig},
    McpResult, McpError,
};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};
use tokio::sync::RwLock as AsyncRwLock;
use tonic::{
    transport::{Channel, Endpoint},
    Status,
};
use serde::{Deserialize, Serialize};

/// Circuit breaker state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CircuitBreakerState {
    /// Circuit is closed, requests are flowing normally
    Closed,
    /// Circuit is open, requests are being rejected
    Open,
    /// Circuit is half-open, testing if service has recovered
    HalfOpen,
}

impl Default for CircuitBreakerState {
    fn default() -> Self {
        Self::Closed
    }
}

impl std::fmt::Display for CircuitBreakerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Closed => write!(f, "Closed"),
            Self::Open => write!(f, "Open"),
            Self::HalfOpen => write!(f, "HalfOpen"),
        }
    }
}

/// Circuit breaker configuration
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of failures needed to open the circuit
    pub failure_threshold: u32,
    /// Number of successes needed to close the circuit from half-open
    pub success_threshold: u32,
    /// Time to wait before transitioning from open to half-open
    pub timeout: Duration,
    /// Time window for counting failures
    pub failure_window: Duration,
    /// Request timeout for individual requests
    pub request_timeout: Duration,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 3,
            timeout: Duration::from_secs(30),
            failure_window: Duration::from_secs(60),
            request_timeout: Duration::from_secs(3),
        }
    }
}

/// Circuit breaker metrics and state
#[derive(Debug, Clone)]
pub struct CircuitBreakerMetrics {
    pub state: CircuitBreakerState,
    pub failure_count: u32,
    pub success_count: u32,
    pub last_failure_time: Option<Instant>,
    pub last_state_change: Instant,
    pub total_requests: u64,
    pub rejected_requests: u64,
    pub successful_requests: u64,
}

impl CircuitBreakerMetrics {
    pub fn new() -> Self {
        Self {
            state: CircuitBreakerState::Closed,
            failure_count: 0,
            success_count: 0,
            last_failure_time: None,
            last_state_change: Instant::now(),
            total_requests: 0,
            rejected_requests: 0,
            successful_requests: 0,
        }
    }

    /// Calculate the failure rate as a percentage
    pub fn failure_rate(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            (self.total_requests - self.successful_requests) as f64 / self.total_requests as f64 * 100.0
        }
    }

    /// Check if the circuit should transition to half-open
    pub fn should_attempt_reset(&self, timeout: Duration) -> bool {
        matches!(self.state, CircuitBreakerState::Open) &&
        self.last_failure_time
            .map(|last_failure| last_failure.elapsed() >= timeout)
            .unwrap_or(false)
    }
}

impl Default for CircuitBreakerMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Circuit breaker implementation
#[derive(Clone)]
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    metrics: Arc<RwLock<CircuitBreakerMetrics>>,
    service_name: String,
}

impl CircuitBreaker {
    /// Create a new circuit breaker
    pub fn new(service_name: String, config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            metrics: Arc::new(RwLock::new(CircuitBreakerMetrics::new())),
            service_name,
        }
    }

    /// Execute a request through the circuit breaker
    pub async fn execute<F, T, E>(&self, request: F) -> Result<T, McpError>
    where
        F: std::future::Future<Output = Result<T, E>> + Send,
        E: std::fmt::Display,
    {
        // Check if we should reject the request
        if self.should_reject_request().await {
            let mut metrics = self.metrics.write().unwrap();
            metrics.rejected_requests += 1;
            metrics.total_requests += 1;
            
            return Err(McpError::service_unavailable(format!(
                "Circuit breaker is OPEN for service: {} (failure rate: {:.1}%)",
                self.service_name,
                metrics.failure_rate()
            )));
        }

        // Execute the request with timeout
        let start_time = Instant::now();
        let result = tokio::time::timeout(self.config.request_timeout, request).await;

        // Update metrics based on the result
        match result {
            Ok(Ok(success)) => {
                self.record_success().await;
                Ok(success)
            }
            Ok(Err(error)) => {
                self.record_failure().await;
                Err(McpError::service_error(format!(
                    "Request failed for {}: {}",
                    self.service_name, error
                )))
            }
            Err(_) => {
                self.record_failure().await;
                Err(McpError::service_error(format!(
                    "Request timeout for {} ({}ms)",
                    self.service_name,
                    start_time.elapsed().as_millis()
                )))
            }
        }
    }

    /// Check if we should reject the request due to circuit breaker state
    async fn should_reject_request(&self) -> bool {
        let mut metrics = self.metrics.write().unwrap();
        
        match metrics.state {
            CircuitBreakerState::Closed => false,
            CircuitBreakerState::Open => {
                // Check if we should transition to half-open
                if metrics.should_attempt_reset(self.config.timeout) {
                    log::info!("Circuit breaker transitioning to HALF_OPEN for service: {}", self.service_name);
                    metrics.state = CircuitBreakerState::HalfOpen;
                    metrics.last_state_change = Instant::now();
                    metrics.success_count = 0;
                    false
                } else {
                    true
                }
            }
            CircuitBreakerState::HalfOpen => false,
        }
    }

    /// Record a successful request
    async fn record_success(&self) {
        let mut metrics = self.metrics.write().unwrap();
        metrics.total_requests += 1;
        metrics.successful_requests += 1;
        
        match metrics.state {
            CircuitBreakerState::Closed => {
                // Reset failure count on success
                metrics.failure_count = 0;
            }
            CircuitBreakerState::HalfOpen => {
                metrics.success_count += 1;
                if metrics.success_count >= self.config.success_threshold {
                    log::info!("Circuit breaker transitioning to CLOSED for service: {}", self.service_name);
                    metrics.state = CircuitBreakerState::Closed;
                    metrics.last_state_change = Instant::now();
                    metrics.failure_count = 0;
                    metrics.success_count = 0;
                }
            }
            CircuitBreakerState::Open => {
                // This shouldn't happen as we should have transitioned to half-open
                log::warn!("Received success while circuit breaker is OPEN for: {}", self.service_name);
            }
        }
    }

    /// Record a failed request
    async fn record_failure(&self) {
        let mut metrics = self.metrics.write().unwrap();
        metrics.total_requests += 1;
        metrics.failure_count += 1;
        metrics.last_failure_time = Some(Instant::now());

        match metrics.state {
            CircuitBreakerState::Closed => {
                if metrics.failure_count >= self.config.failure_threshold {
                    log::warn!(
                        "Circuit breaker transitioning to OPEN for service: {} (failures: {})",
                        self.service_name, metrics.failure_count
                    );
                    metrics.state = CircuitBreakerState::Open;
                    metrics.last_state_change = Instant::now();
                }
            }
            CircuitBreakerState::HalfOpen => {
                log::warn!("Circuit breaker transitioning back to OPEN for service: {}", self.service_name);
                metrics.state = CircuitBreakerState::Open;
                metrics.last_state_change = Instant::now();
                metrics.success_count = 0;
            }
            CircuitBreakerState::Open => {
                // Already open, just update metrics
            }
        }
    }

    /// Get current circuit breaker metrics
    pub fn get_metrics(&self) -> CircuitBreakerMetrics {
        let metrics = self.metrics.read().unwrap();
        metrics.clone()
    }

    /// Get current state
    pub fn get_state(&self) -> CircuitBreakerState {
        let metrics = self.metrics.read().unwrap();
        metrics.state
    }
}

/// Connection pool configuration
#[derive(Debug, Clone)]
pub struct ConnectionPoolConfig {
    /// Maximum number of connections per endpoint
    pub max_connections: usize,
    /// Connection idle timeout
    pub idle_timeout: Duration,
    /// Keep-alive interval for connections
    pub keep_alive_interval: Duration,
    /// Keep-alive timeout
    pub keep_alive_timeout: Duration,
    /// Connection establishment timeout
    pub connect_timeout: Duration,
}

impl Default for ConnectionPoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 100,
            idle_timeout: Duration::from_secs(10),
            keep_alive_interval: Duration::from_secs(45),
            keep_alive_timeout: Duration::from_secs(15),
            connect_timeout: Duration::from_secs(5),
        }
    }
}

/// Managed connection with health awareness
#[derive(Clone)]
pub struct ManagedConnection {
    pub channel: Channel,
    pub endpoint_url: String,
    pub created_at: Instant,
    pub last_used: Arc<RwLock<Instant>>,
    pub use_count: Arc<RwLock<u64>>,
}

impl ManagedConnection {
    pub fn new(channel: Channel, endpoint_url: String) -> Self {
        let now = Instant::now();
        Self {
            channel,
            endpoint_url,
            created_at: now,
            last_used: Arc::new(RwLock::new(now)),
            use_count: Arc::new(RwLock::new(0)),
        }
    }

    /// Mark this connection as used
    pub fn mark_used(&self) {
        {
            let mut last_used = self.last_used.write().unwrap();
            *last_used = Instant::now();
        }
        {
            let mut use_count = self.use_count.write().unwrap();
            *use_count += 1;
        }
    }

    /// Check if connection is idle and should be cleaned up
    pub fn is_idle(&self, timeout: Duration) -> bool {
        let last_used = self.last_used.read().unwrap();
        last_used.elapsed() > timeout
    }

    /// Get connection statistics
    pub fn get_stats(&self) -> (Duration, Duration, u64) {
        let last_used = self.last_used.read().unwrap();
        let use_count = self.use_count.read().unwrap();
        (self.created_at.elapsed(), last_used.elapsed(), *use_count)
    }
}

/// Connection manager with health monitoring and circuit breakers
pub struct ConnectionManager {
    /// Connection pools per service
    connections: Arc<AsyncRwLock<HashMap<String, Vec<ManagedConnection>>>>,
    /// Circuit breakers per service
    circuit_breakers: Arc<RwLock<HashMap<String, CircuitBreaker>>>,
    /// Health checker
    health_checker: Arc<GrpcHealthChecker>,
    /// Configuration for connections
    pool_config: ConnectionPoolConfig,
    /// Configuration for circuit breakers
    circuit_config: CircuitBreakerConfig,
}

impl ConnectionManager {
    /// Create a new connection manager
    pub fn new(
        pool_config: ConnectionPoolConfig,
        circuit_config: CircuitBreakerConfig,
        health_config: HealthConfig,
    ) -> Self {
        let health_checker = Arc::new(GrpcHealthChecker::new(health_config));
        
        Self {
            connections: Arc::new(AsyncRwLock::new(HashMap::new())),
            circuit_breakers: Arc::new(RwLock::new(HashMap::new())),
            health_checker,
            pool_config,
            circuit_config,
        }
    }

    /// Add a service endpoint for management
    pub async fn add_service(&self, service_name: String, endpoint: Endpoint) -> McpResult<()> {
        // Add to health monitoring
        self.health_checker.add_endpoint(service_name.clone(), endpoint.clone()).await?;

        // Create circuit breaker
        let circuit_breaker = CircuitBreaker::new(service_name.clone(), self.circuit_config.clone());
        {
            let mut breakers = self.circuit_breakers.write().unwrap();
            breakers.insert(service_name.clone(), circuit_breaker);
        }

        // Create initial connection pool
        let connections = self.create_connection_pool(endpoint).await?;
        {
            let mut conn_pools = self.connections.write().await;
            conn_pools.insert(service_name.clone(), connections);
        }

        log::info!("Added service {} to connection manager", service_name);
        Ok(())
    }

    /// Get a connection for a service, considering health and circuit breaker status
    pub async fn get_connection(&self, service_name: &str) -> McpResult<Channel> {
        // Check health status first
        if let Some(health) = self.health_checker.get_health_status(service_name) {
            if !health.is_healthy() {
                return Err(McpError::service_unavailable(format!(
                    "Service {} is not healthy: {}",
                    service_name, health.status
                )));
            }
        }

        // Check circuit breaker
        let circuit_breaker = {
            let breakers = self.circuit_breakers.read().unwrap();
            breakers.get(service_name).cloned()
        };

        if let Some(breaker) = circuit_breaker {
            let state = breaker.get_state();
            if matches!(state, CircuitBreakerState::Open) {
                return Err(McpError::service_unavailable(format!(
                    "Circuit breaker is OPEN for service: {}",
                    service_name
                )));
            }
        }

        // Get connection from pool
        self.get_pooled_connection(service_name).await
    }

    /// Execute a request through the connection manager with all protections
    pub async fn execute_with_circuit_breaker<F, T>(
        &self,
        service_name: &str,
        request: F,
    ) -> McpResult<T>
    where
        F: std::future::Future<Output = Result<T, Status>> + Send,
    {
        let circuit_breaker = {
            let breakers = self.circuit_breakers.read().unwrap();
            breakers.get(service_name).cloned()
        };

        match circuit_breaker {
            Some(breaker) => breaker.execute(request).await,
            None => {
                // No circuit breaker configured, execute directly
                request.await.map_err(|e| McpError::service_error(format!(
                    "Request failed for {}: {}",
                    service_name, e
                )))
            }
        }
    }

    /// Get a connection from the pool, creating new ones if needed
    async fn get_pooled_connection(&self, service_name: &str) -> McpResult<Channel> {
        let mut connections = self.connections.write().await;
        
        if let Some(conn_list) = connections.get_mut(service_name) {
            // Clean up idle connections first
            conn_list.retain(|conn| !conn.is_idle(self.pool_config.idle_timeout));

            // Find an available connection
            if let Some(conn) = conn_list.first() {
                conn.mark_used();
                return Ok(conn.channel.clone());
            }
        }

        Err(McpError::connection_failed(format!(
            "No healthy connections available for service: {}",
            service_name
        )))
    }

    /// Create a connection pool for an endpoint
    async fn create_connection_pool(&self, endpoint: Endpoint) -> McpResult<Vec<ManagedConnection>> {
        let mut connections = Vec::new();
        let endpoint_url = endpoint.uri().to_string();

        // Create initial connections (start with 2-3 connections as recommended)
        for i in 0..3 {
            match endpoint.clone()
                .timeout(self.pool_config.connect_timeout)
                .connect()
                .await
            {
                Ok(channel) => {
                    let managed_conn = ManagedConnection::new(
                        channel,
                        format!("{}#{}", endpoint_url, i)
                    );
                    connections.push(managed_conn);
                }
                Err(e) => {
                    log::warn!("Failed to create connection {}: {}", i, e);
                    // Don't fail completely if some connections succeeded
                    if connections.is_empty() && i == 2 {
                        return Err(McpError::connection_failed(format!(
                            "Failed to create any connections to {}: {}",
                            endpoint_url, e
                        )));
                    }
                }
            }
        }

        log::info!("Created {} connections for endpoint: {}", connections.len(), endpoint_url);
        Ok(connections)
    }

    /// Get health status for all services
    pub fn get_all_health_status(&self) -> HashMap<String, HealthResult> {
        self.health_checker.get_all_health_status()
    }

    /// Get circuit breaker status for all services
    pub fn get_all_circuit_breaker_status(&self) -> HashMap<String, CircuitBreakerMetrics> {
        let breakers = self.circuit_breakers.read().unwrap();
        breakers.iter()
            .map(|(name, breaker)| (name.clone(), breaker.get_metrics()))
            .collect()
    }

    /// Get connection pool statistics
    pub async fn get_connection_stats(&self) -> HashMap<String, (usize, Vec<(Duration, Duration, u64)>)> {
        let connections = self.connections.read().await;
        connections.iter()
            .map(|(name, conn_list)| {
                let stats: Vec<(Duration, Duration, u64)> = conn_list.iter()
                    .map(|conn| conn.get_stats())
                    .collect();
                (name.clone(), (conn_list.len(), stats))
            })
            .collect()
    }

    /// Cleanup idle connections across all pools
    pub async fn cleanup_idle_connections(&self) {
        let mut connections = self.connections.write().await;
        let mut total_cleaned = 0;

        for (service_name, conn_list) in connections.iter_mut() {
            let initial_count = conn_list.len();
            conn_list.retain(|conn| !conn.is_idle(self.pool_config.idle_timeout));
            let cleaned = initial_count - conn_list.len();
            
            if cleaned > 0 {
                log::debug!("Cleaned up {} idle connections for service: {}", cleaned, service_name);
                total_cleaned += cleaned;
            }
        }

        if total_cleaned > 0 {
            log::info!("Total idle connections cleaned up: {}", total_cleaned);
        }
    }

    /// Start periodic maintenance tasks
    pub async fn start_maintenance(&self) -> McpResult<()> {
        let connections = Arc::clone(&self.connections);
        let idle_timeout = self.pool_config.idle_timeout;

        // Spawn background task for connection cleanup
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            
            loop {
                interval.tick().await;
                
                let mut connections = connections.write().await;
                for (service_name, conn_list) in connections.iter_mut() {
                    let initial_count = conn_list.len();
                    conn_list.retain(|conn| !conn.is_idle(idle_timeout));
                    let cleaned = initial_count - conn_list.len();
                    
                    if cleaned > 0 {
                        log::debug!("Maintenance: Cleaned {} idle connections for {}", cleaned, service_name);
                    }
                }
            }
        });

        log::info!("Started connection manager maintenance tasks");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_circuit_breaker_state_display() {
        assert_eq!(CircuitBreakerState::Closed.to_string(), "Closed");
        assert_eq!(CircuitBreakerState::Open.to_string(), "Open");
        assert_eq!(CircuitBreakerState::HalfOpen.to_string(), "HalfOpen");
    }

    #[test]
    fn test_circuit_breaker_metrics() {
        let mut metrics = CircuitBreakerMetrics::new();
        assert_eq!(metrics.state, CircuitBreakerState::Closed);
        assert_eq!(metrics.failure_count, 0);
        assert_eq!(metrics.failure_rate(), 0.0);

        // Simulate some requests
        metrics.total_requests = 10;
        metrics.successful_requests = 8;
        assert_eq!(metrics.failure_rate(), 20.0);
    }

    #[tokio::test]
    async fn test_circuit_breaker_creation() {
        let config = CircuitBreakerConfig::default();
        let breaker = CircuitBreaker::new("test_service".to_string(), config);
        
        assert_eq!(breaker.get_state(), CircuitBreakerState::Closed);
        assert_eq!(breaker.service_name, "test_service");
    }

    #[test]
    fn test_managed_connection() {
        use tonic::transport::Channel;
        
        // Mock channel creation (this would normally connect to a real endpoint)
        let channel = Channel::from_static("http://example.com");
        let conn = ManagedConnection::new(channel, "http://example.com".to_string());
        
        assert!(!conn.is_idle(Duration::from_secs(1)));
        
        conn.mark_used();
        let (age, last_used, use_count) = conn.get_stats();
        assert!(age >= Duration::from_nanos(0));
        assert!(last_used >= Duration::from_nanos(0));
        assert_eq!(use_count, 1);
    }

    #[test]
    fn test_connection_pool_config_default() {
        let config = ConnectionPoolConfig::default();
        assert_eq!(config.max_connections, 100);
        assert_eq!(config.idle_timeout, Duration::from_secs(10));
        assert_eq!(config.connect_timeout, Duration::from_secs(5));
    }

    #[test]
    fn test_circuit_breaker_config_default() {
        let config = CircuitBreakerConfig::default();
        assert_eq!(config.failure_threshold, 5);
        assert_eq!(config.success_threshold, 3);
        assert_eq!(config.timeout, Duration::from_secs(30));
    }
}
