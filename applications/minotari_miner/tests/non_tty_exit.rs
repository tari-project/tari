// Copyright 2026. The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use clap::Parser;
use minotari_miner::{Cli, run_miner};
use tari_common::exit_codes::ExitCode;

#[test]
fn miner_exits_promptly_when_stdin_is_null_and_payment_address_is_default() {
    let temp_dir = std::env::temp_dir().join(format!("tari_miner_non_tty_test_{}", std::process::id()));
    let _unused = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).expect("create temp base dir");

    let start = Instant::now();
    let mut child = Command::new(env!("CARGO_BIN_EXE_minotari_miner"))
        .arg("--base-path")
        .arg(&temp_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn minotari_miner with /dev/null stdin");

    let timeout = Duration::from_secs(5);
    let poll_interval = Duration::from_millis(50);
    let mut status = None;
    while start.elapsed() < timeout {
        if let Some(s) = child.try_wait().expect("try_wait on child") {
            status = Some(s);
            break;
        }
        std::thread::sleep(poll_interval);
    }

    if status.is_none() {
        let _unused = child.kill();
        let _unused = child.wait();
        let _unused = std::fs::remove_dir_all(&temp_dir);
        panic!("miner hung without a TTY instead of exiting promptly within {timeout:?}");
    }

    let output = child.wait_with_output().expect("wait_with_output on child");
    let elapsed = start.elapsed();

    let _unused = std::fs::remove_dir_all(&temp_dir);

    assert!(
        !output.status.success(),
        "miner should fail when started without a TTY and default payment address"
    );
    assert!(
        elapsed < Duration::from_secs(5),
        "miner should exit promptly without spinning, took {elapsed:?}"
    );
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("wallet_payment_address"),
        "expected output to mention wallet_payment_address, got:\n{combined}"
    );
}

#[tokio::test]
async fn start_miner_non_interactive_mode_rejects_default_payment_address() {
    let temp_dir = std::env::temp_dir().join(format!("tari_miner_ni_flag_test_{}", std::process::id()));
    let _unused = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).expect("create temp base dir");

    let cli = Cli::parse_from([
        "minotari_miner",
        "--base-path",
        temp_dir.to_str().expect("utf8 path"),
        "--non-interactive-mode",
    ]);
    let err = run_miner(cli)
        .await
        .expect_err("non-interactive mode with default payment address must fail");
    let _unused = std::fs::remove_dir_all(&temp_dir);

    assert!(matches!(err.exit_code, ExitCode::WalletPaymentAddress));
    assert!(
        err.details
            .as_deref()
            .unwrap_or_default()
            .contains("wallet_payment_address"),
        "unexpected error details: {err:?}"
    );
}
