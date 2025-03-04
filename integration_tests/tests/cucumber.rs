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
#![feature(internal_output_capture)]

use std::{
    fs,
    io,
    path::PathBuf,
    str::{self},
    sync::{Arc, Mutex},
};

use cucumber::{event::ScenarioFinished, writer, writer::Verbosity, World as _};
use log::*;
use tari_common::initialize_logging;
use tari_integration_tests::TariWorld;
use tokio::runtime::Runtime;

pub mod steps;

pub const LOG_TARGET: &str = "cucumber";
pub const LOG_TARGET_STDOUT: &str = "stdout";

fn flush_stdout(buffer: &Arc<Mutex<Vec<u8>>>) {
    // After each test we flush the stdout to the logs.
    info!(
        target: LOG_TARGET_STDOUT,
        "{}",
        str::from_utf8(&buffer.lock().unwrap()).unwrap()
    );
    buffer.lock().unwrap().clear();
}

fn main() {
    initialize_logging(
        &PathBuf::from("log4rs/cucumber.yml"),
        &PathBuf::from("./"),
        include_str!("../log4rs/cucumber.yml"),
    )
    .expect("logging not configured");
    let stdout_buffer = Arc::new(Mutex::new(Vec::<u8>::new()));
    #[cfg(test)]
    std::io::set_output_capture(Some(stdout_buffer.clone()));
    // Never move this line below the runtime creation!!! It will cause that any new thread created via task::spawn will
    // not be affected by the output capture.
    let stdout_buffer_clone = stdout_buffer.clone();
    let runtime = Runtime::new().unwrap();
    runtime.block_on(async {
        let world = TariWorld::cucumber()
        .repeat_failed()
        // following config needed to use eprint statements in the tests
        .max_concurrent_scenarios(5)
        .after(move |_feature, _rule, scenario, ev, maybe_world| {
            let stdout_buffer = stdout_buffer_clone.clone();
            Box::pin(async move {
                flush_stdout(&stdout_buffer);
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
                println!("{} : {}", scenario.keyword, scenario.name); // This will be printed into the stdout_buffer
                info!(target: LOG_TARGET, "Starting {} {}", scenario.keyword, scenario.name);

                world.before(feature, scenario).await;
            })
        });
        let file = fs::File::create("cucumber-output-junit.xml").unwrap();
        world
            // .fail_on_skipped()
            // .fail_fast() - Not yet supported in 0.18
            .with_writer(writer::Tee::new(writer::JUnit::new(file, Verbosity::ShowWorldAndDocString),
                                          writer::Summarize::new(writer::Basic::new(io::stdout(), writer::Coloring::Auto, Verbosity::ShowWorldAndDocString))))
            .run("tests/features/")
            .await;
    });

    // If by any chance we have anything in the stdout buffer just log it.
    flush_stdout(&stdout_buffer);
}
