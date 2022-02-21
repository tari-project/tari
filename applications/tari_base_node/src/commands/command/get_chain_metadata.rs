use std::time::{Duration, Instant};

use anyhow::{anyhow, Error};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use clap::Parser;
use tari_app_utilities::consts;

use super::{CommandContext, HandleCommand};
use crate::{commands::status_line::StatusLine, StatusOutput};

#[derive(Debug, Parser)]
pub struct Args {}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, args: Args) -> Result<(), Error> {
        let data = self.node_service.get_metadata().await?;
        println!("{}", data);
        Ok(())
    }
}
