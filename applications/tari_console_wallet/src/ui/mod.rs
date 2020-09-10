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

use crate::utils::crossterm_events::CrosstermEvents;
use log::error;
use tari_app_utilities::utilities::ExitCodes;

mod app;
mod components;
pub mod multi_column_list;
mod selected_transaction_list;
mod send_input_mode;
pub mod state;
mod stateful_list;
mod ui_contact;
mod ui_error;

pub use app::*;
pub use selected_transaction_list::*;
pub use send_input_mode::*;
pub use stateful_list::*;
pub use ui_contact::*;
pub use ui_error::*;

use crate::utils::events::{Event, EventStream};
use crossterm::{
    event::{KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io::{stdout, Stdout, Write};
use tokio::runtime::Handle;
use tui::{backend::CrosstermBackend, Terminal};

const MAX_WIDTH: u16 = 133;

pub fn run(app: App<CrosstermBackend<Stdout>>) -> Result<(), ExitCodes> {
    let mut app = app;
    Handle::current()
        .block_on(app.refresh_state())
        .map_err(|e| ExitCodes::WalletError(e.to_string()))?;
    crossterm_loop(app)
}
/// This is the main loop of the application UI using Crossterm based events
fn crossterm_loop(app: App<CrosstermBackend<Stdout>>) -> Result<(), ExitCodes> {
    let mut app = app;
    let events = CrosstermEvents::new();
    enable_raw_mode().map_err(|e| {
        error!(target: LOG_TARGET, "Error enabling Raw Mode {}", e);
        ExitCodes::InterfaceError
    })?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen).map_err(|e| {
        error!(target: LOG_TARGET, "Error creating stdout context. {}", e);
        ExitCodes::InterfaceError
    })?;

    let backend = CrosstermBackend::new(stdout);

    let mut terminal = Terminal::new(backend).map_err(|e| {
        error!(target: LOG_TARGET, "Error creating Terminal context. {}", e);
        ExitCodes::InterfaceError
    })?;

    loop {
        terminal
            .draw(|f| Handle::current().block_on(app.draw(f)))
            .map_err(|e| {
                error!(target: LOG_TARGET, "Error drawing interface. {}", e);
                ExitCodes::InterfaceError
            })?;

        match events.next().map_err(|e| {
            error!(target: LOG_TARGET, "Error reading input event: {}", e);
            ExitCodes::InterfaceError
        })? {
            Event::Input(event) => match (event.code, event.modifiers) {
                (KeyCode::Char(c), KeyModifiers::CONTROL) => Handle::current().block_on(app.on_control_key(c)),
                (KeyCode::Char(c), _) => Handle::current().block_on(app.on_key(c)),
                (KeyCode::Left, _) => Handle::current().block_on(app.on_left()),
                (KeyCode::Up, _) => Handle::current().block_on(app.on_up()),
                (KeyCode::Right, _) => Handle::current().block_on(app.on_right()),
                (KeyCode::Down, _) => Handle::current().block_on(app.on_down()),
                (KeyCode::Esc, _) => Handle::current().block_on(app.on_esc()),
                (KeyCode::Backspace, _) => Handle::current().block_on(app.on_backspace()),
                (KeyCode::Enter, _) => Handle::current().block_on(app.on_key('\n')),
                (KeyCode::Tab, _) => Handle::current().block_on(app.on_key('\t')),
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

    disable_raw_mode().map_err(|e| {
        error!(target: LOG_TARGET, "Error disabling Raw Mode {}", e);
        ExitCodes::InterfaceError
    })?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen).map_err(|e| {
        error!(target: LOG_TARGET, "Error releasing stdout {}", e);
        ExitCodes::InterfaceError
    })?;
    terminal.show_cursor().map_err(|e| {
        error!(target: LOG_TARGET, "Error showing cursor: {}", e);
        ExitCodes::InterfaceError
    })?;

    Ok(())
}
