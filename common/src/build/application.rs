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

pub struct StaticApplicationInfo {
    version: String,
    authors: String,
    commit: String,
}

impl StaticApplicationInfo {
    /// Reads the version and authors cargo passes to the build script, and the git commit of the workspace the crate
    /// is built in (or "unknown" if it is not built from a git checkout).
    pub fn initialize() -> Result<Self, anyhow::Error> {
        let version = env::var("CARGO_PKG_VERSION")?;
        // Cargo joins multiple authors with ':'
        let authors = env::var("CARGO_PKG_AUTHORS").unwrap_or_default().replace(':', ",");
        let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
        let commit = match find_git_root(&manifest_dir) {
            Some(git_root) => get_commit(&git_root).unwrap_or_else(|e| {
                emit_cargo_warn(e);
                "unknown".to_string()
            }),
            None => "unknown".to_string(),
        };
        Ok(Self {
            version,
            authors,
            commit,
        })
    }

    /// Writes the consts file to the given file in the OUT_DIR. Returns the written file path.
    /// This will overwrite existing files
    pub fn write_consts_to_outdir<P: AsRef<Path>>(&self, filename: P) -> Result<PathBuf, anyhow::Error> {
        let out_dir = env::var_os("OUT_DIR").unwrap();
        let out_path = Path::new(&out_dir).join(filename);
        let mut file = fs::File::create(&out_path)?;
        writeln!(file, "{}", const_line("APP_VERSION", &self.get_full_version()))?;
        writeln!(file, "{}", const_line("APP_VERSION_NUMBER", &self.version))?;
        writeln!(file, "{}", const_line("APP_AUTHORS", &self.authors))?;
        Ok(out_path)
    }

    /// Add the git version commit and built type to the version number
    /// The final output looks like 0.1.2-fc435c-release
    fn get_full_version(&self) -> String {
        let build = env::var("PROFILE").unwrap_or_else(|e| {
            emit_cargo_warn(e);
            "Unknown".to_string()
        });
        format!("{}-{}-{}", self.version, self.commit, build)
    }
}

/// Formats a `&str` const. The value is written with `{:?}` so it is always a valid, escaped Rust string literal.
fn const_line(name: &str, value: &str) -> String {
    format!("#[allow(dead_code)] pub const {name}: &str = {value:?};")
}

/// Finds the directory to read the git commit from. The search is bounded by the workspace root: the first ancestor of
/// `manifest_dir` (including itself) whose `Cargo.toml` has a `[workspace]` table. A `.git` is only accepted at the
/// workspace root or between it and `manifest_dir`, so a crate unpacked from a registry inside some unrelated git
/// checkout does not pick up that checkout's commit.
fn find_git_root(manifest_dir: &Path) -> Option<PathBuf> {
    let workspace_root = manifest_dir.ancestors().find(|dir| is_workspace_root(dir))?;
    for dir in manifest_dir.ancestors() {
        if dir.join(".git").exists() {
            return Some(dir.to_path_buf());
        }
        if dir == workspace_root {
            break;
        }
    }
    None
}

/// Returns true if `dir/Cargo.toml` contains a `[workspace]` table header.
fn is_workspace_root(dir: &Path) -> bool {
    match fs::read_to_string(dir.join("Cargo.toml")) {
        Ok(contents) => contents.lines().any(is_workspace_header),
        Err(_) => false,
    }
}

/// Returns true if the line is the bare `[workspace]` table header, ignoring whitespace and a trailing `# comment`.
/// `[workspace.package]` and other tables do not count.
fn is_workspace_header(line: &str) -> bool {
    let line = line.trim();
    let line = match line.split_once('#') {
        Some((before_comment, _)) => before_comment,
        None => line,
    };
    line.trim() == "[workspace]"
}

fn get_commit<P: AsRef<Path>>(git_root: P) -> Result<String, anyhow::Error> {
    let repo = git2::Repository::open(git_root)?;
    let head = repo.revparse_single("HEAD")?;
    let id = head.id().to_string();

    let short = id
        .split_at_checked(7)
        .ok_or(anyhow::anyhow!("commit id is too short"))?
        .0
        .to_string();
    Ok(short)
}

fn emit_cargo_warn<T: fmt::Display>(e: T) {
    println!("cargo:warning=Could not open repo: {e}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn const_line_escapes_values() {
        let line = const_line("APP_AUTHORS", r#"a"b\c"#);
        assert_eq!(line, r#"#[allow(dead_code)] pub const APP_AUTHORS: &str = "a\"b\\c";"#);
        // The literal part round-trips back to the original value
        let literal = line
            .strip_prefix("#[allow(dead_code)] pub const APP_AUTHORS: &str = ")
            .unwrap()
            .strip_suffix(';')
            .unwrap();
        let unescaped: String = serde_json::from_str(literal).unwrap();
        assert_eq!(unescaped, r#"a"b\c"#);
    }

    #[test]
    fn workspace_header_matching() {
        assert!(is_workspace_header("[workspace]"));
        assert!(is_workspace_header("  [workspace]   "));
        assert!(is_workspace_header("[workspace] # members below"));
        assert!(!is_workspace_header("[workspace.package]"));
        assert!(!is_workspace_header("[dependencies]"));
        assert!(!is_workspace_header("# [workspace]"));

        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace.package]\nversion = \"1.0.0\"\n\n[workspace] # members below\nmembers = []\n",
        )
        .unwrap();
        assert!(is_workspace_root(dir.path()));
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace.package]\nversion = \"1.0.0\"\n",
        )
        .unwrap();
        assert!(!is_workspace_root(dir.path()));
    }

    #[test]
    fn git_root_is_bounded_by_workspace_root() {
        let tmp = tempfile::tempdir().unwrap();
        let outer = tmp.path();
        let workspace = outer.join("ws");
        let krate = workspace.join("crate");
        fs::create_dir_all(&krate).unwrap();
        fs::write(workspace.join("Cargo.toml"), "[workspace]\nmembers = []\n").unwrap();
        fs::write(krate.join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();

        // A .git above the workspace root is ignored
        fs::create_dir_all(outer.join(".git")).unwrap();
        assert_eq!(find_git_root(&krate), None);

        // A .git at the workspace root is used
        fs::create_dir_all(workspace.join(".git")).unwrap();
        assert_eq!(find_git_root(&krate), Some(workspace.clone()));
    }
}
