//  Copyright 2024. The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{
    env,
    fmt,
    fs,
    io::Write,
    panic,
    path::{Path, PathBuf},
};

use cargo_toml::Manifest;

pub struct StaticApplicationInfo {
    manifest: Manifest,
    commit: String,
}

impl StaticApplicationInfo {
    pub fn initialize() -> Result<Self, anyhow::Error> {
        let git_root = find_git_root()?;
        let manifest = extract_manifest(&git_root)?;
        let commit = get_commit(&git_root).unwrap_or_else(|e| {
            emit_cargo_warn(e);
            "NoGitRepository".to_string()
        });
        Ok(Self { manifest, commit })
    }

    /// Writes the consts file to the given file in the OUT_DIR. Returns the written file path.
    /// This will overwrite existing files
    pub fn write_consts_to_outdir<P: AsRef<Path>>(&self, filename: P) -> Result<PathBuf, anyhow::Error> {
        let out_dir = env::var_os("OUT_DIR").unwrap();
        let out_path = Path::new(&out_dir).join(filename);
        let mut file = fs::File::create(&out_path)?;
        writeln!(
            file,
            r#"#[allow(dead_code)] pub const APP_VERSION: &str = "{}";"#,
            self.get_full_version()
        )?;
        writeln!(
            file,
            r#"#[allow(dead_code)] pub const APP_VERSION_NUMBER: &str = "{}";"#,
            self.get_version_number()
        )?;
        writeln!(
            file,
            r#"#[allow(dead_code)] pub const APP_AUTHORS: &str = "{}";"#,
            self.manifest
                .workspace
                .as_ref()
                .and_then(|w| w.package.as_ref())
                .and_then(|p| p.authors.as_ref())
                .map(|a| a.join(","))
                .unwrap_or_default()
        )?;
        Ok(out_path)
    }

    /// Add the git version commit and built type to the version number
    /// The final output looks like 0.1.2-fc435c-release
    fn get_full_version(&self) -> String {
        let build = env::var("PROFILE").unwrap_or_else(|e| {
            emit_cargo_warn(e);
            "Unknown".to_string()
        });
        format!("{}-{}-{}", self.get_version_number(), self.commit, build)
    }

    /// Get the version number only
    /// The final output looks like 0.1.2
    fn get_version_number(&self) -> String {
        self.manifest
            .workspace
            .as_ref()
            .and_then(|w| w.package.as_ref())
            .and_then(|p| p.version.clone())
            .unwrap_or_default()
    }
}

fn extract_manifest<P: AsRef<Path>>(git_root: P) -> Result<Manifest, anyhow::Error> {
    let cargo_path = git_root.as_ref().join("Cargo.toml");
    let cargo = fs::read(cargo_path)?;
    let cargo = std::str::from_utf8(&cargo)?;
    let manifest = toml::from_str(cargo)?;
    Ok(manifest)
}

fn find_git_root() -> Result<PathBuf, anyhow::Error> {
    let manifest = env::var("CARGO_MANIFEST_DIR")?;
    let mut path = PathBuf::from(manifest);

    let mut loop_count = 0;
    while !path.join(".git").exists() {
        path = path.join("..");
        if loop_count == 10 {
            return Err(anyhow::anyhow!(
                "Not a git repository or CARGO_MANIFEST_DIR nested deeper than 10 from the root"
            ));
        }
        loop_count += 1;
    }

    Ok(path)
}

fn get_commit<P: AsRef<Path>>(git_root: P) -> Result<String, anyhow::Error> {
    let repo = git2::Repository::open(git_root)?;
    let head = repo.revparse_single("HEAD")?;
    let id = format!("{:?}", head.id());
    let result = panic::catch_unwind(|| id.split_at(7));
    let id = match result {
        Ok((first, _)) => first.to_string(),
        Err(_) => return Err(anyhow::anyhow!("invalid utf8 in commit id")),
    };

    // replace after stable 1.80 release
    // id.split_at_checked(7)
    //     .ok_or(anyhow::anyhow!("invalid utf8 in commit id"))?
    //     .0
    //     .to_string();
    Ok(id)
}

fn emit_cargo_warn<T: fmt::Display>(e: T) {
    println!("cargo:warning=Could not open repo: {}", e);
}
