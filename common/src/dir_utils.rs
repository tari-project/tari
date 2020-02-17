// Copyright 2019. The Tari Project
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

use std::{io::ErrorKind, path::PathBuf};

/// Create the default data directory (`~/.tari` on OSx and Linux, for example) if it doesn't already exist
pub fn create_data_directory() -> Result<(), std::io::Error> {
    let mut home = dirs::home_dir().ok_or_else(|| std::io::Error::from(ErrorKind::NotFound))?;
    home.push(".tari");
    if !home.exists() {
        std::fs::create_dir(home)
    } else {
        Ok(())
    }
}

/// A convenience function for creating subfolders inside the `~/.tari` default data directory
///
/// # Panics
/// This function panics if the home folder location cannot be found or if the path value is not valid UTF-8.
/// This is a trade-off made in favour of convenience of use.
pub fn default_subdir(path: &str) -> String {
    let mut home = dirs::home_dir().expect("Home folder location failed");
    home.push(".tari");
    home.push(path);
    String::from(home.to_str().expect("Invalid path value"))
}

pub fn default_path(filename: &str) -> PathBuf {
    let mut home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.push(".tari");
    home.push(filename);
    home
}
