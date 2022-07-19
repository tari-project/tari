// Copyright 2021. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
//

use std::{
    convert::TryFrom,
    path::PathBuf,
    process::{Command, Stdio},
    time::Duration,
};

use bollard::{system::Version, Docker};
use log::*;
use tauri::{api::path::home_dir, AppHandle, Wry};

use crate::{commands::AppState, docker::DOCKER_INSTANCE};

const LOG_TARGET: &str = "tari_launchpad::host";

#[tauri::command]
pub async fn check_docker() -> Result<Version, String> {
    let version: Version = DOCKER_INSTANCE.clone().version().await.map_err(|e| {
        error!("Failed check docker version: {}", e);
        "The docker version cannot be run"
    })?;

    Ok(version)
}

#[tauri::command]
pub async fn open_terminal(_app: AppHandle<Wry>, platform: String) -> Result<(), String> {
    let terminal_path = if let Some(home_dir_path) = home_dir() {
        match home_dir_path.into_os_string().into_string() {
            Ok(path) => path,
            Err(_oss_err) => {
                error!("Failed to convert home directory path to string");
                return Err("Failed to convert home directory path to string".to_string());
            },
        }
    } else {
        return Err("Failed to get home directory".to_string());
    };

    if platform.to_lowercase().trim() == "darwin" {
        Command::new("open")
            .arg("-a")
            .arg("Terminal")
            .arg(&terminal_path)
            .spawn()
            .map_err(|e| {
                error!("Failed to open terminal with path {}. Error: {}", terminal_path, e);
                format!("Terminal cannot be opened. cmd: open -a Terminal {}", terminal_path)
            })?;
    } else if platform.to_lowercase().trim() == "windows_nt" {
        Command::new("powershell")
            .arg("-command")
            .arg("start")
            .arg("powershell")
            .spawn()
            .map_err(|e| {
                error!("Failed to start powershell Error: {}", e);
                format!("Terminal cannot be opened. cmd: powershell -command start powershell")
            })?;
    } else if platform.to_lowercase().trim() == "linux" {
        Command::new("gnome-terminal")
            .arg("--working-directory")
            .arg(&terminal_path)
            .spawn()
            .map_err(|e| {
                error!("Failed to open terminal with path {}. Error: {}", terminal_path, e);
                format!(
                    "Terminal cannot be opened. cmd: gnome-terminal --working-directory {}",
                    terminal_path
                )
            })?;
    } else {
        return Err(format!("Unsupported platform: {}", platform));
    };

    Ok(())
}

#[tauri::command]
pub async fn check_internet_connection() -> Result<bool, String> {
    // @TODO - add https://crates.io/crates/online
    // Return 'true' if online, 'false' if offline
    Ok(true)
}
