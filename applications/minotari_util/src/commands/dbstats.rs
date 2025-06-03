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

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use bytesize::ByteSize;
use clap::Args;
use lmdb_zero::open;
use log::debug;
use serde::{Deserialize, Serialize};
use tabled::{settings::Style, Table, Tabled};

use tari_storage::lmdb_store::{LMDBBuilder, LMDBConfig, LMDBStore};

use crate::{cli::Cli, config::AppConfig};

/// Create a read-only LMDB store connection that doesn't require exclusive lock
fn create_readonly_lmdb_store<P: AsRef<Path>>(
    path: P,
    config: LMDBConfig,
) -> Result<LMDBStore, anyhow::Error> {

    
    debug!("Opening LMDB store in read-only mode at {:?}", path.as_ref());
    
    if !path.as_ref().exists() {
        return Err(anyhow!("Database path does not exist: {}", path.as_ref().display()));
    }

    // Create LMDB store without acquiring exclusive file lock - just open existing databases
    let lmdb_store = LMDBBuilder::new()
        .set_path(path)
        .set_env_flags(open::NOLOCK | open::RDONLY) // Read-only mode without locks
        .set_env_config(config)
        .set_max_number_of_databases(40)
        .build()
        .map_err(|e| anyhow!("Failed to open LMDB store: {}", e))?;

    Ok(lmdb_store)
}

#[derive(Args, Default)]
pub struct DbStatsArgs {
    /// Custom LMDB path (overrides default: data/base_node/db/)
    #[arg(long, value_name = "PATH")]
    pub db_path: Option<PathBuf>,

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

fn format_size(size: &usize) -> String {
    ByteSize(*size as u64).to_string()
}

impl DbStatsArgs {
    pub fn execute(self, cli: &Cli) -> Result<()> {
        let config = AppConfig::from_cli(cli)?;
        let db_path = self.db_path.clone().unwrap_or(config.db_path);

        if !db_path.exists() {
            return Err(anyhow!("Database path does not exist: {}", db_path.display()));
        }

        let stats = collect_database_stats(&db_path)?;

        match self.format {
            OutputFormat::Table => self.output_table(&stats)?,
            OutputFormat::Json => self.output_json(&stats)?,
            OutputFormat::Csv => self.output_csv(&stats)?,
        }

        if let Some(export_path) = &self.export {
            self.export_to_file(&stats, export_path)?;
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

fn collect_database_stats(db_path: &Path) -> Result<DbStatsOutput> {
    
    // Open LMDB store in read-only mode without exclusive lock
    let lmdb_store = create_readonly_lmdb_store(db_path, LMDBConfig::default())
        .map_err(|e| anyhow!("Failed to open LMDB store: {}", e))?;

    // Get environment information directly from LMDB
    let env = lmdb_store.env();
    let env_info = env.info().map_err(|e| anyhow!("Failed to get environment info: {}", e))?;
    let env_stat = env.stat().map_err(|e| anyhow!("Failed to get environment stat: {}", e))?;

    let environment = EnvironmentInfo {
        mapsize: env_info.mapsize,
        last_pgno: env_info.last_pgno,
        last_txnid: env_info.last_txnid,
        maxreaders: env_info.maxreaders,
        numreaders: env_info.numreaders,
    };

    // Get basic environment stats only for now (can be expanded later)
    let page_size = env_stat.psize as usize;
    let total_pages = env_stat.leaf_pages + env_stat.branch_pages + env_stat.overflow_pages;
    let total_size = total_pages * page_size;
    
    let databases = vec![DatabaseStats {
        name: "Environment Summary".to_string(),
        entries: env_stat.entries,
        total_size,
        avg_size: if env_stat.entries > 0 {
            total_size / env_stat.entries
        } else {
            0
        },
        depth: env_stat.depth,
        total_pages,
        leaf_pages: env_stat.leaf_pages,
        branch_pages: env_stat.branch_pages,
        overflow_pages: env_stat.overflow_pages,
    }];

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



