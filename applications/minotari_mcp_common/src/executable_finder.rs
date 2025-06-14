//! Smart executable discovery for Tari applications
//!
//! Provides intelligent discovery of Tari executables using PATH, relative paths,
//! build directories, and environment variables. Includes validation and
//! permission checking for found executables.

use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

use crate::error::{McpError, McpResult};

/// Executable finder for Tari applications
pub struct ExecutableFinder {
    executable_name: String,
    search_paths: Vec<PathBuf>,
    build_directories: Vec<String>,
}

impl ExecutableFinder {
    /// Create a new executable finder
    pub fn new(executable_name: &str) -> Self {
        Self {
            executable_name: executable_name.to_string(),
            search_paths: Self::default_search_paths(),
            build_directories: Self::default_build_directories(),
        }
    }

    /// Add custom search paths
    pub fn with_search_paths(mut self, paths: Vec<PathBuf>) -> Self {
        self.search_paths.extend(paths);
        self
    }

    /// Add custom build directories
    pub fn with_build_directories(mut self, dirs: Vec<String>) -> Self {
        self.build_directories.extend(dirs);
        self
    }

    /// Find the executable with comprehensive search strategy
    pub fn find(&self) -> McpResult<PathBuf> {
        // Strategy 1: Check PATH environment variable
        if let Ok(path) = self.find_in_path() {
            log::debug!("Found {} in PATH: {}", self.executable_name, path.display());
            return Ok(path);
        }

        // Strategy 2: Check current directory
        if let Ok(path) = self.find_in_current_directory() {
            log::debug!(
                "Found {} in current directory: {}",
                self.executable_name,
                path.display()
            );
            return Ok(path);
        }

        // Strategy 3: Check relative paths
        if let Ok(path) = self.find_in_relative_paths() {
            log::debug!("Found {} in relative paths: {}", self.executable_name, path.display());
            return Ok(path);
        }

        // Strategy 4: Check build directories
        if let Ok(path) = self.find_in_build_directories() {
            log::debug!(
                "Found {} in build directories: {}",
                self.executable_name,
                path.display()
            );
            return Ok(path);
        }

        // Strategy 5: Check custom search paths
        if let Ok(path) = self.find_in_search_paths() {
            log::debug!("Found {} in search paths: {}", self.executable_name, path.display());
            return Ok(path);
        }

        Err(McpError::config_error(format!(
            "Could not find executable '{}'. Searched in:\n{}\n\n{}",
            self.executable_name,
            self.generate_search_summary(),
            self.generate_suggestions()
        )))
    }

    /// Find executable in PATH
    fn find_in_path(&self) -> McpResult<PathBuf> {
        which::which(&self.executable_name).map_err(|_| McpError::config_error("Not found in PATH"))
    }

    /// Find executable in current directory
    fn find_in_current_directory(&self) -> McpResult<PathBuf> {
        let current_dir = env::current_dir()
            .map_err(|e| McpError::config_error(format!("Cannot access current directory: {}", e)))?;

        let candidate = current_dir.join(&self.executable_name);
        self.validate_executable(&candidate)
    }

    /// Find executable in relative paths
    fn find_in_relative_paths(&self) -> McpResult<PathBuf> {
        let relative_paths = [
            format!("./{}", self.executable_name),
            format!("../{}", self.executable_name),
            format!("./target/release/{}", self.executable_name),
            format!("./target/debug/{}", self.executable_name),
        ];

        for rel_path in &relative_paths {
            let path = Path::new(rel_path);
            if let Ok(validated) = self.validate_executable(path) {
                return Ok(validated);
            }
        }

        Err(McpError::config_error("Not found in relative paths"))
    }

    /// Find executable in build directories
    fn find_in_build_directories(&self) -> McpResult<PathBuf> {
        for build_dir in &self.build_directories {
            let candidate = Path::new(build_dir).join(&self.executable_name);
            if let Ok(validated) = self.validate_executable(&candidate) {
                return Ok(validated);
            }
        }

        Err(McpError::config_error("Not found in build directories"))
    }

    /// Find executable in custom search paths
    fn find_in_search_paths(&self) -> McpResult<PathBuf> {
        for search_path in &self.search_paths {
            let candidate = search_path.join(&self.executable_name);
            if let Ok(validated) = self.validate_executable(&candidate) {
                return Ok(validated);
            }
        }

        Err(McpError::config_error("Not found in search paths"))
    }

    /// Validate that a path is an executable file
    fn validate_executable(&self, path: &Path) -> McpResult<PathBuf> {
        if !path.exists() {
            return Err(McpError::config_error(format!(
                "Path does not exist: {}",
                path.display()
            )));
        }

        if !path.is_file() {
            return Err(McpError::config_error(format!(
                "Path is not a file: {}",
                path.display()
            )));
        }

        // Check if file is executable (Unix-like systems)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = path
                .metadata()
                .map_err(|e| McpError::config_error(format!("Cannot read file metadata: {}", e)))?;

            let permissions = metadata.permissions();
            if permissions.mode() & 0o111 == 0 {
                return Err(McpError::config_error(format!(
                    "File is not executable: {}",
                    path.display()
                )));
            }
        }

