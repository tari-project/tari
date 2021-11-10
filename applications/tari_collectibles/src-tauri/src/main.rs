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
mod base_node_client;
mod commands;
mod models;
mod schema;
mod settings;
mod storage;
mod wallet_client;

fn main() {
  let state = ConcurrentAppState::new();

  tauri::Builder::default()
    .manage(state)
    .invoke_handler(tauri::generate_handler![
      commands::assets::assets_create,
      commands::assets::assets_list_owned,
      commands::assets::assets_list_registered_assets,
      commands::assets::assets_issue_simple_tokens,
      commands::accounts::accounts_create,
      commands::accounts::accounts_list
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
