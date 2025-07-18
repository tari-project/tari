//! Advanced process launcher for Tari applications
//!
//! Provides sophisticated process launching with CLI integration, health monitoring,
//! and comprehensive error handling. Supports both node and wallet launching with
//! proper argument passthrough and startup coordination.

use std::{collections::HashMap, path::PathBuf, process::Stdio, sync::Arc, time::Duration};

use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
    sync::{mpsc, RwLock},
};
use uuid::Uuid;

use crate::{
    error::{McpError, McpResult},
    executable_finder::TariExecutables,
    health_monitor::HealthMonitor,
};

/// Process launch configuration
#[derive(Debug, Clone)]
pub struct LaunchConfig {
    /// Executable path (will be auto-discovered if None)
    pub executable_path: Option<PathBuf>,
    /// Command line arguments
    pub args: Vec<String>,
    /// Environment variables
    pub env_vars: HashMap<String, String>,
    /// Working directory
    pub working_dir: Option<PathBuf>,
    /// Startup timeout
    pub startup_timeout: Duration,
    /// Health check configuration
    pub health_check_config: HealthCheckConfig,
}

/// Health check configuration for launched processes
#[derive(Debug, Clone)]
pub struct HealthCheckConfig {
    /// gRPC endpoint for health checks
    pub grpc_endpoint: String,
    /// Initial delay before health checks start
    pub initial_delay: Duration,
    /// Interval between health checks
    pub check_interval: Duration,
    /// Maximum time to wait for service to become healthy
    pub max_wait_time: Duration,
}

/// Status of a launched process
#[derive(Debug, Clone)]
pub enum ProcessLaunchStatus {
    /// Process is starting up
    Starting,
    /// Process is running and healthy
    Running,
    /// Process failed to start
    Failed(String),
    /// Process is stopping
    Stopping,
    /// Process has stopped
    Stopped,
}

/// Result of a process launch operation
#[derive(Debug)]
pub struct LaunchResult {
    pub process_id: Uuid,
    pub executable_path: PathBuf,
    pub pid: Option<u32>,
    pub grpc_endpoint: String,
    pub status: ProcessLaunchStatus,
}

/// Advanced process launcher
pub struct ProcessLauncher {
    launch_id: Uuid,
    config: LaunchConfig,
    process: RwLock<Option<Child>>,
    health_monitor: Option<HealthMonitor>,
    status_tx: mpsc::UnboundedSender<ProcessLaunchStatus>,
    output_buffer: Arc<RwLock<Vec<String>>>,
}

impl ProcessLauncher {
    /// Create a new process launcher
    pub fn new(config: LaunchConfig) -> (Self, mpsc::UnboundedReceiver<ProcessLaunchStatus>) {
        let (status_tx, status_rx) = mpsc::unbounded_channel();

        let health_monitor = Some(
            HealthMonitor::new(
                "launched_process".to_string(),
                config.health_check_config.grpc_endpoint.clone(),
            )
            .with_timeout(Duration::from_secs(10)),
        );

        (
            Self {
                launch_id: Uuid::new_v4(),
                config,
                process: RwLock::new(None),
                health_monitor,
                status_tx,
                output_buffer: Arc::new(RwLock::new(Vec::new())),
            },
            status_rx,
        )
    }

    /// Launch the process with comprehensive monitoring
    pub async fn launch(&self) -> McpResult<LaunchResult> {
        drop(self.status_tx.send(ProcessLaunchStatus::Starting));

        // Discover executable if not provided
        let executable_path = if let Some(ref path) = self.config.executable_path {
            path.clone()
        } else {
            self.discover_executable().await?
        };

        log::info!(
            "Launching process: {} with args: {:?}",
            executable_path.display(),
            self.config.args
        );
        log::debug!("Process launch config: {:?}", self.config);

        // Prepare command
        let mut command = Command::new(&executable_path);
        command
            .args(&self.config.args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        // Set environment variables
        for (key, value) in &self.config.env_vars {
            command.env(key, value);
        }

        // Set working directory
        if let Some(ref working_dir) = self.config.working_dir {
            command.current_dir(working_dir);
        }

        // Launch the process
        log::debug!("About to spawn process: {:?}", command);
        let mut child = command.spawn().map_err(|e| {
            log::error!(
                "Failed to spawn process: {} (executable: {})",
                e,
                executable_path.display()
            );
            McpError::server_error(format!("Failed to launch process: {}", e))
        })?;

        let pid = child.id();

        // Capture stdout and stderr
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        // Store the process
        *self.process.write().await = Some(child);

        log::info!("Process launched with PID: {:?}", pid);

        // Start output capture tasks
        if let Some(stdout) = stdout {
            let output_buffer = self.output_buffer.clone();
            tokio::spawn(async move {
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    log::debug!("STDOUT: {}", line);
                    output_buffer.write().await.push(format!("STDOUT: {}", line));
                }
                log::debug!("STDOUT capture ended");
            });
        }

        if let Some(stderr) = stderr {
            let output_buffer = self.output_buffer.clone();
            tokio::spawn(async move {
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    log::debug!("STDERR: {}", line);
                    output_buffer.write().await.push(format!("STDERR: {}", line));
                }
                log::debug!("STDERR capture ended");
            });
        }

