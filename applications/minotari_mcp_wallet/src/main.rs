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
use std::process;

#[tokio::main]
async fn main() {
    // Parse command line arguments
    let cli = Cli::parse();
    
    // MCP servers use stdio for protocol communication, so no logging initialization

    // Load configuration
    let config = match WalletMcpConfig::load(&cli) {
        Ok(config) => config,
        Err(e) => {
            // Cannot use eprintln as it interferes with MCP protocol
            process::exit(1);
        }
    };



    // Create and start the MCP server
    let server = match WalletMcpServer::new(config, &cli).await {
        Ok(server) => server,
        Err(e) => {
            process::exit(1);
        }
    };

    // Start the server
    if let Err(_) = server.start().await {
        process::exit(1);
    }
}
