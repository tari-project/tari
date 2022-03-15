use rustyline::{error::ReadlineError, Editor};
use tokio::{
    sync::mpsc,
    task::{self, JoinHandle},
};

use super::parser::Parser;

pub struct CommandReader {
    #[allow(dead_code)]
    task: JoinHandle<()>,
    sender: mpsc::Sender<()>,
    receiver: mpsc::Receiver<Result<String, ReadlineError>>,
}

impl CommandReader {
    pub fn new(mut rustyline: Editor<Parser>) -> Self {
        let (tx_next, mut rx_next) = mpsc::channel(1);
        let (tx_event, rx_event) = mpsc::channel(1);
        let task = task::spawn_blocking(move || loop {
            if rx_next.blocking_recv().is_none() {
                break;
            }
            let event = rustyline.readline(">> ");
            if tx_event.blocking_send(event).is_err() {
                break;
            }
        });
        Self {
            task,
            sender: tx_next,
            receiver: rx_event,
        }
    }

    pub async fn next_command(&mut self) -> Option<Result<String, ReadlineError>> {
        self.sender.send(()).await.ok()?;
        self.receiver.recv().await
    }
}