        // Check if process crashed immediately after launch
        tokio::time::sleep(Duration::from_millis(100)).await;
        if !self.is_running().await {
            // Process crashed immediately, get output
            let output = self.get_captured_output().await;
            let output_summary = if output.is_empty() {
                "No output captured".to_string()
            } else {
                output.join("\n")
            };

            log::error!("Process crashed immediately after launch. PID was: {:?}", pid);
            log::error!("Process output: {}", output_summary);

            let error_msg = format!("Process crashed immediately after launch. Output:\n{}", output_summary);
            drop(self.status_tx.send(ProcessLaunchStatus::Failed(error_msg.clone())));
            return Err(McpError::server_error(error_msg));
        }

        // Wait for process to become healthy
        let health_result = self.wait_for_health().await;

        let status = match health_result {
            Ok(_) => {
                log::info!("Process is healthy and ready");
                ProcessLaunchStatus::Running
            },
            Err(e) => {
                log::error!("Process failed health checks: {}", e);

                // Get captured output for error reporting
                let output = self.get_captured_output().await;
                let output_summary = if output.is_empty() {
                    "No output captured".to_string()
                } else {
                    output.join("\n")
                };

                let error_msg = format!("Health check failed: {}\nProcess output:\n{}", e, output_summary);
                drop(self.status_tx.send(ProcessLaunchStatus::Failed(error_msg.clone())));
                return Err(McpError::server_error(error_msg));
            },
        };

        let result = LaunchResult {
            process_id: self.launch_id,
            executable_path,
            pid,
            grpc_endpoint: self.config.health_check_config.grpc_endpoint.clone(),
            status,
        };

        drop(self.status_tx.send(ProcessLaunchStatus::Running));
        Ok(result)
    }

    /// Wait for process to become healthy
    async fn wait_for_health(&self) -> McpResult<()> {
        if let Some(ref health_monitor) = self.health_monitor {
            // Initial delay before starting health checks
            tokio::time::sleep(self.config.health_check_config.initial_delay).await;

            log::debug!("Starting health checks for launched process");

            // Wait for service to become healthy
            health_monitor
                .wait_for_healthy(self.config.health_check_config.max_wait_time)
                .await?;
        }

        Ok(())
    }

    /// Discover executable based on launch configuration
    async fn discover_executable(&self) -> McpResult<PathBuf> {
        // Try to determine executable type from arguments
        if self.config.args.iter().any(|arg| arg.contains("wallet")) {
            TariExecutables::find_wallet()
        } else if self.config.args.iter().any(|arg| arg.contains("node")) {
            TariExecutables::find_node()
        } else {
            // Default to node if we can't determine
            TariExecutables::find_node()
        }
    }

    /// Stop the launched process gracefully
    pub async fn stop(&self) -> McpResult<()> {
        drop(self.status_tx.send(ProcessLaunchStatus::Stopping));

        if let Some(child) = self.process.write().await.as_mut() {
            log::info!("Stopping launched process with PID: {:?}", child.id());

            // Try graceful shutdown first (SIGTERM)
            match child.kill().await {
                Ok(_) => log::info!("Sent SIGTERM to process"),
                Err(e) => log::warn!("Failed to send SIGTERM: {}", e),
            }

            // Wait for graceful shutdown with periodic checks
            let mut attempts = 0;
            while attempts < 10 {
                // 5 seconds total
                match child.try_wait() {
                    Ok(Some(status)) => {
                        log::info!("Process exited with status: {:?}", status);
                        break;
                    },
                    Ok(None) => {
                        // Still running, continue waiting
                        tokio::time::sleep(Duration::from_millis(500)).await;
                        attempts += 1;
                    },
                    Err(e) => {
                        log::warn!("Error checking process status: {}", e);
                        break;
                    },
                }
            }

            // Force kill if still running
            match child.try_wait() {
                Ok(None) => {
                    log::warn!("Process did not exit gracefully, force killing");
                    drop(child.kill().await);

                    // Wait a bit more for force kill to take effect
                    tokio::time::sleep(Duration::from_secs(1)).await;
                },
                Ok(Some(_)) => {
                    // Process has already exited
                },
                Err(e) => {
                    log::warn!("Error checking process status for force kill: {}", e);
                },
            }

            log::info!("Process stopped");
        }

        drop(self.status_tx.send(ProcessLaunchStatus::Stopped));
        Ok(())
    }

    /// Check if the process is still running
    pub async fn is_running(&self) -> bool {
        if let Some(child) = self.process.write().await.as_mut() {
            child.try_wait().map(|status| status.is_none()).unwrap_or(false)
        } else {
            false
        }
    }

    /// Get the process ID
    pub async fn get_pid(&self) -> Option<u32> {
        if let Some(child) = self.process.read().await.as_ref() {
            child.id()
        } else {
            None
        }
    }

    /// Get captured output from the process
    pub async fn get_captured_output(&self) -> Vec<String> {
        self.output_buffer.read().await.clone()
    }
}

