// Copyright 2026. The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{fs, path::Path, process::Command};

use tempfile::tempdir;

#[test]
fn test_effective_paths_are_logged_and_absolute() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path().to_path_buf();

    let output = Command::new(env!("CARGO_BIN_EXE_minotari_node"))
        .arg("--init")
        .arg("--non-interactive-mode")
        .arg("--network")
        .arg("localnet")
        .arg("--base-path")
        .arg(&base_path)
        .output()
        .expect("failed to run minotari_node");

    assert!(
        output.status.success(),
        "minotari_node --init failed with code {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let log_file = base_path
        .join("localnet")
        .join("log")
        .join("base_node")
        .join("base_layer.log");
    assert!(log_file.exists(), "Expected log file at {:?}", log_file);
    let log_content = fs::read_to_string(&log_file).unwrap();

    let expected_prefixes = [
        "Effective base path: ",
        "Effective config path: ",
        "Effective log config path: ",
        "Effective identity file: ",
        "Effective Tor identity file: ",
    ];

    for prefix in expected_prefixes {
        let matching_lines: Vec<_> = log_content.lines().filter(|l| l.contains(prefix)).collect();
        assert_eq!(
            matching_lines.len(),
            1,
            "Expected exactly 1 line with prefix {:?}, found {:?} in log:\n{}",
            prefix,
            matching_lines,
            log_content
        );

        let line = matching_lines.first().expect("matching line missing");
        let after_prefix = line.split(prefix).nth(1).expect("prefix missing");
        let path_str = after_prefix
            .rsplit_once(" // ")
            .map_or(after_prefix, |(msg_part, _)| msg_part)
            .trim();
        let path = Path::new(path_str);
        assert!(
            path.is_absolute(),
            "Path for {:?} must be absolute, got {:?}",
            prefix,
            path_str
        );
        assert!(
            path.starts_with(&base_path),
            "Path for {:?} should be rooted under {:?}, got {:?}",
            prefix,
            base_path,
            path_str
        );
    }

    // Verify created config message was logged
    assert!(
        log_content.contains("Created default configuration file at "),
        "Log should contain 'Created default configuration file at '\n{}",
        log_content
    );

    // Verify HOME-fallback warning does NOT fire when an explicit base path is provided
    assert!(
        !log_content.contains("Could not determine user home directory; falling back to current working directory"),
        "HOME-fallback warning should not fire when explicit --base-path is provided"
    );
    let stderr_str = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr_str.contains("Could not determine user home directory"),
        "HOME-fallback warning should not appear in stderr when explicit --base-path is provided"
    );
}

#[test]
fn test_unset_home_with_explicit_base_path_emits_no_warning() {
    const WARNING_PREFIX: &str = "Could not determine user home directory; falling back to current working directory: ";

    let work_dir = tempdir().unwrap();
    let explicit_dir = tempdir().unwrap();
    let explicit_output = Command::new(env!("CARGO_BIN_EXE_minotari_node"))
        .current_dir(work_dir.path())
        .env_remove("HOME")
        .env_remove("USERPROFILE")
        .env_remove("TARI_BASE_DIR")
        .arg("--init")
        .arg("--non-interactive-mode")
        .arg("--network")
        .arg("localnet")
        .arg("--base-path")
        .arg(explicit_dir.path())
        .output()
        .expect("failed to run minotari_node with explicit --base-path");

    assert!(
        explicit_output.status.success(),
        "minotari_node --init with --base-path failed with code {:?}",
        explicit_output.status.code()
    );

    let explicit_log_file = explicit_dir
        .path()
        .join("localnet")
        .join("log")
        .join("base_node")
        .join("base_layer.log");
    assert!(
        explicit_log_file.exists(),
        "Expected explicit log file at {:?}",
        explicit_log_file
    );
    let explicit_log = fs::read_to_string(&explicit_log_file).unwrap();
    assert!(
        !explicit_log.contains(WARNING_PREFIX),
        "Fallback warning must not appear in log when --base-path is set:\n{}",
        explicit_log
    );
    let explicit_stderr = String::from_utf8_lossy(&explicit_output.stderr);
    assert!(
        !explicit_stderr.contains(WARNING_PREFIX),
        "Fallback warning must not appear on stderr when --base-path is set:\n{}",
        explicit_stderr
    );
}
