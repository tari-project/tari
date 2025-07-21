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
//! Process management for auto-launching and supervising Tari applications

use std::{
    net::{Ipv4Addr, TcpListener},
    process::Stdio,
    time::Duration,
};

use tokio::{
    process::{Child, Command},
    sync::{mpsc, RwLock},
};
use uuid::Uuid;

use crate::error::{McpError, McpResult};

/// Process supervisor that manages application lifecycle
pub struct ProcessSupervisor {
    process_id: Uuid,
    process_type: ProcessType,
    executable_path: String,
    args: Vec<String>,
    port: u16,
    child: RwLock<Option<Child>>,
    status_tx: mpsc::UnboundedSender<ProcessStatus>,
    shutdown_rx: RwLock<Option<mpsc::UnboundedReceiver<()>>>,
    max_restart_attempts: u32,
    restart_delay_secs: u64,
}

#[derive(Debug, Clone)]
pub enum ProcessType {
    BaseNode,
    Wallet,
}

#[derive(Debug, Clone)]
pub enum ProcessStatus {
    Starting,
    Running,
    Failed(String),
    Stopped,
    Restarting(u32), // attempt number
}

impl ProcessSupervisor {
    pub fn new(
        process_type: ProcessType,
        executable_path: String,
        args: Vec<String>,
        port: u16,
    ) -> McpResult<(Self, mpsc::UnboundedReceiver<ProcessStatus>)> {
        let (status_tx, status_rx) = mpsc::unbounded_channel();
        let (_shutdown_tx, shutdown_rx) = mpsc::unbounded_channel();

        Ok((
            Self {
                process_id: Uuid::new_v4(),
                process_type,
                executable_path,
                args,
                port,
                child: RwLock::new(None),
                status_tx,
                shutdown_rx: RwLock::new(Some(shutdown_rx)),
                max_restart_attempts: 3,
                restart_delay_secs: 5,
            },
            status_rx,
        ))
    }

    /// Get the unique process supervisor ID
    pub fn id(&self) -> Uuid {
        self.process_id
    }

    /// Start supervising the process
    pub async fn start(&self) -> McpResult<()> {
        let mut restart_attempts = 0;
        let mut shutdown_rx = self
            .shutdown_rx
            .write()
            .await
            .take()
            .ok_or_else(|| McpError::server_error("Supervisor already started"))?;

        log::info!(
            "Starting {} process supervisor {} on port {}",
            match self.process_type {
                ProcessType::BaseNode => "base node",
                ProcessType::Wallet => "wallet",
            },
            self.process_id,
            self.port
        );

        loop {
            // Check if already running via health check
            if self.is_process_healthy().await {
                log::info!("Process already running and healthy on port {}", self.port);
                drop(self.status_tx.send(ProcessStatus::Running));

                // Wait for shutdown signal
                shutdown_rx.recv().await;
                break;
            }

            // Launch the process
            drop(self.status_tx.send(ProcessStatus::Starting));

            match self.launch_process().await {
                Ok(_) => {
                    restart_attempts = 0;
                    drop(self.status_tx.send(ProcessStatus::Running));

                    // Wait for process to exit or shutdown signal
                    tokio::select! {
                        result = self.wait_for_process() => {
                            match result {
                                Ok(_) => {
                                    log::info!("Process exited normally");
                                    break;
                                }
                                Err(e) => {
                                    log::error!("Process failed: {}", e);
                                    drop(self.status_tx.send(ProcessStatus::Failed(e.to_string())));
                                }
                            }
                        }
                        _ = shutdown_rx.recv() => {
                            log::info!("Shutdown signal received");
                            self.stop_process().await?;
                            break;
                        }
                    }
                },
                Err(e) => {
                    log::error!("Failed to launch process: {}", e);
                    drop(self.status_tx.send(ProcessStatus::Failed(e.to_string())));
                },
            }

            // Handle restart logic
            restart_attempts += 1;
            if restart_attempts > self.max_restart_attempts {
                log::error!("Maximum restart attempts reached, giving up");
                drop(
                    self.status_tx
                        .send(ProcessStatus::Failed("Max restarts exceeded".to_string())),
                );
                break;
            }

            drop(self.status_tx.send(ProcessStatus::Restarting(restart_attempts)));
            log::warn!(
                "Restarting process in {} seconds (attempt {})",
                self.restart_delay_secs,
                restart_attempts
            );
            tokio::time::sleep(Duration::from_secs(self.restart_delay_secs)).await;
        }

        drop(self.status_tx.send(ProcessStatus::Stopped));
        Ok(())
    }