/// Builder for launch configurations
pub struct LaunchConfigBuilder {
    executable_path: Option<PathBuf>,
    args: Vec<String>,
    env_vars: HashMap<String, String>,
    working_dir: Option<PathBuf>,
    startup_timeout: Duration,
    health_check_config: Option<HealthCheckConfig>,
}

impl LaunchConfigBuilder {
    pub fn new() -> Self {
        Self {
            executable_path: None,
            args: Vec::new(),
            env_vars: HashMap::new(),
            working_dir: None,
            startup_timeout: Duration::from_secs(60),
            health_check_config: None,
        }
    }

    pub fn with_executable(mut self, path: PathBuf) -> Self {
        self.executable_path = Some(path);
        self
    }

    pub fn with_args(mut self, args: Vec<String>) -> Self {
        self.args = args;
        self
    }

    pub fn with_env_var(mut self, key: String, value: String) -> Self {
        self.env_vars.insert(key, value);
        self
    }

    pub fn with_working_dir(mut self, dir: PathBuf) -> Self {
        self.working_dir = Some(dir);
        self
    }

    pub fn with_startup_timeout(mut self, timeout: Duration) -> Self {
        self.startup_timeout = timeout;
        self
    }

    pub fn with_health_check(mut self, config: HealthCheckConfig) -> Self {
        self.health_check_config = Some(config);
        self
    }

    pub fn build(self) -> McpResult<LaunchConfig> {
        let health_check_config = self
            .health_check_config
            .ok_or_else(|| McpError::config_error("Health check configuration is required"))?;

        Ok(LaunchConfig {
            executable_path: self.executable_path,
            args: self.args,
            env_vars: self.env_vars,
            working_dir: self.working_dir,
            startup_timeout: self.startup_timeout,
            health_check_config,
        })
    }
}

/// Convenience functions for launching common Tari services
pub struct TariProcessLauncher;

impl TariProcessLauncher {
    /// Convert IP:PORT format to multiaddr format
    fn convert_to_multiaddr(address: &str) -> String {
        if address.starts_with("/ip4/") {
            // Already in multiaddr format
            address.to_string()
        } else if let Some((ip, port)) = address.split_once(':') {
            // Convert IP:PORT to /ip4/IP/tcp/PORT
            format!("/ip4/{}/tcp/{}", ip, port)
        } else {
            // Assume it's just a port, use localhost
            format!("/ip4/127.0.0.1/tcp/{}", address)
        }
    }

