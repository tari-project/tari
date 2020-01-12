// Copyright 2019, The Tari Project
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

use crate::random;
use std::{
    env::temp_dir,
    fs,
    path::{Path, PathBuf},
};
use tempdir::TempDir;

pub const RELATIVE_TARI_PATH: &str = "tari-tests/";

pub fn create_temporary_data_path() -> PathBuf {
    let path = temp_tari_path().join(random::prefixed_string("data-", 20));
    fs::create_dir_all(&path).unwrap();
    path
}

pub fn with_temp_dir<F, R>(f: F) -> R
where F: FnOnce(&Path) -> R {
    let tmp = TempDir::new("tari-test").unwrap();
    let r = f(&tmp.path());
    drop(tmp);
    r
}

pub fn temp_tari_path() -> PathBuf {
    temp_dir().join(RELATIVE_TARI_PATH)
}

pub fn cargo_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[cfg(test)]
mod test {
    use std::{
        path::{Path, PathBuf},
        str::FromStr,
    };

    #[test]
    fn with_temp_dir() {
        let path = super::with_temp_dir(|path| {
            assert!(Path::exists(path));
            path.to_str().unwrap().to_string()
        });

        assert_eq!(PathBuf::from_str(&path).unwrap().exists(), false);
    }
}
