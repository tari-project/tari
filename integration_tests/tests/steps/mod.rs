//   Copyright 2023. The Tari Project
//
//   Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//   following conditions are met:
//
//   1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//   disclaimer.
//
//   2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//   following disclaimer in the documentation and/or other materials provided with the distribution.
//
//   3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//   products derived from this software without specific prior written permission.
//
//   THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//   INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//   DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//   SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//   SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//   WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//   USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{
    fs::OpenOptions,
    io::{BufRead, Write},
    path::PathBuf,
    time::Duration,
};

use chrono::Local;
use cucumber::{then, when};
use tari_integration_tests::TariWorld;

pub mod merge_mining_steps;
pub mod mining_steps;
pub mod node_steps;
pub mod wallet_cli_steps;
pub mod wallet_ffi_steps;
pub mod wallet_steps;

pub const CONFIRMATION_PERIOD: u64 = 4;
pub const TWO_MINUTES_WITH_HALF_SECOND_SLEEP: u64 = 240;
pub const HALF_SECOND: u64 = 500;

#[when(expr = "I wait {int} seconds")]
async fn wait_seconds(_world: &mut TariWorld, seconds: u64) {
    tokio::time::sleep(Duration::from_secs(seconds)).await;
}

#[then(regex = r"I receive an error containing '(.*)'")]
async fn receive_an_error(world: &mut TariWorld, error: String) {
    match world.errors.back() {
        Some(err) => assert_eq!(err, &error),
        None => panic!("Should have received an error"),
    };

    // No-op.
    // Was not implemented in previous suite, gave it a quick try but missing other peices

    // assert!(world.errors.len() > 1);
    // assert!(world.errors.pop_front().unwrap().contains(&error))
}

pub fn cucumber_steps_log<S: AsRef<str>>(log_message: S) {
    let log_file = PathBuf::from("./log/steps.log");
    let mut file = OpenOptions::new().create(true).append(true).open(log_file).unwrap();
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    writeln!(file, "[{}] {}", timestamp, log_message.as_ref()).unwrap();
}

pub fn get_saved_seed_words(world: &mut TariWorld, wallet_name: &str) -> Vec<String> {
    let source_wallet = world
        .get_wallet(&wallet_name)
        .unwrap_or_else(|e| panic!("Wallet process '{}' does not exist in world: {}", wallet_name, e));
    let seed_words_path = source_wallet.temp_dir_path.clone().join("seed_words.txt");
    let seed_words_file = std::fs::File::open(&seed_words_path)
        .unwrap_or_else(|e| panic!("Failed to open seed words file at {:?}: {}", seed_words_path, e));
    let reader = std::io::BufReader::new(seed_words_file);
    let line = reader
        .lines()
        .next()
        .unwrap_or_else(|| panic!("Seed words file at {:?} is empty", seed_words_path))
        .unwrap_or_else(|e| panic!("Failed to read seed words from file: {}", e));
    line.split_whitespace()
        .collect::<Vec<_>>()
        .into_iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
}
