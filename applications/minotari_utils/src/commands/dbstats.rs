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

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Result};
use bytesize::ByteSize;
use clap::Args;
use csv;
use lmdb_zero::{Database, DatabaseOptions, ReadTransaction};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tabled::{settings::Style, Table, Tabled};
use tari_core::chain_storage::{create_readonly_lmdb_environment, get_all_database_names};

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
    Lmdb,
    SQLite,
}

#[derive(Debug, Serialize, Deserialize, Tabled)]
pub struct ComponentDatabaseInfo {
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

#[derive(Debug, Clone, Serialize, Deserialize, Tabled)]
pub struct SqliteTableInfo {
    #[tabled(rename = "Table")]
    name: String,
    #[tabled(rename = "Rows", display_with = "format_u64")]
    row_count: u64,
    #[tabled(rename = "Size", display_with = "format_size_u64")]
    size_bytes: u64,
    #[tabled(rename = "Avg Row Size", display_with = "format_size_u64")]
    avg_row_size: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SqliteStatsOutput {
    pub file_size: u64,
    pub page_size: u64,
    pub page_count: u64,
    pub freelist_count: u64,
    pub tables: Vec<SqliteTableInfo>,
    pub summary: SqliteSummary,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SqliteSummary {
    pub total_tables: usize,
    pub total_rows: u64,
    pub total_size: u64,
    pub largest_table: String,
    pub avg_rows_per_table: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AllDatabasesOutput {
    pub component_databases: Vec<ComponentDatabaseInfo>,
    pub lmdb_details: Option<DbStatsOutput>, // Detailed LMDB stats for base node if requested
    pub sqlite_details: Vec<(String, SqliteStatsOutput)>, // Path and detailed SQLite stats if requested
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn format_size(size: &usize) -> String {
    ByteSize(*size as u64).to_string()
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn format_size_u64(size: &u64) -> String {
    ByteSize(*size).to_string()
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn format_u64(value: &u64) -> String {
    value.to_string()
}

impl DbStatsArgs {
    #[allow(clippy::too_many_lines)]
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

        // Scan for all databases
        let databases = scan_for_databases(&network_dir)?;

        // Create output structure
        let all_stats = AllDatabasesOutput {
            component_databases: databases,
            lmdb_details: None,         // Will be populated below if include_detailed
            sqlite_details: Vec::new(), // Will be populated below if include_detailed
        };

        // Output in requested format
        match self.format {
            OutputFormat::Table => {
                println!(
                    "Found {} databases in {}",
                    all_stats.component_databases.len(),
                    network_dir.display()
                );
                println!();

                let mut table_data = Table::new(&all_stats.component_databases);
                let table = table_data.with(Style::rounded());
                println!("{}", table);
            },
            OutputFormat::Json => {
                let json = serde_json::to_string_pretty(&all_stats)?;
                println!("{}", json);
            },
            OutputFormat::Csv => {
                let mut wtr = csv::Writer::from_writer(std::io::stdout());
                for db in &all_stats.component_databases {
                    wtr.serialize(db)?;
                }
                wtr.flush()?;
            },
        }

        // Export to file if requested
        if let Some(export_path) = &self.export {
            let content = match export_path.extension().and_then(|s| s.to_str()) {
                Some("json") => serde_json::to_string_pretty(&all_stats)?,
                Some("csv") => {
                    let mut buffer = Vec::new();
                    {
                        let mut wtr = csv::Writer::from_writer(&mut buffer);
                        for db in &all_stats.component_databases {
                            wtr.serialize(db)?;
                        }
                        wtr.flush()?;
                    }
                    String::from_utf8(buffer)?
                },
                _ => {
                    // Default to JSON for unknown extensions
                    serde_json::to_string_pretty(&all_stats)?
                },
            };
            std::fs::write(export_path, content)?;
            println!("Exported statistics to {}", export_path.display());
        }

        // Add detailed LMDB analysis if requested
        if self.include_detailed {
            if let Some(base_node_lmdb) = find_base_node_lmdb_database(&all_stats.component_databases) {
                println!("\n=== Detailed Base Node LMDB Analysis ===");
                match collect_database_stats(Path::new(&base_node_lmdb.path)) {
                    Ok(lmdb_stats) => {
                        println!("\nEnvironment Information:");
                        println!("  Map Size: {}", ByteSize(lmdb_stats.environment.mapsize as u64));
                        println!("  Last Page: {}", lmdb_stats.environment.last_pgno);
                        println!("  Last Transaction ID: {}", lmdb_stats.environment.last_txnid);
                        println!("  Max Readers: {}", lmdb_stats.environment.maxreaders);
                        println!("  Used Readers: {}", lmdb_stats.environment.numreaders);

                        println!("\nDatabase Statistics:");
                        let mut lmdb_databases = lmdb_stats.databases.clone();
                        self.sort_lmdb_databases(&mut lmdb_databases);

                        if let Some(top) = self.top {
                            lmdb_databases.truncate(top);
                        }

                        let mut table_data = Table::new(&lmdb_databases);
                        let table = table_data.with(Style::rounded());
                        println!("{}", table);

                        println!("\nSummary:");
                        println!("  Total Databases: {}", lmdb_stats.summary.total_databases);
                        println!("  Total Entries: {}", lmdb_stats.summary.total_entries);
                        println!("  Total Size: {}", ByteSize(lmdb_stats.summary.total_size as u64));
                        println!("  Largest Database: {}", lmdb_stats.summary.largest_db);
                        println!("  Average Entries per DB: {}", lmdb_stats.summary.avg_entries_per_db);
                    },
                    Err(e) => {
                        println!("Failed to analyze base node LMDB database: {}", e);
                    },
                }
            } else {
                println!("\nNo base node LMDB database found for detailed analysis");
            }

            // Add detailed SQLite analysis for all SQLite databases
            let sqlite_databases: Vec<_> = all_stats
                .component_databases
                .iter()
                .filter(|db| db.db_type == "SQLite")
                .collect();

            if !sqlite_databases.is_empty() {
                println!("\n=== Detailed SQLite Database Analysis ===");
                for sqlite_db in sqlite_databases {
                    println!("\n--- {} ({}) ---", sqlite_db.name, sqlite_db.component);
                    match collect_sqlite_stats(Path::new(&sqlite_db.path)) {
                        Ok(sqlite_stats) => {
                            println!("\nDatabase Information:");
                            println!("  File Size: {}", ByteSize(sqlite_stats.file_size));
                            println!("  Page Size: {}", ByteSize(sqlite_stats.page_size));
                            println!("  Page Count: {}", sqlite_stats.page_count);
                            println!("  Free Pages: {}", sqlite_stats.freelist_count);

                            if !sqlite_stats.tables.is_empty() {
                                println!("\nTable Statistics:");
                                let mut sqlite_tables = sqlite_stats.tables.clone();

                                if let Some(top) = self.top {
                                    sqlite_tables.truncate(top);
                                }

                                let mut table_data = Table::new(&sqlite_tables);
                                let table = table_data.with(Style::rounded());
                                println!("{}", table);
                            }

                            println!("\nSummary:");
                            println!("  Total Tables: {}", sqlite_stats.summary.total_tables);
                            println!("  Total Rows: {}", sqlite_stats.summary.total_rows);
                            println!("  Total Size: {}", ByteSize(sqlite_stats.summary.total_size));
                            println!("  Largest Table: {}", sqlite_stats.summary.largest_table);
                            println!("  Average Rows per Table: {}", sqlite_stats.summary.avg_rows_per_table);
                        },
                        Err(e) => {
                            println!("Failed to analyze SQLite database {}: {}", sqlite_db.name, e);
                        },
                    }
                }
            }
        }

        Ok(())
    }

    fn sort_lmdb_databases(&self, databases: &mut [DatabaseStats]) {
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
        a.component
            .cmp(&b.component)
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
            // Check if this is an LMDB database directory (contains data.mdb)
            if path.join("data.mdb").exists() {
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
        } else if path.extension().is_some_and(|ext| ext == "db") {
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
        } else {
            // clippy
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
        } else { // clippy
        }
    }

    Ok(total_size)
}

fn find_base_node_lmdb_database(databases: &[ComponentDatabaseInfo]) -> Option<&ComponentDatabaseInfo> {
    databases
        .iter()
        .find(|db| db.component == "Base Node" && db.db_type == "LMDB" && db.path.contains("data/base_node/db"))
}

fn collect_database_stats(db_path: &Path) -> Result<DbStatsOutput> {
    // Open LMDB environment directly in read-only mode (like the original working approach)
    let env =
        create_readonly_lmdb_environment(db_path).map_err(|e| anyhow!("Failed to open LMDB environment: {}", e))?;

    // Get environment information
    let env_info = env
        .info()
        .map_err(|e| anyhow!("Failed to get environment info: {}", e))?;
    let env_stat = env
        .stat()
        .map_err(|e| anyhow!("Failed to get environment stat: {}", e))?;

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

fn collect_sqlite_stats(db_path: &Path) -> Result<SqliteStatsOutput> {
    let conn = Connection::open(db_path)?;

    // Get database file information
    let file_size = db_path.metadata()?.len();

    // Get database pragma information
    let page_size: u64 = conn.pragma_query_value(None, "page_size", |row| row.get(0))?;
    let page_count: u64 = conn.pragma_query_value(None, "page_count", |row| row.get(0))?;
    let freelist_count: u64 = conn.pragma_query_value(None, "freelist_count", |row| row.get(0))?;

    // Get list of tables
    let mut stmt = conn.prepare("SELECT name FROM sqlite_master WHERE type='table'")?;
    let table_names: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;

    let mut tables = Vec::new();
    let mut total_rows = 0;

    for table_name in table_names {
        // Skip SQLite internal tables
        if table_name.starts_with("sqlite_") {
            continue;
        }

        // Get row count
        let row_count: u64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM {}", table_name), [], |row| row.get(0))
            .unwrap_or(0);

        // Estimate table size using database_list and page info
        // This is an approximation since SQLite doesn't provide direct table size info
        let table_size_estimate = if row_count > 0 {
            // Use rough estimation: file_size * (table_rows / total_db_rows_estimate)
            // This is not perfect but gives a reasonable approximation
            let size_per_row = if row_count > 0 {
                file_size / page_count.max(1)
            } else {
                0
            };
            row_count * size_per_row
        } else {
            0
        };

        let avg_row_size = if row_count > 0 {
            table_size_estimate / row_count
        } else {
            0
        };

        tables.push(SqliteTableInfo {
            name: table_name,
            row_count,
            size_bytes: table_size_estimate,
            avg_row_size,
        });

        total_rows += row_count;
    }

    // Sort tables by row count (descending)
    tables.sort_by(|a, b| b.row_count.cmp(&a.row_count));

    let largest_table = tables
        .first()
        .map(|t| t.name.clone())
        .unwrap_or_else(|| "N/A".to_string());

    let avg_rows_per_table = if tables.is_empty() {
        0
    } else {
        total_rows / tables.len() as u64
    };

    let summary = SqliteSummary {
        total_tables: tables.len(),
        total_rows,
        total_size: file_size,
        largest_table,
        avg_rows_per_table,
    };

    Ok(SqliteStatsOutput {
        file_size,
        page_size,
        page_count,
        freelist_count,
        tables,
        summary,
    })
}
