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

use crossterm::terminal::SetTitle;
use log::error;
use tari_common::exit_codes::{ExitCode, ExitError};

use crate::utils::crossterm_events::CrosstermEvents;
mod app;
mod components;
pub mod state;
mod ui_burnt_proof;
mod ui_contact;
mod ui_error;
mod widgets;

use std::io::{stdout, Stdout};

pub use app::*;
use crossterm::{
    event::{KeyCode, KeyEventState, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use log::*;
use tokio::runtime::Handle;
use tui::{backend::CrosstermBackend, Terminal};
use ui_error::UiError;

use crate::utils::events::{Event, EventStream};

pub const MAX_WIDTH: u16 = 167;

pub fn run(app: App<CrosstermBackend<Stdout>>) -> Result<(), ExitError> {
    let mut app = app;
    Handle::current()
        .block_on(async {
            trace!(target: LOG_TARGET, "Refreshing transaction state");
            app.app_state.refresh_transaction_state().await?;
            trace!(target: LOG_TARGET, "Refreshing contacts state");
            app.app_state.refresh_contacts_state().await?;
            trace!(target: LOG_TARGET, "Refreshing burnt proofs state");
            app.app_state.refresh_burnt_proofs_state().await?;
            trace!(target: LOG_TARGET, "Refreshing connected peers state");
            app.app_state.refresh_connected_peers_state().await?;
            trace!(target: LOG_TARGET, "Checking connectivity");
            app.app_state.check_connectivity().await;
            trace!(target: LOG_TARGET, "Starting balance enquiry debouncer");
            app.app_state.start_balance_enquiry_debouncer().await?;
            trace!(target: LOG_TARGET, "Starting app state event monitor");
            app.app_state.start_event_monitor(app.notifier.clone()).await;
            Result::<_, UiError>::Ok(())
        })
        .map_err(|e| ExitError::new(ExitCode::WalletError, e))?;
    crossterm_loop(app)
}

/// This is the main loop of the application UI using Crossterm based events
#[allow(clippy::too_many_lines)]
fn crossterm_loop(mut app: App<CrosstermBackend<Stdout>>) -> Result<(), ExitError> {
    let events = CrosstermEvents::new();
    enable_raw_mode().map_err(|e| {
        error!(target: LOG_TARGET, "Error enabling Raw Mode {}", e);
        ExitCode::InterfaceError
    })?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen).map_err(|e| {
        error!(target: LOG_TARGET, "Error creating stdout context. {}", e);
        ExitCode::InterfaceError
    })?;
    let terminal_title = format!("Minotari Console Wallet - Version {}", env!("CARGO_PKG_VERSION"));
    if let Err(e) = execute!(stdout, SetTitle(terminal_title.as_str())) {
        println!("Error setting terminal title. {}", e)
    }

    let backend = CrosstermBackend::new(stdout);

    let mut terminal = Terminal::new(backend).map_err(|e| {
        error!(target: LOG_TARGET, "Error creating Terminal context. {}", e);
        ExitCode::InterfaceError
    })?;

    terminal.clear().map_err(|e| {
        error!(target: LOG_TARGET, "Error clearing interface. {}", e);
        ExitCode::InterfaceError
    })?;

    #[cfg(target_os = "windows")]
    let (mut key_press, mut previous_code, mut previous_kind) = (None, None, None);
    loop {
        terminal.draw(|f| app.draw(f)).map_err(|e| {
            error!(target: LOG_TARGET, "Error drawing interface. {}", e);
            ExitCode::InterfaceError
        })?;
        let event = events.next();
        #[allow(clippy::blocks_in_conditions)]
        match event.map_err(|e| {
            error!(target: LOG_TARGET, "Error reading input event: {}", e);
            ExitCode::InterfaceError
        })? {
            Event::Input(event) => {
                trace!(target: LOG_TARGET, "event: '{:?}' '{}' '{:?}' '{}'",
                    event.code,
                    event.modifiers,
                    event.kind,
                    match event.state {
                        KeyEventState::KEYPAD => "KEYPAD",
                        KeyEventState::CAPS_LOCK => "CAPS_LOCK",
                        KeyEventState::NUM_LOCK => "NUM_LOCK",
                        _ => "NONE",
                    }
                );
                #[cfg(target_os = "windows")]
                let (action_now, change_case) = {
                    use crossterm::event::KeyEventKind;
                    let mut change_case = false;
                    if let KeyEventKind::Press = event.kind {
                        key_press = Some(event.code);
                    }
                    let action_now = match (event.kind, event.modifiers, event.code) {
                        (KeyEventKind::Press, KeyModifiers::CONTROL, KeyCode::Char(c)) => c == 'q',
                        (KeyEventKind::Press, _, KeyCode::F(c)) => c == 10,
                        (KeyEventKind::Press, _, _) => {
                            previous_kind == Some(KeyEventKind::Press) && previous_code == Some(event.code)
                        },
                        (KeyEventKind::Release, _, _) => match event.code {
                            // Typing with Caps lock on results in Press and Release keycodes having different
                            // cases
                            KeyCode::Char(cr) => {
                                if let Some(KeyCode::Char(cp)) = key_press {
                                    if String::from(cp).to_lowercase() == String::from(cr).to_lowercase() && cp != cr {
                                        change_case = true;
                                    }
                                    String::from(cp).to_lowercase() == String::from(cr).to_lowercase()
                                } else {
                                    false
                                }
                            },
                            _ => key_press == Some(event.code),
                        },
                        (..) => false,
                    };
                    previous_kind = Some(event.kind);
                    previous_code = Some(event.code);
                    (action_now, change_case)
                };
                #[cfg(not(target_os = "windows"))]
                let (action_now, change_case) = (true, false);
                match (event.code, event.modifiers, action_now) {
                    (_, _, false) => {},
                    (KeyCode::Char(c), KeyModifiers::CONTROL, _) => app.on_control_key(c),
                    (KeyCode::Char(c), _, _) => {
                        let mut c_new = c;
                        if change_case {
                            if c_new.is_uppercase() {
                                c_new = c_new.to_lowercase().next().unwrap_or(c_new);
                            } else {
                                c_new = c_new.to_uppercase().next().unwrap_or(c_new);
                            }
                            trace!(target: LOG_TARGET, "Inconsistent case detected; '{}' changed to '{}'", c, c_new);
                        }
                        app.on_key(c_new)
                    },
                    (KeyCode::Left, _, _) => app.on_left(),
                    (KeyCode::Up, _, _) => app.on_up(),
                    (KeyCode::Right, _, _) => app.on_right(),
                    (KeyCode::Down, _, _) => app.on_down(),
                    (KeyCode::Esc, _, _) => app.on_esc(),
                    (KeyCode::Backspace, _, _) => app.on_backspace(),
                    (KeyCode::Enter, _, _) => app.on_key('\n'),
                    (KeyCode::Tab, _, _) => app.on_key('\t'),
                    (KeyCode::BackTab, _, _) => app.on_backtab(),
                    (KeyCode::F(10), _, _) => app.on_f10(),
                    _ => {},
                }
            },
            Event::Tick => {
                app.on_tick();
            },
        }
        if app.should_quit {
            break;
        }
    }

    terminal.clear().map_err(|e| {
        error!(target: LOG_TARGET, "Error clearing interface. {}", e);
        ExitCode::InterfaceError
    })?;

    disable_raw_mode().map_err(|e| {
        error!(target: LOG_TARGET, "Error disabling Raw Mode {}", e);
        ExitCode::InterfaceError
    })?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen).map_err(|e| {
        error!(target: LOG_TARGET, "Error releasing stdout {}", e);
        ExitCode::InterfaceError
    })?;
    terminal.show_cursor().map_err(|e| {
        error!(target: LOG_TARGET, "Error showing cursor: {}", e);
        ExitCode::InterfaceError
    })?;

    Ok(())
}
