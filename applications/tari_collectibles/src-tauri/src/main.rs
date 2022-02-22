#![cfg_attr(
  all(not(debug_assertions), target_os = "windows"),
  windows_subsystem = "windows"
)]

use std::error::Error;
use tauri::{Menu, MenuItem, Submenu};

use tari_app_utilities::initialization::init_configuration;
use tari_common::configuration::bootstrap::ApplicationType;

use crate::app_state::ConcurrentAppState;

#[macro_use]
extern crate diesel;

#[macro_use]
extern crate diesel_migrations;

mod app_state;
mod clients;
mod commands;
mod error;
mod models;
mod providers;
mod schema;
mod status;
mod storage;

fn main() -> Result<(), Box<dyn Error>> {
  let (bootstrap, config, _) = init_configuration(ApplicationType::Collectibles)?;
  let state = ConcurrentAppState::new(
    bootstrap.base_path,
    config.collectibles_config.unwrap_or_default(),
  );

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
