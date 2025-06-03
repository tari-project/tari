// Copyright 2024. The Tari Project
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
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{path::{Path, PathBuf}, fs};

use anyhow::{anyhow, Result};
use bytesize::ByteSize;
use clap::Args;


use serde::{Deserialize, Serialize};
use tabled::{settings::Style, Table, Tabled};
use tari_core::chain_storage::{create_readonly_lmdb_environment, get_all_database_names};
use lmdb_zero::{ReadTransaction, Database, DatabaseOptions};


use crate::{cli::Cli, config::AppConfig};

#[derive(Args, Default)]
pub struct DbStatsArgs {
    /// Tari network directory path (e.g., ~/.tari/mainnet)
    #[arg(long, value_name = "PATH")]
    pub network_dir: Option<PathBuf>,

    /// Output format: table (default), json, csv
    #[arg(long, value_enum, default_value = "table")]
    pub format: OutputFormat,

    /// Sort by: name, size, entries, pages
    #[arg(long, value_enum, default_value = "size")]
    pub sort_by: SortField,

    /// Show only top N databases by size
    #[arg(long, value_name = "N")]
    pub top: Option<usize>,

    /// Include detailed per-database stats
    #[arg(long)]
    pub include_detailed: bool,

    /// Export stats to file
    #[arg(long, value_name = "FILE")]
    pub export: Option<PathBuf>,
}

#[derive(clap::ValueEnum, Clone, Default)]
pub enum OutputFormat {
    #[default]
    Table,
    Json,
    Csv,
}

#[derive(clap::ValueEnum, Clone, Default)]
pub enum SortField {
    Name,
    #[default]
    Size,
    Entries,
    Pages,
}

