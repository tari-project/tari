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

use std::{path::PathBuf, sync::OnceLock};

use path_clean::PathClean;

/// Records the resolved absolute fallback directory if the user home directory could not be determined.
static FALLBACK_HOME_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Create the default data directory (`~/.tari` on OSx and Linux, for example) if it doesn't already exist
pub fn create_data_directory(base_dir: Option<&PathBuf>) -> Result<(), std::io::Error> {
    let home = default_path("", base_dir);

    if home.exists() {
        Ok(())
    } else {
        println!("Creating {home:?}");
        std::fs::create_dir_all(home)
    }
}

/// A convenience function for creating subfolders inside the `~/.tari` default data directory
///
/// # Panics
/// This function panics if the home folder location cannot be found or if the path value is not valid UTF-8.
/// This is a trade-off made in favour of convenience of use.
pub fn default_subdir(path: &str, base_dir: Option<&PathBuf>) -> String {
    let home = default_path(path, base_dir);
    String::from(home.to_str().expect("Invalid path value"))
}

pub fn default_path(filename: &str, base_path: Option<&PathBuf>) -> PathBuf {
    default_path_with_home_fn(filename, base_path, dirs_next::home_dir, &FALLBACK_HOME_DIR)
}

pub(crate) fn default_path_with_home_fn<F: FnOnce() -> Option<PathBuf>>(
    filename: &str,
    base_path: Option<&PathBuf>,
    home_fn: F,
    fallback_slot: &OnceLock<PathBuf>,
) -> PathBuf {
    let mut home = base_path.cloned().unwrap_or_else(|| {
        let home_dir = home_fn().unwrap_or_else(|| {
            let fallback = PathBuf::from(".");
            fallback_slot.get_or_init(|| absolute_path(&fallback));
            fallback
        });
        [home_dir, PathBuf::from(".tari")].iter().collect()
    });
    for component in PathBuf::from(filename).components() {
        home.push(component);
    }
    home
}

/// Returns the absolute path of the fallback directory if an unset home directory fallback occurred.
pub fn get_fallback_home_dir() -> Option<PathBuf> {
    FALLBACK_HOME_DIR.get().cloned()
}

pub fn absolute_path<P>(path: P) -> PathBuf
where P: AsRef<std::path::Path> {
    let path = path.as_ref();
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    }
    .clean()
}

#[cfg(test)]
mod test {
    use std::{path::PathBuf, sync::OnceLock};

    use tari_test_utils::random::string;
    use tempfile::tempdir;

    use crate::dir_utils;

    #[test]
    fn test_multiple_levels_create_data_directory() {
        let temp_dir = tempdir().unwrap();
        let dir = &PathBuf::from(
            temp_dir.path().to_path_buf().display().to_string() +
                "/" +
                &(0..12)
                    .collect::<Vec<usize>>()
                    .iter()
                    .map(|_| string(2))
                    .collect::<Vec<std::string::String>>()
                    .join("/") +
                "/",
        );

        assert!(!std::path::Path::new(&dir.display().to_string()).exists());
        dir_utils::create_data_directory(Some(dir)).unwrap();
        assert!(std::path::Path::new(&dir.display().to_string()).exists());
    }

    #[test]
    fn test_absolute_path_from_relative_path() {
        let current_path = std::env::current_dir().unwrap_or_default();
        let relative_path = PathBuf::from("./01/02/");
        let joined_path = current_path.join(&relative_path);
        assert_eq!(dir_utils::absolute_path(&relative_path), joined_path);
    }

    #[test]
    fn test_default_path_unset_home_records_fallback_once() {
        let slot = OnceLock::new();
        assert_eq!(slot.get(), None);

        let fallback_abs = dir_utils::absolute_path(".");

        let path1 = dir_utils::default_path_with_home_fn("test_file", None, || None, &slot);
        assert_eq!(path1, PathBuf::from("./.tari/test_file"));
        assert_eq!(slot.get(), Some(&fallback_abs));

        // Second call should preserve the first recorded fallback
        let path2 = dir_utils::default_path_with_home_fn("another_file", None, || None, &slot);
        assert_eq!(path2, PathBuf::from("./.tari/another_file"));
        assert_eq!(slot.get(), Some(&fallback_abs));
    }

    #[test]
    fn test_default_path_with_explicit_base_path() {
        let slot = OnceLock::new();
        let base = PathBuf::from("/custom/base");
        let path = dir_utils::default_path_with_home_fn("file.txt", Some(&base), || None, &slot);
        assert_eq!(path, PathBuf::from("/custom/base/file.txt"));
        assert_eq!(slot.get(), None);
    }
}
