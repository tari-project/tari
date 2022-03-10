// Copyright 2022. The Tari Project
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

use crate::{commands, commands::assets::inner_assets_list_registered_assets, ConcurrentAppState};
use clap::{Parser, Subcommand};
use tari_common::exit_codes::{ExitCode, ExitError};
use tonic::codegen::Body;
use uuid::Uuid;

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
#[clap(propagate_version = true)]
pub struct Cli {
  #[clap(subcommand)]
  pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
  ListAssets {
    offset: u64,
    count: u64,
  },
  MakeItRain {
    asset_public_key: String,
    amount_per_transaction: u64,
    number_transactions: u32,
    destination_address: String,
    source_address: Option<String>,
  },
  RegisterAsset {},
}

pub fn register_asset(state: &ConcurrentAppState) -> Result<(), ExitError> {
  let runtime = tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .build()
    .expect("Failed to build a runtime!");
  match runtime.block_on(inner(offset, count, state)) {
    Ok(rows) => {
      println!("{}", serde_json::to_string_pretty(&rows).unwrap());
      Ok(())
    }
    Err(e) => Err(ExitError::new(ExitCode::CommandError, e.to_string())),
  }
}

pub fn list_assets(offset: u64, count: u64, state: &ConcurrentAppState) -> Result<(), ExitError> {
  let runtime = tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .build()
    .expect("Failed to build a runtime!");
  match runtime.block_on(inner_assets_list_registered_assets(offset, count, state)) {
    Ok(rows) => {
      println!("{}", serde_json::to_string_pretty(&rows).unwrap());
      Ok(())
    }
    Err(e) => Err(ExitError::new(ExitCode::CommandError, e.to_string())),
  }
}

// make-it-rain <asset_public_key> <amount_per_transaction> <number_transactions> <destination address> <source address - optional>
pub(crate) fn make_it_rain(
  asset_public_key: String,
  amount: u64,
  number_transactions: u32,
  to_address: String,
  source_address: Option<String>,
  state: &ConcurrentAppState,
) -> Result<(), ExitError> {
  let runtime = tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .build()
    .expect("Failed to build a runtime!");
  let id = match runtime.block_on(commands::wallets::inner_wallets_list(state)) {
    Ok(rows) => {
      if rows.is_empty() {
        return Err(ExitError::new(
          ExitCode::CommandError,
          "There is no wallet!",
        ));
      }
      match source_address {
        Some(source_address) => {
          let source_uuid = Uuid::parse_str(&source_address)
            .map_err(|e| ExitError::new(ExitCode::CommandError, e.to_string()))?;
          if !rows.iter().any(|wallet| wallet.id == source_uuid) {
            return Err(ExitError::new(ExitCode::CommandError, "Wallet not found!"));
          }
          source_uuid
        }
        None => rows[0].id,
      }
    }
    Err(e) => {
      return Err(ExitError::new(ExitCode::CommandError, e.to_string()));
    }
  };

  runtime
    .block_on(commands::wallets::inner_wallets_unlock(id, state))
    .map_err(|e| ExitError::new(ExitCode::CommandError, e.to_string()))?;
  println!(
    "Sending {} of {} to {} {} times.",
    asset_public_key, amount, to_address, number_transactions
  );
  for _ in 0..number_transactions {
    runtime
      .block_on(commands::asset_wallets::inner_asset_wallets_send_to(
        asset_public_key.clone(),
        amount,
        to_address.clone(),
        state,
      ))
      .map_err(|e| ExitError::new(ExitCode::CommandError, e.to_string()))?;
  }
  Ok(())
}
