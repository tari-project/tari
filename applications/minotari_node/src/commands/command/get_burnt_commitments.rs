//  Copyright 2022, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{
    fs,
    fs::OpenOptions,
    io::Write,
    path::PathBuf,
    time::{Duration, Instant},
};

use anyhow::{anyhow, Error};
use async_trait::async_trait;
use clap::Parser;
use tari_transaction_components::transaction_components::BurntCommitmentInfo;
use tari_utilities::hex::Hex;

use super::{CommandContext, HandleCommand};
use crate::table::Table;

/// This will search for all burnt commitments in the blockchain, for the specified range.
/// If no range is specified, it will search the entire chain.
/// The results can be printed to the console or written to a CSV file.
#[derive(Debug, Parser)]
pub struct ArgsGetBurnCommitments {
    /// The block height to start searching from - if not provided, the search will start from the genesis block
    #[clap(short, long)]
    start_height: Option<u64>,
    /// The block height to end searching to - if not provided, the search will end at the tip of the chain
    #[clap(short, long)]
    end_height: Option<u64>,
    /// If provided, will return all burnt commitments found in the blockchain - ignored if the other parameters are
    /// provided
    #[clap(short, long)]
    all: bool,
    /// Write the result to file - results will be written to 'burnt_commitments.csv'
    #[clap(short = 'f', long)]
    output_to_file: bool,
    /// Optional output directory (otherwise current directory will be used)
    #[clap(short = 'd', long)]
    output_directory: Option<PathBuf>,
}

#[async_trait]
impl HandleCommand<ArgsGetBurnCommitments> for CommandContext {
    async fn handle_command(&mut self, args: ArgsGetBurnCommitments) -> Result<(), Error> {
        let blockchain_db = &self.blockchain_db;
        let chain_metadata = blockchain_db.get_chain_metadata().await?;
        let best_block_height = chain_metadata.best_block_height();
        let range = match (args.start_height, args.end_height, args.all) {
            (Some(start), Some(end), _) => {
                if start > end {
                    return Err(anyhow!("start_height cannot be greater than end_height"));
                }
                if end > best_block_height {
                    println!(
                        "End block height ({}) is greater than the best block height ({}), using ({})",
                        end, best_block_height, best_block_height
                    );
                    Some(start..=best_block_height)
                } else {
                    Some(start..=end)
                }
            },
            (Some(start), None, _) => {
                if start > best_block_height {
                    return Err(anyhow!(
                        "start_height ({}) cannot be greater than the best block height ({})",
                        start,
                        best_block_height
                    ));
                }
                Some(start..=best_block_height)
            },
            (None, Some(end), _) => {
                if end > best_block_height {
                    println!(
                        "End block height ({}) is greater than the best block height ({}), using ({})",
                        end, best_block_height, best_block_height
                    );
                    Some(0..=best_block_height)
                } else {
                    Some(0..=end)
                }
            },
            (None, None, true) => None,
            (None, None, false) => {
                return Err(anyhow!(
                    "Either\n   '--start-height <START_HEIGHT>' and '--end-height <END_HEIGHT>', or\n   \
                     '--start-height <START_HEIGHT>', or\n   '--end-height <END_HEIGHT>' or\n   '--all'\nmust be \
                     provided"
                ));
            },
        };

        let start = Instant::now();
        let burnt_commitments_info = blockchain_db.fetch_burnt_commitments_info(range).await?;
        let duration = Instant::now() - start;
        println!();
        println!("'get-burnt-commitments' command completed in {:.2?}.", duration);
        if args.output_to_file {
            print_to_file(&burnt_commitments_info, args.output_directory).await;
        } else {
            print_results_to_console(&burnt_commitments_info);
        }
        println!();

        Ok(())
    }
}

fn print_results_to_console(burnt_commitments_info: &[BurntCommitmentInfo]) {
    // Table print the results to the console
    if burnt_commitments_info.is_empty() {
        println!("No burnt commitments found in the specified range.\n");
        return;
    }

    let mut table = Table::new();
    table.set_titles(vec!["#", "Commitment", "Height", "Header Hash", "Kernel Hash"]);

    for (i, item) in burnt_commitments_info.iter().enumerate() {
        table.add_row(row![
            i + 1,
            item.commitment.to_hex(),
            item.header_height,
            item.header_hash.to_hex(),
            item.kernel_hash.to_hex(),
        ]);
    }
    table.print_stdout();
}

async fn print_to_file(burnt_commitments_info: &[BurntCommitmentInfo], output_directory: Option<PathBuf>) {
    let file_name = "burnt_commitments.csv";
    let file_path = if let Some(path) = output_directory.clone() {
        if let Ok(true) = path.try_exists() {
            path.join(file_name)
        } else if fs::create_dir_all(&path).is_ok() {
            path.join(file_name)
        } else {
            PathBuf::from(file_name)
        }
    } else {
        PathBuf::from(file_name)
    };
    let _unused = fs::remove_file(&file_path);
    tokio::time::sleep(Duration::from_secs(1)).await;

    let write_header = !file_path.exists();
    if let Ok(mut file) = OpenOptions::new().append(true).create(true).open(file_path.clone()) {
        let mut file_content = String::new();
        if write_header {
            file_content.push_str("#,Commitment,Height,Header Hash,Kernel Hash,\n");
        }
        for (i, item) in burnt_commitments_info.iter().enumerate() {
            file_content.push_str(&format!(
                "{},{},{},{},{},\n",
                i + 1,
                item.commitment.to_hex(),
                item.header_height,
                item.header_hash.to_hex(),
                item.kernel_hash.to_hex()
            ));
        }
        match writeln!(file, "{file_content}") {
            Ok(_) => {
                println!("📝 Result written to file: {}", file_path.display());
            },
            Err(e) => {
                println!("❌ Error writing result to file: {e}");
            },
        }
    }
}
