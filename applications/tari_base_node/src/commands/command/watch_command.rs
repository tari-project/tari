use std::fmt;

use anyhow::Error;
use async_trait::async_trait;
use clap::Parser;

use super::{CommandContext, HandleCommand};

pub type WatchCommand = Args;

impl fmt::Display for WatchCommand {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        self.line().fmt(f)
    }
}

const DEFAULT_WATCH: &str = "status";

/// Repeat a command within an interval.
#[derive(Debug, Parser)]
pub struct Args {
    /// Interval in seconds
    #[clap(short, long)]
    pub interval: Option<u64>,
    /// The command to perform. `status` if empty.
    #[clap(default_value = DEFAULT_WATCH)]
    pub command: String,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            interval: None,
            command: DEFAULT_WATCH.into(),
        }
    }
}

impl Args {
    pub fn line(&self) -> &str {
        &self.command
    }
}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, _: Args) -> Result<(), Error> {
        Ok(())
    }
}