#[derive(Debug, Clone, Serialize, Deserialize, Tabled)]
pub struct DatabaseStats {
    #[tabled(rename = "Database")]
    pub name: String,
    #[tabled(rename = "Entries")]
    pub entries: usize,
    #[tabled(rename = "Size", display_with = "format_size")]
    pub total_size: usize,
    #[tabled(rename = "Avg Size", display_with = "format_size")]
    pub avg_size: usize,
    #[tabled(rename = "Depth")]
    pub depth: u32,
    #[tabled(rename = "Pages")]
    pub total_pages: usize,
    #[tabled(rename = "Leaf")]
    pub leaf_pages: usize,
    #[tabled(rename = "Branch")]
    pub branch_pages: usize,
    #[tabled(rename = "Overflow")]
    pub overflow_pages: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EnvironmentInfo {
    pub mapsize: usize,
    pub last_pgno: usize,
    pub last_txnid: usize,
    pub maxreaders: u32,
    pub numreaders: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DbStatsOutput {
    pub environment: EnvironmentInfo,
    pub databases: Vec<DatabaseStats>,
    pub summary: DatabaseSummary,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DatabaseSummary {
    pub total_databases: usize,
    pub total_entries: usize,
    pub total_size: usize,
    pub largest_db: String,
    pub avg_entries_per_db: usize,
}

// New structures for multi-database analysis
#[derive(Debug, Serialize, Deserialize)]
enum DatabaseType {
    LMDB,
    SQLite,
}

#[derive(Debug, Serialize, Deserialize, Tabled)]
struct ComponentDatabaseInfo {
    #[tabled(rename = "Component")]
    component: String,
    #[tabled(rename = "Database")]
    name: String,
    #[tabled(rename = "Type")]
    db_type: String,
    #[tabled(rename = "Size", display_with = "format_size_u64")]
    total_size: u64,
    #[tabled(rename = "Path")]
    path: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AllDatabasesOutput {
    pub component_databases: Vec<ComponentDatabaseInfo>,
    pub lmdb_details: Option<DbStatsOutput>, // Detailed LMDB stats for base node if requested
}

fn format_size(size: &usize) -> String {
    ByteSize(*size as u64).to_string()
}

fn format_size_u64(size: &u64) -> String {
    ByteSize(*size).to_string()
}

impl DbStatsArgs {
    pub fn execute(self, cli: &Cli) -> Result<()> {
        let _config = AppConfig::from_cli(cli)?;
        
        // Default to ~/.tari/mainnet if no network dir specified
        let network_dir = self.network_dir.clone().unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            PathBuf::from(home).join(".tari").join("mainnet")
        });

        if !network_dir.exists() {
            return Err(anyhow!("Network directory does not exist: {}", network_dir.display()));
        }

        // For now, just scan and show what databases we find
        let databases = scan_for_databases(&network_dir)?;
        
        // Show summary table
        println!("Found {} databases in {}", databases.len(), network_dir.display());
        println!();
        
        let mut table_data = Table::new(&databases);
        let table = table_data.with(Style::rounded());
        println!("{}", table);
        
        // TODO: Add detailed LMDB analysis if requested
        if self.include_detailed {
            println!("\nDetailed LMDB analysis not yet implemented in network scan mode");
        }

        Ok(())
    }

    fn output_table(&self, stats: &DbStatsOutput) -> Result<()> {
        println!("Environment Information:");
        println!("  Map Size: {}", ByteSize(stats.environment.mapsize as u64));
        println!("  Last Page: {}", stats.environment.last_pgno);
        println!("  Last Transaction ID: {}", stats.environment.last_txnid);
        println!("  Max Readers: {}", stats.environment.maxreaders);
        println!("  Used Readers: {}", stats.environment.numreaders);
        println!();

        let mut databases = stats.databases.clone();
        self.sort_databases(&mut databases);

        if let Some(top) = self.top {
            databases.truncate(top);
        }

        let mut binding = Table::new(&databases);
        let table = binding.with(Style::rounded());
        println!("Database Statistics:");
        println!("{}", table);
        println!();

        println!("Summary:");
        println!("  Total Databases: {}", stats.summary.total_databases);
        println!("  Total Entries: {}", stats.summary.total_entries);
        println!("  Total Size: {}", ByteSize(stats.summary.total_size as u64));
        println!("  Largest Database: {}", stats.summary.largest_db);
        println!("  Average Entries per DB: {}", stats.summary.avg_entries_per_db);

        Ok(())
    }

    fn output_json(&self, stats: &DbStatsOutput) -> Result<()> {
        let json = serde_json::to_string_pretty(stats)?;
        println!("{}", json);
        Ok(())
    }

    fn output_csv(&self, stats: &DbStatsOutput) -> Result<()> {
        let mut wtr = csv::Writer::from_writer(std::io::stdout());

        for db in &stats.databases {
            wtr.serialize(db)?;
        }

        wtr.flush()?;
        Ok(())
    }

    fn export_to_file(&self, stats: &DbStatsOutput, path: &PathBuf) -> Result<()> {
        let content = match path.extension().and_then(|s| s.to_str()) {
            Some("json") => serde_json::to_string_pretty(stats)?,
            Some("csv") => {
                let mut wtr = csv::Writer::from_writer(Vec::new());
                for db in &stats.databases {
                    wtr.serialize(db)?;
                }
                String::from_utf8(wtr.into_inner()?)?
            },
            _ => return Err(anyhow!("Unsupported export format. Use .json or .csv")),
        };

        std::fs::write(path, content)?;
        println!("Stats exported to: {}", path.display());
        Ok(())
    }

    fn sort_databases(&self, databases: &mut [DatabaseStats]) {
        match self.sort_by {
            SortField::Name => databases.sort_by(|a, b| a.name.cmp(&b.name)),
            SortField::Size => databases.sort_by(|a, b| b.total_size.cmp(&a.total_size)),
            SortField::Entries => databases.sort_by(|a, b| b.entries.cmp(&a.entries)),
            SortField::Pages => databases.sort_by(|a, b| b.total_pages.cmp(&a.total_pages)),
        }
    }
}

fn scan_for_databases(network_dir: &Path) -> Result<Vec<ComponentDatabaseInfo>> {
    let mut databases = Vec::new();
    
    // Recursively scan for database files
    scan_directory(network_dir, network_dir, &mut databases)?;
    
    // Sort by component and then by size
    databases.sort_by(|a, b| {
        a.component.cmp(&b.component)
            .then_with(|| b.total_size.cmp(&a.total_size))
    });
    
    Ok(databases)
}

fn scan_directory(dir: &Path, base_dir: &Path, databases: &mut Vec<ComponentDatabaseInfo>) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        
        if path.is_dir() {
            // Check if this is an LMDB database directory (contains data.mdb and lock.mdb)
            if path.join("data.mdb").exists() && path.join("lock.mdb").exists() {
                let component = determine_component(&path, base_dir);
                let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                let size = get_directory_size(&path)?;
                
                databases.push(ComponentDatabaseInfo {
                    component,
                    name,
                    db_type: "LMDB".to_string(),
                    total_size: size,
                    path: path.to_string_lossy().to_string(),
                });
            } else {
                // Recursively scan subdirectories
                scan_directory(&path, base_dir, databases)?;
            }
        } else if path.extension().map_or(false, |ext| ext == "db") {
            // SQLite database file
            let component = determine_component(&path, base_dir);
            let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
            let size = path.metadata()?.len();
            
            databases.push(ComponentDatabaseInfo {
                component,
                name,
                db_type: "SQLite".to_string(),
                total_size: size,
                path: path.to_string_lossy().to_string(),
            });
        }
    }
    
