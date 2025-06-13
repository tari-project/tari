//! Minotari Wallet MCP Server
//!
//! This application provides Model Context Protocol (MCP) access to Tari wallet functionality,
//! allowing AI agents to interact with wallet operations through a secure, standardized interface.

mod cli;
mod config;
mod server;
mod tools;
mod resources;
mod prompts;

use crate::cli::Cli;
use crate::config::WalletMcpConfig;
use crate::server::WalletMcpServer;
use clap::Parser;
use log::info;
use tari_common::initialize_logging;
use std::process;

#[tokio::main]
async fn main() {
    // Parse command line arguments
    let cli = Cli::parse();
    
    // Initialize logging
    initialize_logging(
        &cli.log_config_path("minotari_mcp_wallet"),
        &cli.get_base_path(),
        include_str!("../../../common/logging/log4rs_sample.yml"),
    ).expect("Failed to initialize logging");

    info!("Starting Minotari Wallet MCP Server v{}", env!("CARGO_PKG_VERSION"));

    // Load configuration
    let config = match WalletMcpConfig::load(&cli) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Failed to load configuration: {}", e);
            process::exit(1);
        }
    };



    // Create and start the MCP server
    let server = match WalletMcpServer::new(config, &cli).await {
        Ok(server) => server,
        Err(e) => {
            eprintln!("Failed to create MCP server: {}", e);
            process::exit(1);
        }
    };

    // Start the server
    if let Err(e) = server.start().await {
        eprintln!("Failed to start MCP server: {}", e);
        process::exit(1);
    }

    // The server runs until interrupted
    info!("MCP server stopped");
}
