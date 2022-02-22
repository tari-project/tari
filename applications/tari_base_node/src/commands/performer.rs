use anyhow::Error;
use derive_more::{Deref, DerefMut};
use log::*;
use strum::IntoEnumIterator;
use tari_app_utilities::utilities::{UniNodeId, UniPublicKey};
use tari_comms::peer_manager::NodeId;
use tari_core::proof_of_work::PowAlgorithm;
use tari_shutdown::Shutdown;
use tari_utilities::ByteArray;

use super::{
    args::{Args, ArgsError, ArgsReason, FromHex},
    command_handler::CommandHandler,
    parser::BaseNodeCommand,
};
use crate::LOG_TARGET;

#[derive(Deref, DerefMut)]
pub struct Performer {
    command_handler: CommandHandler,
}

impl Performer {
    pub fn new(command_handler: CommandHandler) -> Self {
        Self { command_handler }
    }

    /// This will parse the provided command and execute the task
    pub async fn handle_command(&mut self, command_str: &str, shutdown: &mut Shutdown) {
        if command_str.trim().is_empty() {
            return;
        }

        let mut typed_args = Args::split(command_str);
        let command = typed_args.take_next("command");
        match command {
            Ok(command) => {
                let res = self.process_command(command, typed_args, shutdown).await;
                if let Err(err) = res {
                    println!("Command Error: {}", err);
                    self.print_help(command);
                }
            },
            Err(_) => {
                println!("{} is not a valid command, please enter a valid command", command_str);
                println!("Enter help or press tab for available commands");
            },
        }
    }

    /// Function to process commands
    async fn process_command<'a>(
        &mut self,
        command: BaseNodeCommand,
        mut typed_args: Args<'a>,
        shutdown: &mut Shutdown,
    ) -> Result<(), Error> {
        use BaseNodeCommand::*;
        match command {
            Help => {
                let command = typed_args.take_next("help-command")?;
                self.print_help(command);
                Ok(())
            },
            GetPeer => self.process_get_peer(typed_args).await,
            Exit | Quit => {
                println!("Shutting down...");
                info!(
                    target: LOG_TARGET,
                    "Termination signal received from user. Shutting node down."
                );
                let _ = shutdown.trigger();
                Ok(())
            },
        }
    }

    /// Displays the commands or context specific help for a given command
    fn print_help(&self, command: BaseNodeCommand) {
        use BaseNodeCommand::*;
        match command {
            Help => {
                println!("Available commands are: ");
                // TODO: Improve that
                let joined = BaseNodeCommand::iter()
                    .map(|item| item.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                println!("{}", joined);
            },
            GetPeer => {
                println!("Get all available info about peer");
                println!("Usage: get-peer [Partial NodeId | PublicKey | EmojiId]");
            },
            PeriodStats => {
                println!(
                    "Prints out certain aggregated stats to of the block chain in csv format for easy copy, use as \
                     follows: "
                );
                println!(
                    "Period-stats [start time in unix timestamp] [end time in unix timestamp] [interval period time \
                     in unix timestamp]"
                );
            },
            Exit | Quit => {
                println!("Exits the base node");
            },
        }
    }

    async fn process_get_peer<'a>(&mut self, mut args: Args<'a>) -> Result<(), Error> {
        let original_str = args
            .try_take_next("node_id")?
            .ok_or_else(|| ArgsError::new("node_id", ArgsReason::Required))?;
        let node_id: Option<UniNodeId> = args.try_take_next("node_id")?;
        let partial;
        if let Some(node_id) = node_id {
            partial = NodeId::from(node_id).to_vec();
        } else {
            let data: FromHex<_> = args.take_next("node_id")?;
            partial = data.0;
        }
        self.command_handler.get_peer(partial, original_str).await;
        Ok(())
    }
}
