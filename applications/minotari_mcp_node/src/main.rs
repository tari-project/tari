//! Minotari Node MCP Server
//!
//! This application provides Model Context Protocol (MCP) access to Tari base node functionality,
//! allowing AI agents to interact with the Tari blockchain through a secure, standardized interface.

mod cli;
mod config;
mod server;
mod tools;
mod resources;
mod prompts;

use crate::cli::Cli;
use crate::config::NodeMcpConfig;
use crate::server::NodeMcpServer;
use clap::Parser;
use log::info;
use minotari_app_utilities::common_cli_args::CommonCliArgs;
use std::process;

#[tokio::main]
async fn main() {
    // Parse command line arguments
    let cli = Cli::parse();
    
    // Initialize logging
    minotari_app_utilities::initialize_logging(
        &cli.common.log_config_path("minotari_mcp_node"),
        &cli.common.get_base_path(),
        include_str!("../../../common/logging/log4rs_sample.yml"),
    ).expect("Failed to initialize logging");

    info!("Starting Minotari Node MCP Server v{}", env!("CARGO_PKG_VERSION"));

    // Load configuration
    let config = match NodeMcpConfig::load(&cli) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Failed to load configuration: {}", e);
            process::exit(1);
        }
    };

    // Check if MCP server is enabled
    if !config.mcp.enabled {
        info!("MCP server is disabled. Use --mcp-enabled to enable it.");
        return;
    }

    // Create and start the MCP server
    let server = match NodeMcpServer::new(config).await {
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
