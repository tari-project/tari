use anyhow::{anyhow, Error};
use async_trait::async_trait;
use clap::Parser;
use tari_common_types::types::{PrivateKey, PublicKey, Signature};
use tari_utilities::hex::Hex;

use super::{CommandContext, HandleCommand};
use crate::commands::args::FromHex;

/// This will search the main chain for the kernel.
/// If the kernel is found, it will print out the
/// block it was found in.
/// This searches for the kernel via the
/// excess signature
#[derive(Debug, Parser)]
pub struct Args {
    /// hex of nonce
    public_nonce: FromHex<PublicKey>,
    /// hex of signature
    signature: FromHex<PrivateKey>,
}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, args: Args) -> Result<(), Error> {
        let kernel_sig = Signature::new(args.public_nonce.0, args.signature.0);
        self.search_kernel(kernel_sig).await
    }
}

impl CommandContext {
    /// Function to process the search kernel command
    pub async fn search_kernel(&mut self, excess_sig: Signature) -> Result<(), Error> {
        let hex_sig = excess_sig.get_signature().to_hex();
        let v = self
            .node_service
            .get_blocks_with_kernels(vec![excess_sig])
            .await?
            .pop()
            .ok_or_else(|| anyhow!("No kernel with signature {} found", hex_sig))?;
        println!("{}", v);
        Ok(())
    }
}
