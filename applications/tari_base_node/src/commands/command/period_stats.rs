use anyhow::{anyhow, Error};
use async_trait::async_trait;
use clap::Parser;
use tokio::io::{self, AsyncWriteExt};

use super::{CommandContext, HandleCommand};

/// Prints out certain aggregated stats to
/// of the block chain in csv format for
/// easy copy.
#[derive(Debug, Parser)]
pub struct Args {
    /// start time in unix timestamp
    period_end: u64,
    /// end time in unix timestamp
    period_ticker_end: u64,
    /// interval period time in unix timestamp
    period: u64,
}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, args: Args) -> Result<(), Error> {
        self.period_stats(args.period_end, args.period_ticker_end, args.period)
            .await
    }
}

impl CommandContext {
    #[allow(deprecated)]
    pub async fn period_stats(
        &mut self,
        period_end: u64,
        mut period_ticker_end: u64,
        period: u64,
    ) -> Result<(), Error> {
        let meta = self.node_service.get_metadata().await?;

        let mut height = meta.height_of_longest_chain();
        // Currently gets the stats for: tx count, hash rate estimation, target difficulty, solvetime.
        let mut results: Vec<(usize, f64, u64, u64, usize)> = Vec::new();

        let mut period_ticker_start = period_ticker_end - period;
        let mut period_tx_count = 0;
        let mut period_block_count = 0;
        let mut period_hash = 0.0;
        let mut period_difficulty = 0;
        let mut period_solvetime = 0;
        print!("Searching for height: ");
        while height > 0 {
            print!("{}", height);
            io::stdout().flush().await?;

            let block = self
                .node_service
                .get_block(height)
                .await?
                .ok_or_else(|| anyhow!("Error in db, block not found at height {}", height))?;

            let prev_block = self
                .node_service
                .get_block(height - 1)
                .await?
                .ok_or_else(|| anyhow!("Error in db, block not found at height {}", height))?;

            height -= 1;
            if block.header().timestamp.as_u64() > period_ticker_end {
                print!("\x1B[{}D\x1B[K", (height + 1).to_string().chars().count());
                continue;
            };
            while block.header().timestamp.as_u64() < period_ticker_start {
                results.push((
                    period_tx_count,
                    period_hash,
                    period_difficulty,
                    period_solvetime,
                    period_block_count,
                ));
                period_tx_count = 0;
                period_block_count = 0;
                period_hash = 0.0;
                period_difficulty = 0;
                period_solvetime = 0;
                period_ticker_end -= period;
                period_ticker_start -= period;
            }
            period_tx_count += block.block().body.kernels().len() - 1;
            period_block_count += 1;
            let st = if prev_block.header().timestamp.as_u64() >= block.header().timestamp.as_u64() {
                1.0
            } else {
                (block.header().timestamp.as_u64() - prev_block.header().timestamp.as_u64()) as f64
            };
            let diff = block.accumulated_data.target_difficulty.as_u64();
            period_difficulty += diff;
            period_solvetime += st as u64;
            period_hash += diff as f64 / st / 1_000_000.0;
            if period_ticker_end <= period_end {
                break;
            }
            print!("\x1B[{}D\x1B[K", (height + 1).to_string().chars().count());
        }
        println!("Complete");
        println!("Results of tx count, hash rate estimation, target difficulty, solvetime, block count");
        for data in results {
            println!("{},{},{},{},{}", data.0, data.1, data.2, data.3, data.4);
        }
        Ok(())
    }
}
