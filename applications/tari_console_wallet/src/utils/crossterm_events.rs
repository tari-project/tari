// Copyright 2020. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use crate::utils::events::{Event, EventStream};
use crossterm::event::{self, Event as CEvent, KeyEvent};
use log::*;
use std::{
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

pub const LOG_TARGET: &str = "wallet::app::crossterm_events";

/// A small event handler that wrap Crossterm input and tick events. Each event
/// type is handled in its own thread and returned to a common `Receiver`
pub struct CrosstermEvents {
    rx: mpsc::Receiver<Event<KeyEvent>>,
    _input_handle: thread::JoinHandle<()>,
}

#[derive(Debug, Clone, Copy)]
pub struct Config {
    pub tick_rate: Duration,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            tick_rate: Duration::from_millis(250),
        }
    }
}

impl CrosstermEvents {
    pub fn new() -> CrosstermEvents {
        CrosstermEvents::with_config(Config::default())
    }

    pub fn with_config(config: Config) -> CrosstermEvents {
        let (tx, rx) = mpsc::channel();
        let input_handle = thread::spawn(move || {
            let mut last_tick = Instant::now();
            loop {
                // poll for tick rate duration, if no events, sent tick event.
                match event::poll(
                    config
                        .tick_rate
                        .checked_sub(last_tick.elapsed())
                        .unwrap_or_else(|| Duration::from_millis(1)),
                ) {
                    Ok(true) => {
                        if let Ok(CEvent::Key(key)) = event::read() {
                            tx.send(Event::Input(key)).unwrap();
                        }
                    },
                    Ok(false) => {},
                    Err(e) => {
                        error!(target: LOG_TARGET, "Internal error in crossterm events: {}", e);
                    },
                }
                if last_tick.elapsed() >= config.tick_rate {
                    if let Err(e) = tx.send(Event::Tick) {
                        warn!(target: LOG_TARGET, "Error sending Tick event on MPSC channel: {}", e);
                    }
                    last_tick = Instant::now();
                }
            }
        });

        CrosstermEvents {
            rx,
            _input_handle: input_handle,
        }
    }
}

impl EventStream<KeyEvent> for CrosstermEvents {
    fn next(&self) -> Result<Event<KeyEvent>, mpsc::RecvError> {
        self.rx.recv()
    }
}