    /// Launch the process with configured arguments
    async fn launch_process(&self) -> McpResult<()> {
        let mut cmd = Command::new(&self.executable_path);
        cmd.args(&self.args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        log::debug!("Launching: {} {}", self.executable_path, self.args.join(" "));

        let child = cmd
            .spawn()
            .map_err(|e| McpError::server_error(format!("Failed to spawn process: {}", e)))?;

        *self.child.write().await = Some(child);

        // Wait for process to become healthy
        let mut attempts = 0;
        const MAX_HEALTH_ATTEMPTS: u32 = 30; // 30 seconds

        while attempts < MAX_HEALTH_ATTEMPTS {
            tokio::time::sleep(Duration::from_secs(1)).await;

            if self.is_process_healthy().await {
                log::info!("Process is healthy on port {}", self.port);
                return Ok(());
            }

            // Check if process is still running
            if let Some(child) = self.child.write().await.as_mut() {
                if let Ok(Some(status)) = child.try_wait() {
                    return Err(McpError::server_error(format!(
                        "Process exited during startup with status: {:?}",
                        status
                    )));
                }
            }

            attempts += 1;
        }

        Err(McpError::server_error(
            "Process failed to become healthy within timeout",
        ))
    }

    /// Check if the process is healthy via gRPC connection
    async fn is_process_healthy(&self) -> bool {
        // For now, just check if we can connect to the port
        // In a full implementation, this would use gRPC health checks
        std::net::TcpStream::connect(format!("127.0.0.1:{}", self.port)).is_ok()
    }

    /// Wait for the process to exit
    async fn wait_for_process(&self) -> McpResult<()> {
        if let Some(child) = self.child.write().await.as_mut() {
            let status = child
                .wait()
                .await
                .map_err(|e| McpError::server_error(format!("Failed to wait for process: {}", e)))?;

            if status.success() {
                Ok(())
            } else {
                Err(McpError::server_error(format!(
                    "Process exited with non-zero status: {:?}",
                    status
                )))
            }
        } else {
            Err(McpError::server_error("No process to wait for"))
        }
    }

    /// Stop the supervised process
    async fn stop_process(&self) -> McpResult<()> {
        if let Some(child) = self.child.write().await.as_mut() {
            log::info!("Stopping supervised process");

            // Try graceful shutdown first
            child
                .kill()
                .await
                .map_err(|e| McpError::server_error(format!("Failed to kill process: {}", e)))?;

            // Wait a bit for graceful shutdown
            tokio::time::sleep(Duration::from_secs(2)).await;

            // Force kill if still running
            drop(child.kill().await);
        }

        Ok(())
    }
}

/// Utility functions for process management
pub struct ProcessUtils;

impl ProcessUtils {
    /// Find an available port starting from the base port
    pub fn find_available_port(base_port: u16) -> Option<u16> {
        (base_port..=u16::MAX).find(|&port| TcpListener::bind((Ipv4Addr::LOCALHOST, port)).is_ok())
    }

    /// Get a random available port
    pub fn get_random_port() -> McpResult<u16> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .map_err(|e| McpError::server_error(format!("Failed to bind to random port: {}", e)))?;

        let port = listener
            .local_addr()
            .map_err(|e| McpError::server_error(format!("Failed to get local address: {}", e)))?
            .port();

        Ok(port)
    }

    /// Check if a specific port is available
    pub fn is_port_available(port: u16) -> bool {
        TcpListener::bind((Ipv4Addr::LOCALHOST, port)).is_ok()
    }

    /// Check if a process is running on a specific port
    pub async fn is_service_running(port: u16) -> bool {
        std::net::TcpStream::connect(format!("127.0.0.1:{}", port)).is_ok()
    }

    /// Generate command arguments for launching Tari applications
    pub fn build_node_command(
        executable_path: &str,
        base_path: &str,
        config_path: &str,
        network: Option<&str>,
        grpc_enabled: bool,
        non_interactive: bool,
        custom_args: &[String],
    ) -> (String, Vec<String>) {
        let mut args = vec![
            "--base-path".to_string(),
            base_path.to_string(),
            "--config".to_string(),
            config_path.to_string(),
        ];

        if let Some(network) = network {
            args.push("--network".to_string());
            args.push(network.to_string());
        }

        if grpc_enabled {
            args.push("--grpc-enabled".to_string());
        }

        if non_interactive {
            args.push("--non-interactive-mode".to_string());
        }

        args.extend_from_slice(custom_args);

        (executable_path.to_string(), args)
    }

    /// Generate command arguments for launching wallet
    pub fn build_wallet_command(
        executable_path: &str,
        base_path: &str,
        config_path: &str,
        network: Option<&str>,
        grpc_enabled: bool,
        grpc_address: Option<&str>,
        non_interactive: bool,
        custom_args: &[String],
    ) -> (String, Vec<String>) {
        let mut args = vec![
            "--base-path".to_string(),
            base_path.to_string(),
            "--config".to_string(),
            config_path.to_string(),
        ];

        if let Some(network) = network {
            args.push("--network".to_string());
            args.push(network.to_string());
        }

        if grpc_enabled {
            args.push("--grpc-enabled".to_string());
        }

        if let Some(grpc_address) = grpc_address {
            args.push("--grpc-address".to_string());
            args.push(grpc_address.to_string());
        }

        if non_interactive {
            args.push("--non-interactive-mode".to_string());
        }

        args.extend_from_slice(custom_args);

        (executable_path.to_string(), args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_random_port() {
        let port = ProcessUtils::get_random_port();
        assert!(port.is_ok());
        assert!(port.unwrap() > 0);
    }

    #[test]
    fn test_is_port_available() {
        // Port 0 should never be available for binding
        assert!(!ProcessUtils::is_port_available(0));

        // Find an available port and verify it's available
        let port = ProcessUtils::find_available_port(50000).unwrap();
        assert!(ProcessUtils::is_port_available(port));
    }

    #[tokio::test]
    async fn test_service_running_check() {
        // Check a port that definitely won't have a service
        assert!(!ProcessUtils::is_service_running(65534).await);
    }
}
