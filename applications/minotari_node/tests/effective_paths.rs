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

        let line = matching_lines.first().expect("matching line");
        let path_part = line.split(prefix).nth(1).expect("prefix missing");
        let path_str = path_part.trim();
        let path = Path::new(path_str);
        assert!(
            path.is_absolute(),
            "Path for {:?} must be absolute, got {:?}",
            prefix,
            path_str
        );
    }
}
