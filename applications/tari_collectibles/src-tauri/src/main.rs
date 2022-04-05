// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

#![cfg_attr(
  all(not(debug_assertions), target_os = "windows"),
  windows_subsystem = "windows"
)]

use std::error::Error;
use tauri::{Menu, MenuItem, Submenu};

use clap::Parser;
use tari_common::{
  exit_codes::{ExitCode, ExitError},
  load_configuration, DefaultConfigLoader,
};
use uuid::Uuid;

use crate::{app_state::ConcurrentAppState, cli::Cli, config::CollectiblesConfig};

#[macro_use]
extern crate diesel;

#[macro_use]
extern crate diesel_migrations;

mod app_state;
mod cli;
mod clients;
mod commands;
mod config;
mod error;
mod models;
mod providers;
mod schema;
mod status;
mod storage;

#[derive(Debug)]
pub enum Command {
  MakeItRain {
    asset_public_key: String,
    amount_per_transaction: u64,
    number_transactions: u32,
    destination_address: String,
    source_address: Option<String>,
  },
}

fn parse_make_it_rain(src: &[&str]) -> Result<Command, ExitError> {
  if src.len() < 4 && 5 < src.len() {
    return Err(ExitError::new(
      ExitCode::CommandError,
      "Invalid arguments for make-it-rain",
    ));
  }
  let asset_public_key = src[0].to_string();
  let amount_per_transaction = src[1]
    .to_string()
    .parse::<u64>()
    .map_err(|e| ExitError::new(ExitCode::CommandError, e.to_string()))?;
  let number_transactions = src[2]
    .to_string()
    .parse::<u32>()
    .map_err(|e| ExitError::new(ExitCode::CommandError, e.to_string()))?;
  let destination_address = src[3].to_string();
  let source_address = match src.len() {
    5 => Some(src[4].to_string()),
    _ => None,
  };
  Ok(Command::MakeItRain {
    asset_public_key,
    amount_per_transaction,
    number_transactions,
    destination_address,
    source_address,
  })
}

fn parse_command(src: &str) -> Result<Command, ExitError> {
  let args: Vec<_> = src.split(' ').collect();
  if args.is_empty() {
    return Err(ExitError::new(ExitCode::CommandError, "Empty command"));
  }
  match args[0] {
    "make-it-rain" => parse_make_it_rain(&args[1..]),
    _ => Err(ExitError::new(ExitCode::CommandError, "Invalid command")),
  }
}

// make-it-rain <asset_public_key> <amount_per_transaction> <number_transactions> <destination address> <source address - optional>
fn make_it_rain(
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

pub fn process_command(command: Command, state: &ConcurrentAppState) -> Result<(), ExitError> {
  println!("command {:?}", command);
  match command {
    Command::MakeItRain {
      asset_public_key,
      amount_per_transaction,
      number_transactions: number_transaction,
      destination_address,
      source_address,
    } => make_it_rain(
      asset_public_key,
      amount_per_transaction,
      number_transaction,
      destination_address,
      source_address,
      state,
    ),
  }
}

fn main() -> Result<(), Box<dyn Error>> {
  let cli = Cli::parse();
  let cfg = load_configuration(
    cli.common.config_path(),
    true,
    &cli.config_property_overrides(),
  )?;

  let config = CollectiblesConfig::load_from(&cfg)?;
  let state = ConcurrentAppState::new(cli.common.get_base_path(), config);

  if let Some(ref command) = cli.command {
    let command = parse_command(command)?;
    process_command(command, &state)?;
    return Ok(());
  }

  tauri::Builder::default()
    .menu(build_menu())
    .manage(state)
    .invoke_handler(tauri::generate_handler![
      commands::create_db,
      commands::assets::assets_create,
      commands::assets::assets_list_owned,
      commands::assets::assets_list_registered_assets,
      commands::assets::assets_create_initial_checkpoint,
      commands::assets::assets_create_committee_definition,
      commands::assets::assets_get_committee_definition,
      commands::assets::assets_get_registration,
      commands::asset_wallets::asset_wallets_create,
      commands::asset_wallets::asset_wallets_list,
      commands::asset_wallets::asset_wallets_get_balance,
      commands::asset_wallets::asset_wallets_get_unspent_amounts,
      commands::asset_wallets::asset_wallets_get_latest_address,
      commands::asset_wallets::asset_wallets_create_address,
      commands::asset_wallets::asset_wallets_send_to,
      commands::keys::next_asset_public_key,
      commands::tip004::tip004_mint_token,
      commands::tip004::tip004_list_tokens,
      commands::tip721::tip721_transfer_from,
      commands::wallets::wallets_create,
      commands::wallets::wallets_list,
      commands::wallets::wallets_unlock,
      commands::wallets::wallets_seed_words,
    ])
    .run(tauri::generate_context!())?;

  Ok(())
}

fn build_menu() -> Menu {
  Menu::new()
    .add_submenu(Submenu::new(
      "Tari Collectibles",
      Menu::new()
        .add_native_item(MenuItem::Hide)
        .add_native_item(MenuItem::Quit),
    ))
    .add_submenu(Submenu::new(
      "Edit",
      Menu::new()
        .add_native_item(MenuItem::Copy)
        .add_native_item(MenuItem::Cut)
        .add_native_item(MenuItem::Paste)
        .add_native_item(MenuItem::Separator)
        .add_native_item(MenuItem::Undo)
        .add_native_item(MenuItem::Redo)
        .add_native_item(MenuItem::Separator)
        .add_native_item(MenuItem::SelectAll),
    ))
}
