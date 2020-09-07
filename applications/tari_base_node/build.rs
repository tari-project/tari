// Copyright 2020. The Tari Project
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

use serde::Deserialize;
use std::{env, fs, path::Path, string::ToString};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    write_constants_file();
    Ok(())
}

#[derive(Deserialize)]
struct Package {
    authors: Vec<String>,
    version: String,
}

#[derive(Deserialize)]
struct Manifest {
    pub package: Package,
}

fn write_constants_file() {
    let data = extract_manifest();
    let mut package = data.package;
    package.version = full_version(&package.version);
    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("consts.rs");
    let output = format!(
        r#"
    pub const VERSION: &str = "{}";
    pub const AUTHOR: &str = "{}";
    "#,
        package.version,
        package.authors.join(",")
    );
    fs::write(&dest_path, output.as_bytes()).unwrap();
}

fn extract_manifest() -> Manifest {
    let cargo_path = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("Cargo.toml");
    let cargo = fs::read(cargo_path).expect("Could not read Cargo.toml");
    let cargo = std::str::from_utf8(&cargo).unwrap();
    toml::from_str(&cargo).unwrap()
}

/// Add the git version commit and built type to the version number
/// The final output looks like 0.1.2-fc435c-release
fn full_version(ver: &str) -> String {
    let sha = get_commit();
    let build = env::var("PROFILE").unwrap_or_else(|_| "Unknown".to_string());
    format!("{}-{}-{}", ver, sha, build)
}

#[allow(clippy::let_and_return)]
fn get_commit() -> String {
    let path = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("..")
        .join("..");
    let repo = match git2::Repository::open(&path) {
        Ok(r) => r,
        Err(e) => {
            println!("cargo:warning=Could not open repo: {}", e.to_string());
            return "NoGitRepository".to_string();
        },
    };
    let result = match repo.revparse_single("HEAD") {
        Ok(head) => {
            let id = format!("{:?}", head.id());
            id.split_at(7).0.to_string()
        },
        Err(e) => {
            println!("cargo:warning=Could not find latest commit: {}", e.to_string());
            String::from("NoGitRepository")
        },
    };
    result
}