        // Try to get version info to verify it's a valid Tari executable
        if let Err(e) = self.verify_executable(path) {
            log::warn!("Executable verification failed for {}: {}", path.display(), e);
            // Continue anyway - the verification might fail for various reasons
        }

        Ok(path.to_path_buf())
    }

    /// Verify that the executable is a valid Tari application
    fn verify_executable(&self, path: &Path) -> McpResult<()> {
        // Try to run the executable with --version flag
        match Command::new(path).arg("--version").output() {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                // Check if it's a Tari executable by looking for "tari" or "minotari" in version output
                if stdout.to_lowercase().contains("tari") ||
                    stdout.to_lowercase().contains("minotari") ||
                    stderr.to_lowercase().contains("tari") ||
                    stderr.to_lowercase().contains("minotari")
                {
                    log::debug!("Verified Tari executable: {}", stdout.trim());
                    Ok(())
                } else {
                    Err(McpError::config_error(format!(
                        "Executable does not appear to be a Tari application: {}",
                        stdout.trim()
                    )))
                }
            },
            Err(e) => Err(McpError::config_error(format!(
                "Cannot execute --version command: {}",
                e
            ))),
        }
    }

    /// Generate a summary of searched locations
    fn generate_search_summary(&self) -> String {
        let mut summary = Vec::new();

        summary.push("- PATH environment variable".to_string());
        summary.push("- Current directory".to_string());
        summary.push("- Relative paths (./target/release, ./target/debug, etc.)".to_string());

        for build_dir in &self.build_directories {
            summary.push(format!("- Build directory: {}", build_dir));
        }

        for search_path in &self.search_paths {
            summary.push(format!("- Search path: {}", search_path.display()));
        }

        summary.join("\n")
    }

    /// Generate helpful suggestions for the user
    fn generate_suggestions(&self) -> String {
        format!(
            "Suggestions:\n1. Install {} using 'cargo install minotari'\n2. Build the project with 'cargo build \
             --release'\n3. Add the executable directory to your PATH\n4. Specify the full path to the executable\n5. \
             Set MINOTARI_EXECUTABLE_PATH environment variable",
            self.executable_name
        )
    }

    /// Default search paths
    fn default_search_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();

        // Add common installation directories
        if let Some(home) = env::var_os("HOME") {
            paths.push(PathBuf::from(home.clone()).join(".cargo/bin"));
            paths.push(PathBuf::from(home).join("bin"));
        }

        // Add system directories
        paths.push(PathBuf::from("/usr/local/bin"));
        paths.push(PathBuf::from("/usr/bin"));

        // Add environment-specific paths
        if let Ok(minotari_path) = env::var("MINOTARI_EXECUTABLE_PATH") {
            paths.push(PathBuf::from(minotari_path));
        }

        paths
    }

    /// Default build directories relative to current location
    fn default_build_directories() -> Vec<String> {
        vec![
            "./target/release".to_string(),
            "./target/debug".to_string(),
            "../target/release".to_string(),
            "../target/debug".to_string(),
            "../../target/release".to_string(),
            "../../target/debug".to_string(),
        ]
    }
}

/// Convenience functions for common Tari executables
pub struct TariExecutables;

impl TariExecutables {
    /// Find minotari_node executable
    pub fn find_node() -> McpResult<PathBuf> {
        ExecutableFinder::new("minotari_node").find()
    }

    /// Find minotari_console_wallet executable
    pub fn find_wallet() -> McpResult<PathBuf> {
        ExecutableFinder::new("minotari_console_wallet").find()
    }

    /// Find minotari_merge_mining_proxy executable
    pub fn find_merge_mining_proxy() -> McpResult<PathBuf> {
        ExecutableFinder::new("minotari_merge_mining_proxy").find()
    }

    /// Find custom Tari executable with specific name
    pub fn find_custom(name: &str) -> McpResult<PathBuf> {
        ExecutableFinder::new(name).find()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_executable_finder_creation() {
        let finder = ExecutableFinder::new("test_executable");
        assert_eq!(finder.executable_name, "test_executable");
        assert!(!finder.search_paths.is_empty());
        assert!(!finder.build_directories.is_empty());
    }

    #[test]
    fn test_custom_search_paths() {
        let custom_paths = vec![PathBuf::from("/custom/path")];
        let finder = ExecutableFinder::new("test").with_search_paths(custom_paths.clone());

        assert!(finder.search_paths.contains(&PathBuf::from("/custom/path")));
    }

    #[test]
    fn test_custom_build_directories() {
        let custom_dirs = vec!["./custom_build".to_string()];
        let finder = ExecutableFinder::new("test").with_build_directories(custom_dirs.clone());

        assert!(finder.build_directories.contains(&"./custom_build".to_string()));
    }

    #[test]
    fn test_default_paths_include_cargo() {
        let paths = ExecutableFinder::default_search_paths();
        let has_cargo_bin = paths.iter().any(|p| p.to_string_lossy().contains(".cargo/bin"));
        assert!(has_cargo_bin || env::var_os("HOME").is_none());
    }
}
