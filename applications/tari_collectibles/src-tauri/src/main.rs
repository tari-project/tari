#![cfg_attr(
  all(not(debug_assertions), target_os = "windows"),
  windows_subsystem = "windows"
)]

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

fn main() {
  #[allow(unused_mut)] // config isn't mutated on windows
  let (bootstrap, mut config, _) = init_configuration(ApplicationType::Collectibles).unwrap();
  let state = ConcurrentAppState::new(bootstrap.base_path, config.collectibles_config.unwrap());

  tauri::Builder::default()
    .manage(state)
    .invoke_handler(tauri::generate_handler![
      commands::create_db,
      commands::assets::assets_create,
      commands::assets::assets_list_owned,
      commands::assets::assets_list_registered_assets,
      commands::assets::assets_create_initial_checkpoint,
      commands::assets::assets_get_registration,
      commands::asset_wallets::asset_wallets_create,
      commands::asset_wallets::asset_wallets_list,
      commands::asset_wallets::asset_wallets_get_balance,
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
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
