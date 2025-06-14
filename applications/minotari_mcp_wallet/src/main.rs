//! Minotari Wallet MCP Server
//!
//! This application provides Model Context Protocol (MCP) access to Tari wallet functionality,
//! allowing AI agents to interact with wallet operations through a secure, standardized interface.

mod cli;
mod config;
mod prompts;
mod resources;
mod server;
mod tools;

use std::{fs, io::Write, process};

use clap::Parser;

use crate::{cli::Cli, config::WalletMcpConfig, server::WalletMcpServer};

/// Initialize minimal file-based logging for MCP server
fn init_file_logging(cli: &Cli) {
    let log_dir = cli.get_base_path().join("log");
    let _ = fs::create_dir_all(&log_dir);

    // Set up a simple logger that writes to file only (no stdio)
    let log_file_path = log_dir.join("minotari_mcp_wallet.log");

    struct FileLogger {
        path: std::path::PathBuf,
    }

    impl log::Log for FileLogger {
        fn enabled(&self, _metadata: &log::Metadata) -> bool {
            true
        }

        fn log(&self, record: &log::Record) {
            if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(&self.path) {
                let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S%.3f");
                let _ = writeln!(file, "{} {} [{}] - {}", timestamp, record.level(), record.target(), record.args());
            }
        }

        fn flush(&self) {}
    }

    let logger = FileLogger { path: log_file_path };
    let _ = log::set_boxed_logger(Box::new(logger));
    log::set_max_level(log::LevelFilter::Info);
}

#[tokio::main]
async fn main() {
    // Parse command line arguments
    let cli = Cli::parse();

    // Initialize file-based logging (no stdout/stderr output)
    init_file_logging(&cli);

    // Load configuration
    let config = match WalletMcpConfig::load(&cli) {
        Ok(config) => config,
        Err(_) => {
            // Cannot use eprintln as it interferes with MCP protocol
            process::exit(1);
        },
    };

    // Create and start the MCP server
    let server = match WalletMcpServer::new(config, &cli).await {
        Ok(server) => server,
        Err(_) => {
            process::exit(1);
        },
    };

    // Set up signal handling for graceful shutdown
    let server_for_signal = server;

    // Handle Ctrl+C gracefully
    tokio::select! {
        result = server_for_signal.start() => {
            if result.is_err() {
                process::exit(1);
            }
        }
        _ = tokio::signal::ctrl_c() => {
            log::info!("Received shutdown signal, stopping server...");

            // Set a timeout for shutdown to prevent hanging
            let shutdown_result = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                server_for_signal.stop()
            ).await;

            match shutdown_result {
                Ok(Ok(_)) => {
                    log::info!("Server stopped successfully");
                }
                Ok(Err(e)) => {
                    log::error!("Error during shutdown: {}", e);
                }
                Err(_) => {
                    log::warn!("Shutdown timed out after 5 seconds, forcing exit");
                }
            }

            // Force exit to ensure clean termination
            process::exit(0);
        }
    }
}
