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
    env,
    path::{Path, PathBuf},
    str::FromStr,
};

use log::*;
use tauri::{
    api::path::{resolve_path, BaseDirectory},
    AppHandle,
    Config,
    Manager,
    PackageInfo,
    Wry,
};

use crate::{commands::AppState, docker::create_workspace_folders, error::LauncherError};

/// Create a new workspace environment by creating a folder hierarchy (if required) at the `root_folder`, and copying
/// the default config files into it.
#[tauri::command]
pub fn create_new_workspace(app: AppHandle<Wry>, root_path: Option<String>) -> Result<(), String> {
    let config = app.config();
    let package_info = &app.state::<AppState>().package_info;
    let path = root_path
        .as_ref()
        .map(|r| PathBuf::from_str(r.as_str()).unwrap())
        .unwrap_or_else(|| env::temp_dir().join("tari"));
    debug!("Creating workspace at {:?}", path);
    create_workspace_folders(&path).map_err(|e| e.chained_message())?;
    dbg!("hello");
    copy_config_file(path.as_path(), config.as_ref(), package_info, "log4rs.yml").map_err(|e| e.chained_message())?;
    copy_config_file(path.as_path(), config.as_ref(), package_info, "config.toml").map_err(|e| e.chained_message())?;
    info!("Workspace at {:?} complete!", path);
    Ok(())
}

pub fn copy_config_file<S: AsRef<Path>>(
    root_path: S,
    config: &Config,
    package_info: &PackageInfo,
    file: &str,
) -> Result<(), LauncherError> {
    let path = Path::new("assets").join(file);
    let config_path = resolve_path(
        config,
        package_info,
        &Default::default(),
        &path,
        Some(BaseDirectory::Resource),
    )?;
    let cfg = std::fs::read_to_string(&config_path).expect("The config assets were not bundled with the App");
    info!("Log Configuration file ({}) loaded", file);
    debug!("{}", cfg);
    let dest = root_path.as_ref().join("config").join(file);
    std::fs::write(&dest, &cfg)?;
    info!("Log configuration file ({}) saved to workspace", file);
    Ok(())
}