    Ok(())
}

fn determine_component(path: &Path, base_dir: &Path) -> String {
    let relative_path = path.strip_prefix(base_dir).unwrap_or(path);
    let path_str = relative_path.to_string_lossy();
    
    if path_str.contains("base_node") {
        "Base Node".to_string()
    } else if path_str.contains("wallet") {
        "Wallet".to_string()
    } else if path_str.contains("peer_db") {
        "Peer Database".to_string()
    } else if path_str.contains("dht") {
        "DHT".to_string()
    } else {
        "Other".to_string()
    }
}

fn get_directory_size(dir: &Path) -> Result<u64> {
    let mut total_size = 0;
    
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        
        if path.is_file() {
            total_size += path.metadata()?.len();
        } else if path.is_dir() {
            total_size += get_directory_size(&path)?;
        }
    }
    
    Ok(total_size)
}

fn collect_database_stats(db_path: &Path) -> Result<DbStatsOutput> {
    // Open LMDB environment directly in read-only mode (like the original working approach)
    let env = create_readonly_lmdb_environment(db_path)
        .map_err(|e| anyhow!("Failed to open LMDB environment: {}", e))?;

    // Get environment information
    let env_info = env.info().map_err(|e| anyhow!("Failed to get environment info: {}", e))?;
    let env_stat = env.stat().map_err(|e| anyhow!("Failed to get environment stat: {}", e))?;

    let environment = EnvironmentInfo {
        mapsize: env_info.mapsize,
        last_pgno: env_info.last_pgno,
        last_txnid: env_info.last_txnid,
        maxreaders: env_info.maxreaders,
        numreaders: env_info.numreaders,
    };

    // Get individual database statistics by opening them directly
    let mut databases = Vec::new();
    let page_size = env_stat.psize as usize;
    
    // Get the authoritative list of database names from Tari core
    let db_names = get_all_database_names();
    
    // Get statistics for each database
    for db_name in db_names {
        if let Ok(database) = Database::open(&*env, Some(db_name), &DatabaseOptions::defaults()) {
            if let Ok(db_stat) = ReadTransaction::new(env.clone()).and_then(|txn| txn.db_stat(&database)) {
                let total_pages = db_stat.leaf_pages + db_stat.branch_pages + db_stat.overflow_pages;
                let total_size = total_pages * page_size;
                let avg_size = if db_stat.entries > 0 {
                    total_size / db_stat.entries
                } else {
                    0
                };

                databases.push(DatabaseStats {
                    name: db_name.to_string(),
                    entries: db_stat.entries,
                    total_size,
                    avg_size,
                    depth: db_stat.depth,
                    total_pages,
                    leaf_pages: db_stat.leaf_pages,
                    branch_pages: db_stat.branch_pages,
                    overflow_pages: db_stat.overflow_pages,
                });
            }
        }
    }

    // Create summary
    let total_databases = databases.len();
    let total_entries: usize = databases.iter().map(|d| d.entries).sum();
    let total_size: usize = databases.iter().map(|d| d.total_size).sum();
    let largest_db = databases
        .iter()
        .max_by_key(|d| d.total_size)
        .map(|d| d.name.clone())
        .unwrap_or_else(|| "None".to_string());
    let avg_entries_per_db = if total_databases > 0 {
        total_entries / total_databases
    } else {
        0
    };

    let summary = DatabaseSummary {
        total_databases,
        total_entries,
        total_size,
        largest_db,
        avg_entries_per_db,
    };

    Ok(DbStatsOutput {
        environment,
        databases,
        summary,
    })
}



