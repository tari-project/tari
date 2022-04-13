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

//! This module does setup and clean up of the host's file system.
//!
//! The docker environment places some files in a volume, e.g. the blockchain db, but other files need to be accessible
//! from the host system, such as logs, and identity files. These all live in a 'workspace' folder, with several sub-
//! directories for the individual components living under it.

use std::{fs, path::Path};

use log::*;
use strum::IntoEnumIterator;

use crate::docker::{DockerWrapperError, ImageType};

/// Creates the folders required for a new workspace off of the given root folder.
/// IF the folders already exist, then nothing happens.
/// On Linux, the permissions are also set to allow world access
pub fn create_workspace_folders<P: AsRef<Path>>(root: P) -> Result<(), DockerWrapperError> {
    if !root.as_ref().exists() {
        info!("Creating new workspace at {}", root.as_ref().to_str().unwrap_or("???"));
        fs::create_dir_all(&root)?;
    }
    let make_subfolder = |folder: &str| -> Result<(), std::io::Error> {
        let p = root.as_ref().join(folder);
        let p_str = p.as_path().to_str().unwrap_or("???");
        if p.exists() {
            info!("{} already exists", p_str);
            Ok(())
        } else {
            info!("Creating new data folder, {}", p_str);
            fs::create_dir_all(&p)?;
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = std::fs::metadata(&p)?.permissions();
                perms.set_mode(0o777);
                fs::set_permissions(&p, perms)?;
            }
            Ok(())
        }
    };
    info!("Making config folder");
    make_subfolder("config")?;
    for image in ImageType::iter() {
        debug!("Making folder for image:{:?}", image);
        make_subfolder(image.data_folder())?;
    }
    Ok(())
}
