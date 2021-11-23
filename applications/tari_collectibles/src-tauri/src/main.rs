#![feature(array_methods)]
#![cfg_attr(
  all(not(debug_assertions), target_os = "windows"),
  windows_subsystem = "windows"
)]

use crate::app_state::ConcurrentAppState;

#[macro_use]
extern crate diesel;

#[macro_use]
extern crate diesel_migrations;

mod app_state;
mod clients;
mod commands;
mod models;
mod schema;
mod settings;
mod storage;

fn main() {
  let state = ConcurrentAppState::new();

  tauri::Builder::default()
    .manage(state)
    .invoke_handler(tauri::generate_handler![
      commands::create_db,
      commands::assets::assets_create,
      commands::assets::assets_list_owned,
      commands::assets::assets_list_registered_assets,
      commands::assets::assets_create_initial_checkpoint,
      commands::assets::assets_get_registration,
      commands::accounts::accounts_create,
      commands::accounts::accounts_list,
      commands::keys::next_asset_public_key,
      commands::wallets::wallets_create,
      commands::wallets::wallets_list,
      commands::wallets::wallets_find,
      commands::wallets::wallets_seed_words,
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
