#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

use log::*;
use tari_launchpad::{
    __cmd__create_default_workspace,
    __cmd__create_new_workspace,
    __cmd__events,
    __cmd__image_list,
    __cmd__launch_docker,
    __cmd__pull_images,
    __cmd__shutdown,
    __cmd__start_service,
    __cmd__stop_service,
    commands::*,
    docker::{DockerWrapper, Workspaces},
};
use tauri::{api::cli::get_matches, async_runtime::block_on, utils::config::CliConfig, Manager, PackageInfo, RunEvent};

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

    // TODO - Load workspace definitions from persistent storage here
    let workspaces = Workspaces::default();
    info!("Using Docker version: {}", docker.version());

    let app = tauri::Builder::default()
        .manage(AppState::new(docker, workspaces, package_info))
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
        .build(context)
        .expect("error while running Launchpad");

    app.run(|app, event| {
        if let RunEvent::Exit = event {
            info!("Received Exit event");
            block_on(async move {
                let state = app.state();
                let _ = shutdown(state).await;
            });
        }
    });
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
