// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

#![cfg_attr(
  all(not(debug_assertions), target_os = "windows"),
  windows_subsystem = "windows"
)]

use std::error::Error;
use tauri::{Menu, MenuItem, Submenu};

use clap::Parser;
use std::path::PathBuf;
use tari_common::{exit_codes::ExitError, load_configuration, DefaultConfigLoader};

use crate::{
  app_state::ConcurrentAppState,
  cli::{Cli, Commands},
  config::CollectiblesConfig,
};

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

pub fn process_command(command: Commands, state: &ConcurrentAppState) -> Result<(), ExitError> {
  match command {
    Commands::MakeItRain {
      asset_public_key,
      amount_per_transaction,
      number_transactions,
      destination_address,
      source_address,
    } => cli::make_it_rain(
      asset_public_key,
      amount_per_transaction,
      number_transactions,
      destination_address,
      source_address,
      state,
    ),
    Commands::ListAssets { offset, count } => cli::list_assets(offset, count, state),
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

  if let Some(command) = cli.command {
    process_command(command, &state)?;
    return Ok(());
  }
  //let (bootstrap, config, _) = init_configuration(ApplicationType::Collectibles)?;
  let state = ConcurrentAppState::new(PathBuf::from("."), CollectiblesConfig::default());

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
