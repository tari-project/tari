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
use std::process;
use std::fs;
use std::io::Write;

/// Initialize minimal file-based logging for MCP server
fn init_file_logging(cli: &Cli) {
    let log_dir = cli.get_base_path().join("log");
    let _ = fs::create_dir_all(&log_dir);
    
    // Set up a simple logger that writes to file only (no stdio)
    let log_file_path = log_dir.join("minotari_mcp_node.log");
    
    struct FileLogger {
        path: std::path::PathBuf,
    }
    
    impl log::Log for FileLogger {
        fn enabled(&self, _metadata: &log::Metadata) -> bool {
            true
        }
        
        fn log(&self, record: &log::Record) {
            if let Ok(mut file) = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path) {
                let _ = writeln!(file, "{} - {}", 
                    record.level(), 
                    record.args());
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
    let config = match NodeMcpConfig::load(&cli) {
        Ok(config) => config,
        Err(_) => {
            // Cannot use eprintln as it interferes with MCP protocol
            process::exit(1);
        }
    };



    // Create and start the MCP server
    let server = match NodeMcpServer::new(config, &cli).await {
        Ok(server) => server,
        Err(_) => {
            process::exit(1);
        }
    };

    // Start the server
    if server.start().await.is_err() {
        process::exit(1);
    }
}
