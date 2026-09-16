//   Copyright 2022. The Tari Project
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
    collections::HashMap,
    fs,
    io,
    path::PathBuf,
    str::{self},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use cucumber::{
    World as _,
    WriterExt,
    event::ScenarioFinished,
    writer::{self, Verbosity},
};
use log::*;
use tari_common::initialize_logging;
use tari_integration_tests::TariWorld;
use tokio::runtime::Runtime;

pub mod steps;

pub const LOG_TARGET: &str = "cucumber";
pub const LOG_TARGET_STDOUT: &str = "stdout";

/// Default number of scenarios to run at once. Overridable with `CUCUMBER_CONCURRENCY` so that a
/// local run and CI can use the same number — a mismatch between the two makes CI-only flakes
/// impossible to reproduce.
const DEFAULT_CONCURRENCY: usize = 4;

/// Hard ceiling on how long a single scenario may run before the watchdog gives up on it.
/// Overridable with `CUCUMBER_SCENARIO_TIMEOUT_SECS`.
const DEFAULT_SCENARIO_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// Scenarios currently in flight, keyed by `feature :: scenario`, with the time they started.
fn in_flight() -> &'static Mutex<HashMap<String, Instant>> {
    static IN_FLIGHT: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    IN_FLIGHT.get_or_init(|| Mutex::new(HashMap::new()))
}

fn env_or<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

/// Watchdog for scenarios that hang instead of failing.
///
/// Nothing in cucumber bounds a scenario's runtime, so a `wait_for!` that never fires or a gRPC
/// call without a deadline would previously hang until the 120-minute GitHub job limit killed the
/// runner — producing a log with no scenario name and no failure in it. This names the culprit and
/// exits non-zero instead.
fn spawn_scenario_watchdog(limit: Duration) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_secs(30));
            let stuck: Vec<(String, Duration)> = in_flight()
                .lock()
                .unwrap()
                .iter()
                .filter_map(|(name, started)| {
                    let elapsed = started.elapsed();
                    (elapsed > limit).then(|| (name.clone(), elapsed))
                })
                .collect();
            if stuck.is_empty() {
                continue;
            }
            eprintln!("\n=== SCENARIO WATCHDOG: aborting run ===");
            for (name, elapsed) in &stuck {
                eprintln!(
                    "  stuck for {:.0}s (limit {:.0}s): {name}",
                    elapsed.as_secs_f64(),
                    limit.as_secs_f64()
                );
            }
            eprintln!("See integration_tests/log/ for the per-component logs of the run.");
            std::process::exit(1);
        }
    });
}

fn main() {
    // Set the network env var once at startup — safe because no other threads exist yet.
    // This replaces the unsafe set_var calls that were scattered across spawn functions.
    unsafe {
        std::env::set_var("TARI_NETWORK", "localnet");
    }

    initialize_logging(
        &PathBuf::from("log4rs/cucumber.yml"),
        &PathBuf::from("./"),
        include_str!("../log4rs/cucumber.yml"),
    )
    .expect("logging not configured");
    // Output capture removed - using internal feature that's not stable
    // Tests will output to regular stdout/stderr instead
    spawn_scenario_watchdog(Duration::from_secs(env_or(
        "CUCUMBER_SCENARIO_TIMEOUT_SECS",
        DEFAULT_SCENARIO_TIMEOUT.as_secs(),
    )));

    let runtime = Runtime::new().unwrap();
    runtime.block_on(async {
        let world = TariWorld::cucumber()
        // .repeat_failed() — removed: retrying hides flaky tests instead of surfacing them
        // following config needed to use eprint statements in the tests
        .max_concurrent_scenarios(env_or("CUCUMBER_CONCURRENCY", DEFAULT_CONCURRENCY))
        .after(move |feature, _rule, scenario, ev, maybe_world| {
            Box::pin(async move {
                in_flight()
                    .lock()
                    .unwrap()
                    .remove(&format!("{} :: {}", feature.name, scenario.name));
                match ev {
                    ScenarioFinished::StepFailed(_capture_locations, _location, _error) => {
                        error!(target: LOG_TARGET, "Scenario failed");
                    },
                    ScenarioFinished::StepPassed => {
                        info!(target: LOG_TARGET, "Scenario was successful.");
                    },
                    ScenarioFinished::StepSkipped => {
                        warn!(target: LOG_TARGET, "Some steps were skipped.");
                    },
                    ScenarioFinished::BeforeHookFailed(_info) => {
                        error!(target: LOG_TARGET, "Before hook failed!");
                    },
                }
                if let Some(maybe_world) = maybe_world {
                    maybe_world.after(scenario).await;
                }
            })
        })
        .before(move |feature, _rule, scenario, world| {
            Box::pin(async move {
                in_flight()
                    .lock()
                    .unwrap()
                    .insert(format!("{} :: {}", feature.name, scenario.name), Instant::now());
                println!("{} : {}", scenario.keyword, scenario.name); // This will be printed into the stdout_buffer
                info!(target: LOG_TARGET, "Starting {} {}", scenario.keyword, scenario.name);

                world.before(feature, scenario).await;
            })
        });
        let file = fs::File::create("cucumber-output-junit.xml").unwrap();
        // NOTE: no `.fail_fast()`. With several scenarios in flight, the first failure used to
        // abort the whole run, discarding the in-flight scenarios and truncating the JUnit report
        // — so a CI run told you about exactly one failure and nothing about the health of the
        // rest of the suite, which is the opposite of what de-flaking needs. The run still exits
        // non-zero on any failure.
        world
            .fail_on_skipped()
            .with_writer(
                writer::Summarize::new(writer::Basic::new(
                    io::stdout(),
                    writer::Coloring::Auto,
                    Verbosity::ShowWorldAndDocString,
                ))
                .tee::<TariWorld, _>(writer::JUnit::for_tee(file, 0))
                .normalized(),
            )
            .run_and_exit("tests/features/")
            .await;
    });
}