    /// Launch a base node with the given configuration
    pub async fn launch_node(
        base_path: String,
        config_path: String,
        network: Option<String>,
        grpc_address: String,
        additional_args: Vec<String>,
    ) -> McpResult<(ProcessLauncher, mpsc::UnboundedReceiver<ProcessLaunchStatus>)> {
        // Convert IP:PORT format to multiaddr format
        let multiaddr_format = Self::convert_to_multiaddr(&grpc_address);

        let mut args = vec![
            "--base-path".to_string(),
            base_path,
            "--config".to_string(),
            config_path,
            "-p".to_string(),
            "base_node.grpc_enabled=true".to_string(),
            "-p".to_string(),
            format!("base_node.grpc_address={}", multiaddr_format),
            "-p".to_string(),
            "base_node.grpc_server_allow_methods=get_version,get_tip_info,get_sync_info,get_network_status,get_peers,\
             get_header_by_hash,get_blocks,get_network_difficulty,get_tokens_in_circulation,get_mempool_stats,\
             get_mempool_transactions,get_new_block_template,get_new_block_template_with_coinbases,submit_transaction,\
             submit_block"
                .to_string(),
            "--non-interactive-mode".to_string(),
        ];

        if let Some(network) = network {
            args.push("--network".to_string());
            args.push(network);
        }

        args.extend(additional_args);

        let health_config = HealthCheckConfig {
            grpc_endpoint: format!("http://{}", grpc_address),
            initial_delay: Duration::from_secs(5),
            check_interval: Duration::from_secs(2),
            max_wait_time: Duration::from_secs(90),
        };

        let config = LaunchConfigBuilder::new()
            .with_executable(TariExecutables::find_node()?)
            .with_args(args)
            .with_startup_timeout(Duration::from_secs(120))
            .with_health_check(health_config)
            .build()?;

        Ok(ProcessLauncher::new(config))
    }

    /// Launch a wallet with the given configuration
    pub async fn launch_wallet(
        base_path: String,
        config_path: String,
        network: Option<String>,
        grpc_address: String,
        additional_args: Vec<String>,
    ) -> McpResult<(ProcessLauncher, mpsc::UnboundedReceiver<ProcessLaunchStatus>)> {
        // Convert IP:PORT format to multiaddr format
        let multiaddr_format = Self::convert_to_multiaddr(&grpc_address);

        let mut args = vec![
            "--base-path".to_string(),
            base_path,
            "--config".to_string(),
            config_path,
            "--grpc-enabled".to_string(),
            "--grpc-address".to_string(),
            multiaddr_format,
            "--non-interactive-mode".to_string(),
        ];

        if let Some(network) = network {
            args.push("--network".to_string());
            args.push(network);
        }

        args.extend(additional_args);

        let health_config = HealthCheckConfig {
            grpc_endpoint: format!("http://{}", grpc_address),
            initial_delay: Duration::from_secs(10), // Wallet takes longer to start
            check_interval: Duration::from_secs(3),
            max_wait_time: Duration::from_secs(180), // Wallet can take up to 3 minutes
        };

        let config = LaunchConfigBuilder::new()
            .with_executable(TariExecutables::find_wallet()?)
            .with_args(args)
            .with_startup_timeout(Duration::from_secs(200))
            .with_health_check(health_config)
            .build()?;

        Ok(ProcessLauncher::new(config))
    }
}

impl Default for LaunchConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_launch_config_builder() {
        let config = LaunchConfigBuilder::new()
            .with_args(vec!["--test".to_string()])
            .with_env_var("TEST_VAR".to_string(), "test_value".to_string())
            .with_startup_timeout(Duration::from_secs(30))
            .with_health_check(HealthCheckConfig {
                grpc_endpoint: "http://127.0.0.1:18142".to_string(),
                initial_delay: Duration::from_secs(1),
                check_interval: Duration::from_secs(1),
                max_wait_time: Duration::from_secs(10),
            })
            .build()
            .unwrap();

        assert_eq!(config.args, vec!["--test".to_string()]);
        assert_eq!(config.env_vars.get("TEST_VAR"), Some(&"test_value".to_string()));
        assert_eq!(config.startup_timeout, Duration::from_secs(30));
    }

    #[test]
    fn test_health_check_config() {
        let config = HealthCheckConfig {
            grpc_endpoint: "http://127.0.0.1:18142".to_string(),
            initial_delay: Duration::from_secs(5),
            check_interval: Duration::from_secs(2),
            max_wait_time: Duration::from_secs(60),
        };

        assert_eq!(config.grpc_endpoint, "http://127.0.0.1:18142");
        assert_eq!(config.initial_delay, Duration::from_secs(5));
    }
}
