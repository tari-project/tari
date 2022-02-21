use rustyline::{error::ReadlineError, Editor};
use tokio::{
    sync::mpsc,
    task::{self, JoinHandle},
};

use super::parser::Parser;
use crate::LOG_TARGET;

// TODO: Remove it and use the result from the `rustyline` directly
pub enum CommandEvent {
    Command(String),
    Interrupt,
    Error(String),
}

pub struct CommandReader {
    #[allow(dead_code)]
    task: JoinHandle<()>,
    sender: mpsc::Sender<()>,
    receiver: mpsc::Receiver<CommandEvent>,
}

impl CommandReader {
    pub fn new(mut rustyline: Editor<Parser>) -> Self {
        let (tx_next, mut rx_next) = mpsc::channel(1);
        let (tx_event, rx_event) = mpsc::channel(1);
        let task = task::spawn_blocking(move || {
            loop {
                if rx_next.blocking_recv().is_none() {
                    break;
                }
                let readline = rustyline.readline(">> ");
                let event;
                match readline {
                    Ok(line) => {
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
                if tx_event.blocking_send(event).is_err() {
                    break;
                }
            }
        });
        Self {
            task,
            sender: tx_next,
            receiver: rx_event,
        }
    }

    pub async fn next_command(&mut self) -> Option<CommandEvent> {
        self.sender.send(()).await.ok()?;
        self.receiver.recv().await
    }
}
