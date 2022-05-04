// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

// FIXME: ignoring for now
#![allow(unused_imports)]
#![allow(dead_code)]
#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]
use log::*;
mod commands;
mod docker;
mod error;
use docker::{DockerWrapper, Workspaces};
use tauri::{
    api::cli::get_matches,
    async_runtime::block_on,
    utils::config::CliConfig,
    CustomMenuItem,
    Manager,
    Menu,
    MenuItem,
    PackageInfo,
    RunEvent,
    Submenu,
};

use crate::commands::{
    create_default_workspace,
    create_new_workspace,
    events,
    image_list,
    launch_docker,
    pull_images,
    shutdown,
    start_service,
    stop_service,
    AppState,
};

fn main() {
    env_logger::init();
    let context = tauri::generate_context!();
    let cli_config = context.config().tauri.cli.clone().unwrap();

    // We're going to attach this to the AppState because Tauri does not expose it for some reason
    let package_info = context.package_info().clone();
    // Handle --help and --version. Exits after printing
    handle_cli_options(&cli_config, &package_info);

    let docker = match DockerWrapper::new() {
        Ok(docker) => docker,
        Err(err) => {
            error!("Could not launch docker backend. {}", err.chained_message());
            std::process::exit(-1);
        },
    };

    let about_menu = Submenu::new(
        "App",
        Menu::new()
            .add_native_item(MenuItem::Hide)
            .add_native_item(MenuItem::HideOthers)
            .add_native_item(MenuItem::ShowAll)
            .add_native_item(MenuItem::Separator)
            .add_native_item(MenuItem::Quit),
    );

    let edit_menu = Submenu::new(
        "Edit",
        Menu::new()
            .add_native_item(MenuItem::Cut)
            .add_native_item(MenuItem::Copy)
            .add_native_item(MenuItem::Paste)
            .add_native_item(MenuItem::SelectAll),
    );

    let view_menu = Submenu::new("View", Menu::new().add_native_item(MenuItem::EnterFullScreen));

    let menu = Menu::new()
        .add_submenu(about_menu)
        .add_submenu(edit_menu)
        .add_submenu(view_menu);

    // TODO - Load workspace definitions from persistent storage here
    let workspaces = Workspaces::default();
    info!("Using Docker version: {}", docker.version());

    tauri::Builder::default()
        .manage(AppState::new(docker, workspaces, package_info))
        .menu(menu)
        .invoke_handler(tauri::generate_handler![
            image_list,
            pull_images,
            create_new_workspace,
            create_default_workspace,
            events,
            launch_docker,
            start_service,
            stop_service,
            shutdown
        ])
        .run(context)
        .expect("error starting");
    // .build(context)
    // .expect("error while running Launchpad");

    // app.run(|app, event| {
    //     if let RunEvent::Exit = event {
    //         info!("Received Exit event");
    //         block_on(async move {
    //             let state = app.state();
    //             let _message = shutdown(state).await;
    //         });
    //     }
    // });
}

fn handle_cli_options(cli_config: &CliConfig, pkg_info: &PackageInfo) {
    match get_matches(cli_config, pkg_info) {
        Ok(matches) => {
            if let Some(arg_data) = matches.args.get("help") {
                debug!("{}", arg_data.value.as_str().unwrap_or("No help available"));
                std::process::exit(0);
            }
            if let Some(arg_data) = matches.args.get("version") {
                debug!("{}", arg_data.value.as_str().unwrap_or("No version data available"));
                std::process::exit(0);
            }
        },
        Err(e) => {
            error!("{}", e.to_string());
            std::process::exit(1);
        },
    }
}
