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
use minotari_app_utilities::consts;
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
    event::{KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use log::*;
use tokio::runtime::Handle;
use tui::{backend::CrosstermBackend, Terminal};
use ui_error::UiError;

use crate::utils::events::{Event, EventStream};

pub const MAX_WIDTH: u16 = 133;

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
    let terminal_title = format!("Minotari Console Wallet - Version {}", consts::APP_VERSION);
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

    loop {
        terminal.draw(|f| app.draw(f)).map_err(|e| {
            error!(target: LOG_TARGET, "Error drawing interface. {}", e);
            ExitCode::InterfaceError
        })?;
        #[allow(clippy::blocks_in_conditions)]
        match events.next().map_err(|e| {
            error!(target: LOG_TARGET, "Error reading input event: {}", e);
            ExitCode::InterfaceError
        })? {
            Event::Input(event) => match (event.code, event.modifiers) {
                (KeyCode::Char(c), KeyModifiers::CONTROL) => app.on_control_key(c),
                (KeyCode::Char(c), _) => app.on_key(c),
                (KeyCode::Left, _) => app.on_left(),
                (KeyCode::Up, _) => app.on_up(),
                (KeyCode::Right, _) => app.on_right(),
                (KeyCode::Down, _) => app.on_down(),
                (KeyCode::Esc, _) => app.on_esc(),
                (KeyCode::Backspace, _) => app.on_backspace(),
                (KeyCode::Enter, _) => app.on_key('\n'),
                (KeyCode::Tab, _) => app.on_key('\t'),
                (KeyCode::BackTab, _) => app.on_backtab(),
                (KeyCode::F(10), _) => app.on_f10(),
                _ => {},
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
