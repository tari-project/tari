// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{io, time::Duration};

use crossterm::{
    cursor,
    event::{Event, EventStream, KeyCode, KeyEvent, KeyModifiers},
    terminal,
};
use futures::{FutureExt, StreamExt};
use rustyline::{config::OutputStreamType, error::ReadlineError, CompletionType, Config, EditMode, Editor};
use tari_shutdown::ShutdownSignal;
use tokio::{signal, time};

use crate::{
    commands::{
        cli,
        command::{CommandContext, WatchCommand},
        parser::Parser,
        reader::CommandReader,
    },
    LOG_TARGET,
};

pub struct CliLoop {
    context: CommandContext,
    reader: CommandReader,
    commands: Vec<String>,
    watch_task: Option<WatchCommand>,
    non_interactive: bool,
    first_signal: bool,
    done: bool,
    shutdown_signal: ShutdownSignal,
}

impl CliLoop {
    pub fn new(context: CommandContext, watch_command: Option<String>, non_interactive: bool) -> Self {
        let parser = Parser::new();
        let commands = parser.get_commands();
        let cli_config = Config::builder()
            .history_ignore_space(true)
            .completion_type(CompletionType::List)
            .edit_mode(EditMode::Emacs)
            .output_stream(OutputStreamType::Stdout)
            .auto_add_history(true)
            .build();
        let mut rustyline = Editor::with_config(cli_config);
        rustyline.set_helper(Some(parser));
        // Saves the user from having to type this in again to return to "watch status"
        rustyline.history_mut().add("watch status");
        let reader = CommandReader::new(rustyline);
        let watch_task = {
            if let Some(line) = watch_command {
                WatchCommand::new(line)
            } else if non_interactive {
                WatchCommand::new("status --output log")
            } else {
                WatchCommand::new("status")
            }
        };
        let shutdown_signal = context.shutdown.to_signal();
        Self {
            context,
            reader,
            commands,
            watch_task: Some(watch_task),
            non_interactive,
            first_signal: false,
            done: false,
            shutdown_signal,
        }
    }

    /// Runs the Base Node CLI loop
    /// ## Parameters
    /// `parser` - The parser to process input commands
    /// `shutdown` - The trigger for shutting down
    ///
    /// ## Returns
    /// Doesn't return anything
    pub async fn cli_loop(mut self) {
        cli::print_banner(self.commands.clone(), 3);

        if self.non_interactive {
            self.watch_loop_non_interactive().await;
        } else {
            while !self.done {
                self.watch_loop().await;
                self.execute_command().await;
            }
        }
    }

    fn is_interrupted(&self, event: Option<Result<Event, io::Error>>) -> bool {
        if let Some(Ok(Event::Key(key))) = event {
            match key {
                KeyEvent {
                    code: KeyCode::Char('c'),
                    modifiers: KeyModifiers::CONTROL,
                } => {
                    return true;
                },
                _ => {
                    if self.non_interactive {
                        println!("Press Ctrl-C to interrupt the node.");
                    } else {
                        println!("Press Ctrl-C to enter the interactive shell.");
                    }
                },
            }
        }
        false
    }

    async fn watch_loop(&mut self) {
        if let Some(command) = self.watch_task.take() {
            let mut interrupt = signal::ctrl_c().fuse().boxed();
            let mut software_update_notif = self.context.software_updater.update_notifier().clone();
            let config = self.context.config.clone();
            let line = command.line();
            let interval = command
                .interval
                .map(Duration::from_secs)
                .unwrap_or(config.base_node.status_line_interval);
            if let Err(err) = self.context.handle_command_str(line).await {
                println!("Wrong command to watch `{}`. Failed with: {}", line, err);
            } else {
                let mut events = EventStream::new();
                loop {
                    let interval = time::sleep(interval);
                    tokio::select! {
                        _ = interval => {
                            if let Err(err) = self.context.handle_command_str(line).await {
                                println!("Watched command `{}` failed: {}", line, err);
                            }
                            continue;
                        },
                        _ = &mut interrupt => {
                            break;
                        }
                        event = events.next() => {
                            if self.is_interrupted(event) {
                                break;
                            }
                        }
                        Ok(_) = software_update_notif.changed() => {
                            // Ensure the watch borrow is dropped immediately after use
                            if let Some(ref update) = *software_update_notif.borrow() {
                                println!(
                                    "Version {} of the {} is available: {} (sha: {})",
                                    update.version(),
                                    update.app(),
                                    update.download_url(),
                                    update.to_hash_hex()
                                );
                            }
                        }
                    }
                    crossterm::execute!(io::stdout(), cursor::MoveToNextLine(1)).ok();
                }
                terminal::disable_raw_mode().ok();
            }
        }
    }

    async fn watch_loop_non_interactive(&mut self) {
        if let Some(command) = self.watch_task.take() {
            let mut interrupt = signal::ctrl_c().fuse().boxed();
            let config = &self.context.config;
            let line = command.line();
            let interval = command
                .interval
                .map(Duration::from_secs)
                .unwrap_or(config.base_node.status_line_interval);
            if let Err(err) = self.context.handle_command_str(line).await {
                println!("Wrong command to watch `{}`. Failed with: {}", line, err);
            } else {
                while !self.done {
                    let interval = time::sleep(interval);
                    tokio::select! {
                        _ = interval => {
                            if let Err(err) = self.context.handle_command_str(line).await {
                                println!("Watched command `{}` failed: {}", line, err);
                            }
                            continue;
                        },
                        _ = &mut interrupt => {
                            break;
                        },
                        _ = self.shutdown_signal.wait() => {
                            self.done = true;
                        }
                    }
                }
            }
        }
    }

    async fn handle_line(&mut self, line: String) {
        // Reset the interruption flag if the command entered.
        self.first_signal = false;
        if !line.is_empty() {
            match self.context.handle_command_str(&line).await {
                Err(err) => {
                    println!("Command `{}` failed: {}", line, err);
                },
                Ok(command) => {
                    self.watch_task = command;
                },
            }
        }
    }

    async fn execute_command(&mut self) {
        tokio::select! {
            res = self.reader.next_command() => {
                if let Some(event) = res {
                    match event {
                        Ok(line) => {
                            self.handle_line(line).await;
                        }
                        Err(ReadlineError::Interrupted) => {
                            // If `Ctrl-C` is pressed
                            if self.first_signal {
                                self.done = true;
                            } else {
                                println!("Are you leaving already? Press Ctrl-C again (or Ctrl-D) to terminate the node.");
                                self.first_signal = true;
                            }
                        }
                        Err(ReadlineError::Eof) => {
                            // If `Ctrl-D` is pressed
                            self.done = true;
                        }
                        Err(err) => {
                            log::debug!(target:  LOG_TARGET, "Could not read line from rustyline:{}", err);
                            self.done = true;
                        }
                    }
                } else {
                    self.done = true;
                }
            },
            _ = self.shutdown_signal.wait() => {
                self.done = true;
            }
        }
    }
}
