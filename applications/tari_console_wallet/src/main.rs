use crate::{
    app::App,
    utils::{
        crossterm_events::CrosstermEvents,
        events::{Event, EventStream},
    },
};
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use log::*;
use std::io::{stdout, Write};
use tui::{backend::CrosstermBackend, Terminal};

#[macro_use]
extern crate lazy_static;

mod app;
mod dummy_data;
mod ui;
mod utils;

pub const LOG_TARGET: &str = "console_wallet::app";

/// Enum to show failure information
enum ExitCodes {
    ConfigError = 101,
    UnknownError = 102,
    InterfaceError = 103,
}

impl From<tari_common::ConfigError> for ExitCodes {
    fn from(err: tari_common::ConfigError) -> Self {
        error!(target: LOG_TARGET, "{}", err);
        Self::ConfigError
    }
}

/// Application entry point
fn main() {
    match main_inner_crossterm() {
        Ok(_) => std::process::exit(0),
        Err(exit_code) => std::process::exit(exit_code as i32),
    }
}

fn main_inner_crossterm() -> Result<(), ExitCodes> {
    let events = CrosstermEvents::new();
    enable_raw_mode().map_err(|e| {
        error!(target: LOG_TARGET, "Error enabling Raw Mode {}", e);
        ExitCodes::InterfaceError
    })?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture).map_err(|e| {
        error!(target: LOG_TARGET, "Error creating stdout context. {}", e);
        ExitCodes::InterfaceError
    })?;

    let backend = CrosstermBackend::new(stdout);

    let mut terminal = Terminal::new(backend).map_err(|e| {
        error!(target: LOG_TARGET, "Error creating Terminal context. {}", e);
        ExitCodes::InterfaceError
    })?;

    let mut app = App::new("Tari Console Wallet");
    loop {
        terminal.draw(|f| ui::draw(f, &mut app)).map_err(|e| {
            error!(target: LOG_TARGET, "Error drawing interface. {}", e);
            ExitCodes::InterfaceError
        })?;

        match events.next().map_err(|e| {
            error!(target: LOG_TARGET, "Error reading input event: {}", e);
            ExitCodes::InterfaceError
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
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture).map_err(|e| {
        error!(target: LOG_TARGET, "Error releasing stdout {}", e);
        ExitCodes::InterfaceError
    })?;
    terminal.show_cursor().map_err(|e| {
        error!(target: LOG_TARGET, "Error showing cursor: {}", e);
        ExitCodes::InterfaceError
    })?;

    Ok(())
}
