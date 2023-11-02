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

use std::{cmp, convert::TryFrom, io::Write};

use anyhow::Error;
use async_trait::async_trait;
use chrono::{NaiveDateTime, Utc};
use clap::Parser;
use tari_core::proof_of_work::{lwma_diff::LinearWeightedMovingAverage, PowAlgorithm};
use tari_utilities::hex::Hex;
use tokio::{
    fs::File,
    io::{self, AsyncWriteExt},
};

use super::{CommandContext, HandleCommand};

/// Prints out certain stats to of the block chain in csv format for easy copy, use as follows:
/// header-stats 0 1000
/// header-stats 0 1000 sample2.csv
/// header-stats 0 1000 monero-sample.csv monero
#[derive(Debug, Parser)]
pub struct Args {
    /// start height
    start_height: u64,
    /// end height
    end_height: u64,
    /// dump file
    #[clap(default_value = "header-data.csv")]
    filename: String,
    /// filter:monero|sha3
    pow_algo: Option<PowAlgorithm>,
}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, args: Args) -> Result<(), Error> {
        self.save_header_stats(args.start_height, args.end_height, args.filename, args.pow_algo)
            .await
    }
}

impl CommandContext {
    #[allow(clippy::cast_possible_wrap)]
    pub async fn save_header_stats(
        &self,
        start_height: u64,
        end_height: u64,
        filename: String,
        pow_algo: Option<PowAlgorithm>,
    ) -> Result<(), Error> {
        let mut output = File::create(&filename).await?;

        println!(
            "Loading header from height {} to {} and dumping to file [working-dir]/{}.{}",
            start_height,
            end_height,
            filename,
            pow_algo.map(|a| format!(" PoW algo = {}", a)).unwrap_or_default()
        );

        let start_height = cmp::max(start_height, 1);
        let mut prev_header = self.blockchain_db.fetch_chain_header(start_height - 1).await?;

        let mut buff = Vec::new();
        writeln!(
            buff,
            "Height,Achieved,TargetDifficulty,CalculatedDifficulty,SolveTime,NormalizedSolveTime,Algo,Timestamp,\
             Window,Acc.Monero,Acc.Sha3"
        )?;
        output.write_all(&buff).await?;

        for height in start_height..=end_height {
            let header = self.blockchain_db.fetch_chain_header(height).await?;

            // Optionally, filter out pow algos
            if pow_algo.map(|algo| header.header().pow_algo() != algo).unwrap_or(false) {
                continue;
            }

            let target_diff = self
                .blockchain_db
                .fetch_target_difficulties_for_next_block(*prev_header.hash())
                .await?;
            let pow_algo = header.header().pow_algo();

            let min = self
                .consensus_rules
                .consensus_constants(height)
                .min_pow_difficulty(pow_algo);
            let max = self
                .consensus_rules
                .consensus_constants(height)
                .max_pow_difficulty(pow_algo);

            let calculated_target_difficulty = target_diff.get(pow_algo).calculate(min, max);
            let existing_target_difficulty = header.accumulated_data().target_difficulty;
            let achieved = header.accumulated_data().achieved_difficulty;
            let solve_time = header.header().timestamp.as_u64() as i64 - prev_header.header().timestamp.as_u64() as i64;
            let normalized_solve_time = cmp::min(
                u64::try_from(cmp::max(solve_time, 1)).unwrap(),
                LinearWeightedMovingAverage::max_block_time(
                    self.consensus_rules
                        .consensus_constants(height)
                        .pow_target_block_interval(pow_algo),
                )
                .map_err(Error::msg)?,
            );
            let acc_sha3 = header.accumulated_data().accumulated_sha3x_difficulty;
            let acc_monero = header.accumulated_data().accumulated_randomx_difficulty;

            buff.clear();
            writeln!(
                buff,
                "{},{},{},{},{},{},{},{},{},{},{}",
                height,
                achieved.as_u64(),
                existing_target_difficulty.as_u64(),
                calculated_target_difficulty.as_u64(),
                solve_time,
                normalized_solve_time,
                pow_algo,
                chrono::DateTime::<Utc>::from_naive_utc_and_offset(
                    NaiveDateTime::from_timestamp_opt(header.header().timestamp.as_u64() as i64, 0).unwrap_or_default(),
                    Utc
                ),
                target_diff.get(pow_algo).len(),
                acc_monero.as_u64(),
                acc_sha3.as_u64(),
            )?;
            output.write_all(&buff).await?;

            if header.header().hash() != header.accumulated_data().hash {
                eprintln!(
                    "Difference in hash at {}! header = {} and accum hash = {}",
                    height,
                    header.header().hash().to_hex(),
                    header.accumulated_data().hash.to_hex()
                );
            }

            if existing_target_difficulty != calculated_target_difficulty {
                eprintln!(
                    "Difference at {}! existing = {} and calculated = {}",
                    height, existing_target_difficulty, calculated_target_difficulty
                );
            }

            print!("{}", height);
            io::stdout().flush().await?;
            print!("\x1B[{}D\x1B[K", (height + 1).to_string().chars().count());
            prev_header = header;
        }
        println!("Complete");
        Ok(())
    }
}
