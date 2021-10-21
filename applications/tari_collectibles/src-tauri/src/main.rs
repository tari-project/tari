#![cfg_attr(
  all(not(debug_assertions), target_os = "windows"),
  windows_subsystem = "windows"
)]

use crate::app_state::ConcurrentAppState;
use serde::Serialize;
use tari_utilities::hex::Hex;

mod app_state;
mod commands;
mod models;
mod settings;
mod wallet_client;

fn main() {
  let state = ConcurrentAppState::new();

  tauri::Builder::default()
    .manage(state.clone())
    .invoke_handler(tauri::generate_handler![
      commands::assets::assets_create,
      commands::assets::assets_list,
      commands::assets::assets_issue_simple_tokens
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
