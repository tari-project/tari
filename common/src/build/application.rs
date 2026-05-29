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
        let full_version = self.get_full_version()?;
        let version_number = self.get_version_number()?;
        let authors = get_authors(&self.manifest).join(",");
        writeln!(
            file,
            r#"#[allow(dead_code)] pub const APP_VERSION: &str = "{full_version}";"#,
        )?;
        writeln!(
            file,
            r#"#[allow(dead_code)] pub const APP_VERSION_NUMBER: &str = "{version_number}";"#,
        )?;
        writeln!(
            file,
            r#"#[allow(dead_code)] pub const APP_AUTHORS: &str = "{authors}";"#
        )?;
        Ok(out_path)
    }

    /// Add the git version commit and built type to the version number
    /// The final output looks like 0.1.2-fc435c-release
    fn get_full_version(&self) -> Result<String, anyhow::Error> {
        let build = env::var("PROFILE").unwrap_or_else(|e| {
            emit_cargo_warn(e);
            "Unknown".to_string()
        });
        Ok(format!("{}-{}-{}", self.get_version_number()?, self.commit, build))
    }

    /// Get the version number only
    /// The final output looks like 0.1.2
    fn get_version_number(&self) -> Result<String, anyhow::Error> {
        get_version_number(&self.manifest)
    }
}

/// Resolve the package version from a parsed manifest.
///
/// When called from a git checkout, `find_git_root` lands on the workspace root and the version
/// lives under `[workspace.package]`. When called from an unpacked registry crate (e.g. as a
/// build-dep of a consumer pulling tari from crates.io) there is no `.git` and no workspace
/// table — cargo strips workspace inheritance on publish and substitutes the literal value into
/// `[package].version`. Try both, in that order.
fn get_version_number(manifest: &Manifest) -> Result<String, anyhow::Error> {
    if let Some(version) = manifest
        .workspace
        .as_ref()
        .and_then(|w| w.package.as_ref())
        .and_then(|p| p.version.clone())
    {
        return Ok(version);
    }
    if let Some(version) = manifest.package.as_ref().and_then(|p| p.version.get().ok()) {
        return Ok(version.clone());
    }
    Err(anyhow::anyhow!(
        "Could not determine package version: neither [workspace.package].version nor [package].version is set"
    ))
}

/// Resolve package authors from a parsed manifest, applying the same workspace → package
/// fallback as [`get_version_number`].
fn get_authors(manifest: &Manifest) -> Vec<String> {
    if let Some(authors) = manifest
        .workspace
        .as_ref()
        .and_then(|w| w.package.as_ref())
        .and_then(|p| p.authors.as_ref())
    {
        return authors.clone();
    }
    if let Some(authors) = manifest.package.as_ref().and_then(|p| p.authors.get().ok().cloned()) {
        return authors;
    }
    Vec::new()
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
    let manifest_path = PathBuf::from(&manifest);

    let mut current = manifest_path.as_path();
    loop {
        if current.join(".git").exists() {
            return Ok(current.to_path_buf());
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => {
                emit_cargo_warn("Not a git repository — no ancestor of CARGO_MANIFEST_DIR contains a .git directory");
                return Ok(manifest_path);
            },
        }
    }
}

fn get_commit<P: AsRef<Path>>(git_root: P) -> Result<String, anyhow::Error> {
    let repo = git2::Repository::open(git_root)?;
    let head = repo.revparse_single("HEAD")?;
    let id = format!("{:?}", head.id());

    id.split_at_checked(7)
        .ok_or(anyhow::anyhow!("invalid utf8 in commit id"))?
        .0
        .to_string();
    Ok(id)
}

fn emit_cargo_warn<T: fmt::Display>(e: T) {
    println!("cargo:warning=Could not open repo: {e}");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(toml_str: &str) -> Manifest {
        toml::from_str(toml_str).expect("test manifest parses")
    }

    // Mirrors the workspace root Cargo.toml seen during an in-tree git checkout build.
    const WORKSPACE_MANIFEST: &str = r#"
[workspace.package]
version = "1.2.3"
authors = ["alice", "bob"]

[workspace]
members = []
"#;

    // Mirrors a published crate's Cargo.toml as unpacked into the cargo registry: cargo strips
    // workspace inheritance on publish and substitutes the literal values into [package].
    const REGISTRY_MANIFEST: &str = r#"
[package]
name = "demo"
version = "1.2.3"
authors = ["alice", "bob"]
edition = "2021"
"#;

    #[test]
    fn reads_version_from_workspace_package() {
        assert_eq!(get_version_number(&parse(WORKSPACE_MANIFEST)).unwrap(), "1.2.3");
    }

    #[test]
    fn falls_back_to_package_version_when_workspace_table_missing() {
        assert_eq!(get_version_number(&parse(REGISTRY_MANIFEST)).unwrap(), "1.2.3");
    }

    #[test]
    fn errors_when_no_version_is_set_anywhere() {
        let manifest: Manifest = parse(
            r#"
[workspace]
members = []
"#,
        );
        assert!(get_version_number(&manifest).is_err());
    }

    #[test]
    fn reads_authors_from_workspace_package() {
        assert_eq!(get_authors(&parse(WORKSPACE_MANIFEST)), vec![
            "alice".to_string(),
            "bob".to_string()
        ]);
    }

    #[test]
    fn falls_back_to_package_authors_when_workspace_table_missing() {
        assert_eq!(get_authors(&parse(REGISTRY_MANIFEST)), vec![
            "alice".to_string(),
            "bob".to_string()
        ]);
    }
}
