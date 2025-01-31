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

use std::{
    env,
    ffi::OsStr,
    fs::{metadata, remove_dir_all, remove_file, File},
    io::Read,
    path::PathBuf,
    process,
};

use log::*;
use serde::{Deserialize, Serialize};
const LOG_TARGET: &str = "tari_libtor_temp_files";

/// Libtor temporary directory
#[derive(Serialize, Deserialize)]
pub struct LibTorTempDir {
    pub path: PathBuf,
}

/// Returns a unique libtore handshake file name; this is required where the main process exits immediately by
/// way of 'process::exit'
pub fn libtor_handshake_file_name() -> PathBuf {
    let current_exe = env::current_exe().unwrap_or_else(|_| PathBuf::new());
    let exe_name = current_exe
        .file_name()
        .unwrap_or_else(|| OsStr::new(""))
        .to_str()
        .unwrap_or("");

    env::temp_dir().join(format!("libtor_tempdir_handshake_{}_{}.txt", exe_name, process::id()))
}

/// Remove the libtor temp files by using a handshake file that stores the tempdir path; this is required where the
/// main process exits immediately by way of 'process::exit'.
pub fn remove_libtor_temp_files() {
    if metadata(libtor_handshake_file_name()).is_ok() {
        // Read the file and remove the tempdir
        if let Ok(mut file) = File::open(libtor_handshake_file_name()) {
            let mut contents = String::new();
            if file.read_to_string(&mut contents).is_ok() {
                if let Ok(data) = serde_json::from_str::<LibTorTempDir>(&contents) {
                    if let Err(e) = remove_dir_all(data.path.clone()) {
                        error!(
                            target: LOG_TARGET,
                            "temporary files '{}' could not be removed: '{}'", data.path.display(), e
                        );
                    } else {
                        trace!(
                            target: LOG_TARGET,
                            "removed temporary files in '{}'", data.path.display()
                        );
                    }
                }
            }
        }
        if let Err(e) = remove_file(libtor_handshake_file_name()) {
            warn!(
                target: LOG_TARGET,
                "could not remove handshake file '{}' ({})", libtor_handshake_file_name().display(), e
            );
        }
    } else {
        trace!(
            target: LOG_TARGET,
            "nothing to clean, handshake file '{}' does not exist", libtor_handshake_file_name().display()
        );
    }
}
