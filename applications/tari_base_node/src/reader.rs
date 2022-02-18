use derive_more::{Deref, DerefMut};
use rustyline::{error::ReadlineError, Editor};
use tokio::{
    sync::mpsc,
    task::{self, JoinHandle},
};

use super::LOG_TARGET;
use crate::parser::Parser;

pub enum CommandEvent {
    Command(String),
    Interrupt,
    Error(String),
}

#[derive(Deref, DerefMut)]
pub struct CommandReader {
    #[allow(dead_code)]
    task: JoinHandle<()>,
    #[deref]
    #[deref_mut]
    receiver: mpsc::UnboundedReceiver<CommandEvent>,
}

impl CommandReader {
    pub fn new(mut rustyline: Editor<Parser>) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let task = task::spawn_blocking(move || {
            loop {
                let readline = rustyline.readline(">> ");

                let event;
                match readline {
                    Ok(line) => {
                        rustyline.add_history_entry(line.as_str());
                        event = CommandEvent::Command(line);
                    },
                    Err(ReadlineError::Interrupted) => {
                        // shutdown section. Will shutdown all interfaces when ctrl-c was pressed
                        log::info!(target: LOG_TARGET, "Interruption signal received from user.");
                        event = CommandEvent::Interrupt;
                    },
                    Err(err) => {
                        println!("Error: {:?}", err);
                        event = CommandEvent::Error(err.to_string());
                    },
                }
                if tx.send(event).is_err() {
                    break;
                }
            }
        });
        Self { task, receiver: rx }
    }
}
