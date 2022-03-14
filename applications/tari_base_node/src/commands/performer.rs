use std::time::Duration;

use anyhow::Error;
use derive_more::{Deref, DerefMut};
use log::*;
use strum::IntoEnumIterator;
use tari_app_utilities::utilities::{UniNodeId, UniPublicKey};
use tari_common_types::types::{Commitment, PrivateKey, PublicKey, Signature};
use tari_comms::peer_manager::NodeId;
use tari_core::proof_of_work::PowAlgorithm;
use tari_shutdown::Shutdown;
use tari_utilities::ByteArray;

use super::{
    args::{Args, ArgsError, ArgsReason, FromHex},
    command_handler::{CommandHandler, StatusLineOutput},
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
            Status => self.command_handler.status(StatusLineOutput::StdOutAndLog).await,
            GetStateInfo => self.command_handler.state_info(),
            Version => self.command_handler.print_version(),
            CheckForUpdates => self.command_handler.check_for_updates().await,
            GetChainMetadata => self.command_handler.get_chain_meta().await,
            GetDbStats => self.command_handler.get_blockchain_db_stats().await,
            DialPeer => self.process_dial_peer(typed_args).await,
            PingPeer => self.process_ping_peer(typed_args).await,
            DiscoverPeer => self.process_discover_peer(typed_args).await,
            GetPeer => self.process_get_peer(typed_args).await,
            ListPeers => self.process_list_peers(typed_args).await,
            ResetOfflinePeers => self.command_handler.reset_offline_peers().await,
            RewindBlockchain => self.process_rewind_blockchain(typed_args).await,
            CheckDb => self.command_handler.check_db().await,
            PeriodStats => self.process_period_stats(typed_args).await,
            HeaderStats => self.process_header_stats(typed_args).await,
            BanPeer => self.process_ban_peer(typed_args, true).await,
            UnbanPeer => self.process_ban_peer(typed_args, false).await,
            UnbanAllPeers => self.command_handler.unban_all_peers().await,
            ListBannedPeers => self.command_handler.list_banned_peers().await,
            ListConnections => self.command_handler.list_connections().await,
            ListHeaders => self.process_list_headers(typed_args).await,
            BlockTiming | CalcTiming => self.process_block_timing(typed_args).await,
            ListReorgs => self.process_list_reorgs().await,
            GetBlock => self.process_get_block(typed_args).await,
            SearchUtxo => self.process_search_utxo(typed_args).await,
            SearchKernel => self.process_search_kernel(typed_args).await,
            GetMempoolStats => self.command_handler.get_mempool_stats().await,
            GetMempoolState => self.command_handler.get_mempool_state(None).await,
            GetMempoolTx => self.get_mempool_state_tx(typed_args).await,
            Whoami => self.command_handler.whoami(),
            GetNetworkStats => self.command_handler.get_network_stats(),
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
            Status => {
                println!("Prints out the status of this node");
            },
            GetStateInfo => {
                println!("Prints out the status of the base node state machine");
            },
            Version => {
                println!("Gets the current application version");
            },
            CheckForUpdates => {
                println!("Checks for software updates if auto update is enabled");
            },
            GetChainMetadata => {
                println!("Gets your base node chain meta data");
            },
            GetDbStats => {
                println!("Gets your base node database stats");
            },
            DialPeer => {
                println!("Attempt to connect to a known peer");
                println!("dial-peer [hex public key or emoji id]");
            },
            PingPeer => {
                println!("Send a ping to a known peer and wait for a pong reply");
                println!("ping-peer [hex public key or emoji id]");
            },
            DiscoverPeer => {
                println!("Attempt to discover a peer on the Tari network");
                println!("discover-peer [hex public key or emoji id]");
            },
            GetPeer => {
                println!("Get all available info about peer");
                println!("Usage: get-peer [Partial NodeId | PublicKey | EmojiId]");
            },
            ListPeers => {
                println!("Lists the peers that this node knows about");
            },
            ResetOfflinePeers => {
                println!("Clear offline flag from all peers");
            },
            RewindBlockchain => {
                println!("Rewinds the blockchain to the given height.");
                println!("Usage: {} [new_height]", command);
                println!("new_height must be less than the current height.");
            },
            BanPeer => {
                println!("Bans a peer");
                println!(
                    "ban-peer/unban-peer [hex public key or emoji id] (length of time to ban the peer for in seconds)"
                );
            },
            UnbanPeer => {
                println!("Removes a peer ban");
            },
            UnbanAllPeers => {
                println!("Unbans all peers");
            },
            ListBannedPeers => {
                println!("Lists peers that have been banned by the node or wallet");
            },
            CheckDb => {
                println!("Checks the blockchain database for missing blocks and headers");
            },
            HeaderStats => {
                println!(
                    "Prints out certain stats to of the block chain in csv format for easy copy, use as follows: "
                );
                println!("header-stats [start height] [end height] (dump_file) (filter:monero|sha3)");
                println!("e.g.");
                println!("header-stats 0 1000");
                println!("header-stats 0 1000 sample2.csv");
                println!("header-stats 0 1000 monero-sample.csv monero");
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
            ListConnections => {
                println!("Lists the peer connections currently held by this node");
            },
            ListHeaders => {
                println!("List the amount of headers, can be called in the following two ways: ");
                println!("list-headers [first header height] [last header height]");
                println!("list-headers [number of headers starting from the chain tip back]");
            },
            BlockTiming | CalcTiming => {
                println!("Calculates the maximum, minimum, and average time taken to mine a given range of blocks.");
                println!("block-timing [start height] [end height]");
                println!("block-timing [number of blocks from chain tip]");
            },
            ListReorgs => {
                println!("List tracked reorgs.");
                println!(
                    "This feature must be enabled by setting `track_reorgs = true` in the [base_node] section of your \
                     config."
                );
            },
            GetBlock => {
                println!("Display a block by height or hash:");
                println!("get-block [height or hash of the block] [format]");
                println!(
                    "[height or hash of the block] The height or hash of the block to fetch from the main chain. The \
                     genesis block has height zero."
                );
                println!(
                    "[format] Optional. Supported options are 'json' and 'text'. 'text' is the default if omitted."
                );
            },
            SearchUtxo => {
                println!(
                    "This will search the main chain for the utxo. If the utxo is found, it will print out the block \
                     it was found in."
                );
                println!("search-utxo [hex of commitment of the utxo]");
            },
            SearchKernel => {
                println!(
                    "This will search the main chain for the kernel. If the kernel is found, it will print out the \
                     block it was found in."
                );
                println!("This searches for the kernel via the excess signature");
                println!("search-kernel [hex of nonce] [Hex of signature]");
            },
            GetMempoolStats => {
                println!("Retrieves your mempools stats");
            },
            GetMempoolState => {
                println!("Retrieves your mempools state");
            },
            GetMempoolTx => {
                println!("Filters and retrieves details about transactions from the mempool's state");
            },
            Whoami => {
                println!(
                    "Display identity information about this node, including: public key, node ID and the public \
                     address"
                );
            },
            GetNetworkStats => {
                println!("Displays network stats");
            },
            Exit | Quit => {
                println!("Exits the base node");
            },
        }
    }

    /// Function to process the get-block command
    async fn process_get_block<'a>(&self, mut args: Args<'a>) -> Result<(), Error> {
        let height = args.try_take_next("height")?;
        let hash: Option<FromHex<Vec<u8>>> = args.try_take_next("hash")?;
        args.shift_one();
        let format = args.try_take_next("format")?.unwrap_or_default();

        match (height, hash) {
            (Some(height), _) => self.command_handler.get_block(height, format).await,
            (_, Some(hash)) => self.command_handler.get_block_by_hash(hash.0, format).await,
            _ => Err(ArgsError::new(
                "height",
                "Invalid block height or hash provided. Height must be an integer.",
            )
            .into()),
        }
    }

    /// Function to process the search utxo command
    async fn process_search_utxo<'a>(&mut self, mut args: Args<'a>) -> Result<(), Error> {
        let commitment: FromHex<Commitment> = args.take_next("hex")?;
        self.command_handler.search_utxo(commitment.0).await
    }

    /// Function to process the search kernel command
    async fn process_search_kernel<'a>(&mut self, mut args: Args<'a>) -> Result<(), Error> {
        let public_nonce: FromHex<PublicKey> = args.take_next("public-key")?;
        let signature: FromHex<PrivateKey> = args.take_next("private-key")?;
        let kernel_sig = Signature::new(public_nonce.0, signature.0);
        self.command_handler.search_kernel(kernel_sig).await
    }

    async fn get_mempool_state_tx<'a>(&mut self, mut args: Args<'a>) -> Result<(), Error> {
        let filter = args.take_next("filter").ok();
        self.command_handler.get_mempool_state(filter).await
    }

    /// Function to process the discover-peer command
    async fn process_discover_peer<'a>(&mut self, mut args: Args<'a>) -> Result<(), Error> {
        let key: UniPublicKey = args.take_next("id")?;
        self.command_handler.discover_peer(Box::new(key.into())).await
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

    /// Function to process the list-peers command
    async fn process_list_peers<'a>(&mut self, mut args: Args<'a>) -> Result<(), Error> {
        let filter = args.take_next("filter").ok();
        self.command_handler.list_peers(filter).await
    }

    /// Function to process the dial-peer command
    async fn process_dial_peer<'a>(&mut self, mut args: Args<'a>) -> Result<(), Error> {
        let dest_node_id: UniNodeId = args.take_next("node-id")?;
        self.command_handler.dial_peer(dest_node_id.into()).await
    }

    /// Function to process the dial-peer command
    async fn process_ping_peer<'a>(&mut self, mut args: Args<'a>) -> Result<(), Error> {
        let dest_node_id: UniNodeId = args.take_next("node-id")?;
        self.command_handler.ping_peer(dest_node_id.into()).await
    }

    /// Function to process the ban-peer command
    async fn process_ban_peer<'a>(&mut self, mut args: Args<'a>, must_ban: bool) -> Result<(), Error> {
        let node_id: UniNodeId = args.take_next("node-id")?;
        let secs = args.try_take_next("length")?.unwrap_or(std::u64::MAX);
        let duration = Duration::from_secs(secs);
        self.command_handler.ban_peer(node_id.into(), duration, must_ban).await;
        Ok(())
    }

    /// Function to process the list-headers command
    async fn process_list_headers<'a>(&self, mut args: Args<'a>) -> Result<(), Error> {
        let start = args.take_next("start")?;
        let end = args.try_take_next("end")?;
        self.command_handler.list_headers(start, end).await;
        Ok(())
    }

    /// Function to process the calc-timing command
    async fn process_block_timing<'a>(&self, mut args: Args<'a>) -> Result<(), Error> {
        let start = args.take_next("start")?;
        let end = args.try_take_next("end")?;
        if end.is_none() && start < 2 {
            Err(ArgsError::new("start", "Number of headers must be at least 2.").into())
        } else {
            self.command_handler.block_timing(start, end).await
        }
    }

    async fn process_period_stats<'a>(&mut self, mut args: Args<'a>) -> Result<(), Error> {
        let period_end = args.take_next("period_end")?;
        let period_ticker_end = args.take_next("period_ticker_end")?;
        let period = args.take_next("period")?;
        self.command_handler
            .period_stats(period_end, period_ticker_end, period)
            .await
    }

    async fn process_header_stats<'a>(&self, mut args: Args<'a>) -> Result<(), Error> {
        let start_height = args.take_next("start_height")?;
        let end_height = args.take_next("end_height")?;
        let filename = args
            .try_take_next("filename")?
            .unwrap_or_else(|| "header-data.csv".into());
        let algo: Option<PowAlgorithm> = args.try_take_next("algo")?;

        self.command_handler
            .save_header_stats(start_height, end_height, filename, algo)
            .await
    }

    async fn process_rewind_blockchain<'a>(&self, mut args: Args<'a>) -> Result<(), Error> {
        let new_height = args.take_next("new_height")?;
        self.command_handler.rewind_blockchain(new_height).await
    }

    async fn process_list_reorgs(&self) -> Result<(), Error> {
        self.command_handler.list_reorgs()
    }
}
