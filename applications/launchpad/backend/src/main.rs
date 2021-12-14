#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

use log::*;
use tari_launchpad::{
    __cmd__create_new_workspace,
    __cmd__image_list,
    __cmd__launch_docker,
    __cmd__pull_images,
    __cmd__shutdown,
    commands::*,
    docker::{DockerWrapper, Workspaces},
};
use tauri::{api::cli::get_matches, async_runtime::block_on, utils::config::CliConfig, Event, Manager};

fn main() {
    env_logger::init();
    let context = tauri::generate_context!();
    let cli_config = context.config().tauri.cli.clone().unwrap();

    // Handle --help and --version. Exits after printing
    handle_cli_options(&cli_config);

    let docker = match DockerWrapper::new() {
        Ok(docker) => docker,
        Err(err) => {
            error!("Could not launch docker backend. {}", err.chained_message());
            std::process::exit(-1);
        },
    };

    // TODO - Load workspace definitions from persistent storage here
    let workspaces = Workspaces::new();
    // We're going to attach this to the AppState because Tauri does not expose it for some reason
    let package_info = context.package_info().clone();
    info!("Using Docker version: {}", docker.version());

    let app = tauri::Builder::default()
        .manage(AppState::new(docker, workspaces, package_info))
        .invoke_handler(tauri::generate_handler![
            image_list,
            pull_images,
            create_new_workspace,
            launch_docker,
            shutdown
        ])
        .build(context)
        .expect("error while running Launchpad");

    app.run(|app, event| {
        if let Event::Exit = event {
            info!("Received Exit event");
            block_on(async move {
                let state = app.state();
                let _ = shutdown(state).await;
            });
        }
    });
}

fn handle_cli_options(cli_config: &CliConfig) {
    match get_matches(cli_config) {
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
