// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{path::Path, str::FromStr};

use log::*;
use tari_core::transactions::{tari_amount::MicroTari, transaction_components::TemplateType};
use tari_wallet::output_manager_service::UtxoSelectionCriteria;
use tokio::{runtime::Handle, sync::watch};
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};
use unicode_width::UnicodeWidthStr;
use url::Url;

use crate::ui::{
    components::{balance::Balance, Component, KeyHandled},
    state::{AppState, UiTransactionSendStatus},
    widgets::draw_dialog,
};

const LOG_TARGET: &str = "wallet::console_wallet::register_template_tab ";

fn maybe_extract_git_repo(git_url: &str) -> Option<String> {
    let url = match Url::parse(git_url) {
        Ok(git_url) => git_url,
        Err(_) => return None,
    };

    match url.domain() {
        Some("github.com") | Some("bitbucket.org") | Some("gitlab.com") => url
            .path_segments()
            .map(|x| x.collect::<Vec<&str>>())
            .map(|segments| match segments.as_slice() {
                &[owner, repo, ..] if url.domain().is_some() => Some(format!(
                    "{}://{}/{}/{}",
                    url.scheme(),
                    url.domain().unwrap(),
                    owner,
                    repo
                )),
                _ => None,
            })
            .unwrap_or_default(),

        _ => None,
    }
}

fn maybe_extract_template_type(url: &str) -> Option<(String, TemplateType)> {
    let url = match Url::parse(url) {
        Ok(url) => url,
        Err(_) => return None,
    };

    if let Some(ext) = Path::new(url.path()).extension() {
        match ext.to_ascii_uppercase().to_str()? {
            "WASM" => Some(("WASM:1".to_string(), TemplateType::Wasm { abi_version: 1 })),
            _ => None,
        }
    } else {
        None
    }
}

pub struct RegisterTemplateTab {
    balance: Balance,
    input_mode: InputMode,
    binary_url: String,
    repository_url: String,
    repository_commit_hash: String,
    binary_checksum: String,
    template_name: String,
    template_version: String,
    fee_per_gram: String,
    template_type: String,
    error_message: Option<String>,
    success_message: Option<String>,
    offline_message: Option<String>,
    result_watch: Option<watch::Receiver<UiTransactionSendStatus>>,
    confirmation_dialog: Option<ConfirmationDialogType>,
}

impl RegisterTemplateTab {
    pub fn new(app_state: &AppState) -> Self {
        Self {
            balance: Balance::new(),
            input_mode: InputMode::None,
            binary_url: String::new(),
            repository_url: String::new(),
            repository_commit_hash: String::new(),
            binary_checksum: String::new(),
            template_version: String::new(),
            template_name: String::new(),
            error_message: None,
            success_message: None,
            offline_message: None,
            result_watch: None,
            confirmation_dialog: None,
            template_type: String::new(),
            fee_per_gram: app_state.get_default_fee_per_gram().as_u64().to_string(),
        }
    }

    #[allow(clippy::too_many_lines)]
    fn draw_form<B>(&self, f: &mut Frame<B>, area: Rect, _app_state: &AppState)
    where B: Backend {
        let block = Block::default().borders(Borders::ALL).title(Span::styled(
            "Register Code Template",
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ));
        f.render_widget(block, area);

        let form_layout = Layout::default()
            .constraints(
                [
                    Constraint::Length(4),
                    Constraint::Length(3),
                    Constraint::Length(3),
                    Constraint::Length(3),
                    Constraint::Length(3),
                ]
                .as_ref(),
            )
            .margin(1)
            .split(area);

        let instructions = Paragraph::new(vec![
            Spans::from(vec![
                Span::raw("Press "),
                Span::styled("B", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to edit "),
                Span::styled("Binary URL", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" field, "),
                Span::styled("C", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to edit "),
                Span::styled("Binary Checksum", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" field, "),
                Span::styled("U", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to edit "),
                Span::styled("Git Repository URL", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" field, "),
                Span::styled("H", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to edit "),
                Span::styled(
                    "Git Repository Commit Hash",
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(" field, "),
                Span::styled("N", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to edit "),
                Span::styled("Template Name", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" and "),
                Span::styled("V", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to edit "),
                Span::styled("Template Version", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" field, "),
                Span::styled("T", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to edit "),
                Span::styled("Template Type", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" field, "),
                Span::styled("F", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to edit "),
                Span::styled("Fee-per-gram", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" field."),
            ]),
            Spans::from(vec![
                Span::raw("Press "),
                Span::styled("S", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to send a template registration transaction."),
            ]),
        ])
        .wrap(Wrap { trim: false })
        .block(Block::default());
        f.render_widget(instructions, form_layout[0]);

        // ----------------------------------------------------------------------------
        // layouts
        // ----------------------------------------------------------------------------

        let first_row_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(
                [
                    Constraint::Percentage(50),
                    Constraint::Percentage(25),
                    Constraint::Percentage(25),
                ]
                .as_ref(),
            )
            .split(form_layout[1]);

        let second_row_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(100)].as_ref())
            .split(form_layout[2]);

        let third_row_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(100)].as_ref())
            .split(form_layout[3]);

        let fourth_row_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(
                [
                    Constraint::Percentage(40),
                    Constraint::Percentage(40),
                    Constraint::Percentage(20),
                ]
                .as_ref(),
            )
            .split(form_layout[4]);

        // ----------------------------------------------------------------------------
        // First row - Template Name, Template Version, Template Type
        // ----------------------------------------------------------------------------

        let template_name = Paragraph::new(self.template_name.as_ref())
            .style(match self.input_mode {
                InputMode::TemplateName => Style::default().fg(Color::Magenta),
                _ => Style::default(),
            })
            .block(Block::default().borders(Borders::ALL).title("Template (N)ame:"));
        f.render_widget(template_name, first_row_layout[0]);

        let template_version = Paragraph::new(self.template_version.to_string())
            .style(match self.input_mode {
                InputMode::TemplateVersion => Style::default().fg(Color::Magenta),
                _ => Style::default(),
            })
            .block(Block::default().borders(Borders::ALL).title("Template (V)ersion:"));
        f.render_widget(template_version, first_row_layout[1]);

        let template_type = Paragraph::new(self.template_type.as_ref())
            .style(match self.input_mode {
                InputMode::TemplateType => Style::default().fg(Color::Magenta),
                _ => Style::default(),
            })
            .block(Block::default().borders(Borders::ALL).title("Template (T)ype:"));
        f.render_widget(template_type, first_row_layout[2]);

        // ----------------------------------------------------------------------------
        // Second row - Binary URL
        // ----------------------------------------------------------------------------

        let binary_url = Paragraph::new(self.binary_url.as_ref())
            .style(match self.input_mode {
                InputMode::BinaryUrl => Style::default().fg(Color::Magenta),
                _ => Style::default(),
            })
            .block(Block::default().borders(Borders::ALL).title("(B)inary URL:"));
        f.render_widget(binary_url, second_row_layout[0]);

        // ----------------------------------------------------------------------------
        // Third row - Repository URL
        // ----------------------------------------------------------------------------

        let repository_url = Paragraph::new(self.repository_url.as_ref())
            .style(match self.input_mode {
                InputMode::RepositoryUrl => Style::default().fg(Color::Magenta),
                _ => Style::default(),
            })
            .block(Block::default().borders(Borders::ALL).title("Repository (U)RL:"));
        f.render_widget(repository_url, third_row_layout[0]);

        // ----------------------------------------------------------------------------
        // Fourth row - Binary checksum, Repository Commit Hash, Fee per gram
        // ----------------------------------------------------------------------------

        let binary_checksum = Paragraph::new(self.binary_checksum.as_ref())
            .style(match self.input_mode {
                InputMode::BinaryChecksum => Style::default().fg(Color::Magenta),
                _ => Style::default(),
            })
            .block(Block::default().borders(Borders::ALL).title("Binary (C)hecksum:"));
        f.render_widget(binary_checksum, fourth_row_layout[0]);

        let repository_commit_hash = Paragraph::new(self.repository_commit_hash.as_ref())
            .style(match self.input_mode {
                InputMode::RepositoryCommitHash => Style::default().fg(Color::Magenta),
                _ => Style::default(),
            })
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Repository Commit (H)ash:"),
            );
        f.render_widget(repository_commit_hash, fourth_row_layout[1]);

        let fee_per_gram = Paragraph::new(self.fee_per_gram.as_ref())
            .style(match self.input_mode {
                InputMode::FeePerGram => Style::default().fg(Color::Magenta),
                _ => Style::default(),
            })
            .block(Block::default().borders(Borders::ALL).title("(F)ee-per-gram:"));
        f.render_widget(fee_per_gram, fourth_row_layout[2]);

        // ----------------------------------------------------------------------------
        // field cursor placement
        // ----------------------------------------------------------------------------

        match self.input_mode {
            InputMode::None => (),
            InputMode::FeePerGram => f.set_cursor(
                fourth_row_layout[2].x + self.fee_per_gram.width() as u16 + 1,
                fourth_row_layout[2].y + 1,
            ),
            InputMode::TemplateName => f.set_cursor(
                first_row_layout[0].x + self.template_name.width() as u16 + 1,
                first_row_layout[0].y + 1,
            ),
            InputMode::TemplateVersion => f.set_cursor(
                first_row_layout[1].x + self.template_version.width() as u16 + 1,
                first_row_layout[1].y + 1,
            ),
            InputMode::TemplateType => f.set_cursor(
                first_row_layout[2].x + self.template_type.width() as u16 + 1,
                first_row_layout[2].y + 1,
            ),
            InputMode::BinaryUrl => f.set_cursor(
                second_row_layout[0].x + self.binary_url.width() as u16 + 1,
                second_row_layout[0].y + 1,
            ),
            InputMode::BinaryChecksum => f.set_cursor(
                fourth_row_layout[0].x + self.binary_checksum.width() as u16 + 1,
                fourth_row_layout[0].y + 1,
            ),
            InputMode::RepositoryUrl => f.set_cursor(
                third_row_layout[0].x + self.repository_url.width() as u16 + 1,
                third_row_layout[0].y + 1,
            ),
            InputMode::RepositoryCommitHash => f.set_cursor(
                fourth_row_layout[1].x + self.repository_commit_hash.width() as u16 + 1,
                fourth_row_layout[1].y + 1,
            ),
        }
    }

    #[allow(clippy::too_many_lines)]
    fn on_key_confirmation_dialog(&mut self, c: char, app_state: &mut AppState) -> KeyHandled {
        if self.confirmation_dialog.is_some() {
            if 'n' == c {
                self.confirmation_dialog = None;
                return KeyHandled::Handled;
            } else if 'y' == c {
                let template_version = if let Ok(version) = self.template_version.parse::<u16>() {
                    version
                } else {
                    self.confirmation_dialog = None;
                    self.error_message =
                        Some("Template version should be an integer\nPress Enter to continue.".to_string());
                    return KeyHandled::Handled;
                };

                let template_type = match self.template_type.to_lowercase().as_str() {
                    "flow" => TemplateType::Flow,
                    "manifest" => TemplateType::Manifest,
                    s => match s.split(':').collect::<Vec<_>>().as_slice() {
                        &[typ, abi_version] if &typ.to_lowercase() == "wasm" => {
                            let abi_version = match abi_version.parse::<u16>() {
                                Ok(abi_version) => abi_version,
                                Err(_) => {
                                    self.confirmation_dialog = None;
                                    self.error_message = Some(format!(
                                        "Invalid `abi_version` for the `wasm` template type\n{}\nPress Enter to \
                                         continue.",
                                        self.template_type
                                    ));
                                    return KeyHandled::Handled;
                                },
                            };

                            TemplateType::Wasm { abi_version }
                        },

                        _ => {
                            self.confirmation_dialog = None;
                            self.error_message = Some(format!(
                                "Unrecognized template type\n{}\nPress Enter to continue.",
                                self.template_type
                            ));
                            return KeyHandled::Handled;
                        },
                    },
                };

                let fee_per_gram = if let Ok(fee_per_gram) = MicroTari::from_str(self.fee_per_gram.as_str()) {
                    fee_per_gram
                } else {
                    self.confirmation_dialog = None;
                    self.error_message =
                        Some("Fee-per-gram should be an integer\nPress Enter to continue.".to_string());
                    return KeyHandled::Handled;
                };

                let (tx, rx) = watch::channel(UiTransactionSendStatus::Initiated);

                let mut reset_fields = false;

                if Some(ConfirmationDialogType::Normal) == self.confirmation_dialog {
                    match Handle::current().block_on(app_state.register_code_template(
                        self.template_name.clone(),
                        template_version,
                        template_type,
                        self.binary_url.clone(),
                        self.binary_checksum.clone(),
                        self.repository_url.clone(),
                        self.repository_commit_hash.clone(),
                        fee_per_gram,
                        UtxoSelectionCriteria::default(),
                        tx,
                    )) {
                        Err(e) => {
                            self.confirmation_dialog = None;
                            self.error_message = Some(format!(
                                "Failed to register code template:\n{:?}\nPress Enter to continue.",
                                e
                            ))
                        },
                        Ok(_) => {
                            Handle::current().block_on(app_state.update_cache());
                            reset_fields = true
                        },
                    }
                }

                if reset_fields {
                    self.fee_per_gram = app_state.get_default_fee_per_gram().as_u64().to_string();
                    self.template_name = "".to_string();
                    self.template_type = "".to_string();
                    self.binary_url = "".to_string();
                    self.binary_checksum = "".to_string();
                    self.repository_url = "".to_string();
                    self.repository_commit_hash = "".to_string();
                    self.input_mode = InputMode::None;
                    self.result_watch = Some(rx);
                }

                self.confirmation_dialog = None;
                return KeyHandled::Handled;
            } else {
                return KeyHandled::Handled;
            }
        }
        KeyHandled::NotHandled
    }

    fn on_key_send_input(&mut self, c: char) -> KeyHandled {
        if self.input_mode != InputMode::None {
            match self.input_mode {
                InputMode::None => (),
                InputMode::TemplateName => match c {
                    '\n' => self.input_mode = InputMode::TemplateVersion,
                    c => {
                        self.template_name.push(c);
                        return KeyHandled::Handled;
                    },
                },
                InputMode::TemplateVersion => match c {
                    '\n' => self.input_mode = InputMode::TemplateType,
                    c => {
                        self.template_version.push(c);
                        return KeyHandled::Handled;
                    },
                },
                InputMode::TemplateType => match c {
                    '\n' => self.input_mode = InputMode::BinaryUrl,
                    c => {
                        self.template_type.push(c.to_uppercase().collect::<Vec<_>>()[0]);
                        return KeyHandled::Handled;
                    },
                },
                InputMode::BinaryUrl => match c {
                    '\n' => {
                        self.input_mode = {
                            if self.repository_url.is_empty() {
                                self.repository_url = match maybe_extract_git_repo(self.binary_url.as_str()) {
                                    None => String::new(),
                                    Some(repository_url) => repository_url,
                                };
                            }

                            if self.template_type.is_empty() {
                                self.repository_url = match maybe_extract_template_type(self.binary_url.as_str()) {
                                    None => String::new(),
                                    Some(assumed_template_type) => assumed_template_type.0,
                                };
                            }

                            InputMode::RepositoryUrl
                        }
                    },
                    c => {
                        self.binary_url.push(c);
                        return KeyHandled::Handled;
                    },
                },
                InputMode::RepositoryUrl => match c {
                    '\n' => self.input_mode = InputMode::BinaryChecksum,
                    c => {
                        self.repository_url.push(c);
                        return KeyHandled::Handled;
                    },
                },
                InputMode::BinaryChecksum => match c {
                    '\n' => self.input_mode = InputMode::RepositoryCommitHash,
                    c => {
                        if c.is_numeric() {
                            self.binary_checksum.push(c);
                        }
                        return KeyHandled::Handled;
                    },
                },
                InputMode::RepositoryCommitHash => match c {
                    '\n' => self.input_mode = InputMode::FeePerGram,
                    c => {
                        if c.is_numeric() {
                            self.repository_commit_hash.push(c);
                        }
                        return KeyHandled::Handled;
                    },
                },
                InputMode::FeePerGram => match c {
                    '\n' => self.input_mode = InputMode::None,
                    c => {
                        if c.is_numeric() || ['t', 'T', 'u', 'U'].contains(&c) {
                            self.fee_per_gram.push(c);
                        }
                        return KeyHandled::Handled;
                    },
                },
            }
        }

        KeyHandled::NotHandled
    }
}

impl<B: Backend> Component<B> for RegisterTemplateTab {
    #[allow(clippy::too_many_lines)]
    fn draw(&mut self, f: &mut Frame<B>, area: Rect, app_state: &AppState) {
        let areas = Layout::default()
            .constraints(
                [
                    Constraint::Length(3),
                    Constraint::Length(18),
                    Constraint::Min(42),
                    Constraint::Length(1),
                    Constraint::Length(1),
                ]
                .as_ref(),
            )
            .split(area);

        self.balance.draw(f, areas[0], app_state);
        self.draw_form(f, areas[1], app_state);

        let rx_option = self.result_watch.take();
        if let Some(rx) = rx_option {
            trace!(target: LOG_TARGET, "{:?}", (*rx.borrow()).clone());
            let status = match (*rx.borrow()).clone() {
                UiTransactionSendStatus::Initiated => "Initiated",
                UiTransactionSendStatus::Error(e) => {
                    self.error_message = Some(format!("Error sending transaction: {}, Press Enter to continue.", e));
                    return;
                },
                UiTransactionSendStatus::TransactionComplete => {
                    self.success_message =
                        Some("Transaction completed successfully!\nPlease press Enter to continue".to_string());
                    return;
                },
                status => {
                    warn!("unhandled transaction status {:?}", status);
                    return;
                },
            };
            draw_dialog(
                f,
                area,
                "Please Wait".to_string(),
                format!("Template Registration Status: {}", status),
                Color::Green,
                120,
                10,
            );
            self.result_watch = Some(rx);
        }

        if let Some(msg) = self.success_message.clone() {
            draw_dialog(f, area, "Success!".to_string(), msg, Color::Green, 120, 9);
        }

        if let Some(msg) = self.offline_message.clone() {
            draw_dialog(f, area, "Offline!".to_string(), msg, Color::Green, 120, 9);
        }

        match self.confirmation_dialog {
            None => (),
            Some(ConfirmationDialogType::Normal) => {
                draw_dialog(
                    f,
                    area,
                    "Confirm Code Template Registration".to_string(),
                    "Are you sure you want to register this template?\n(Y)es / (N)o".to_string(),
                    Color::Red,
                    120,
                    9,
                );
            },
        }

        if let Some(msg) = self.error_message.clone() {
            draw_dialog(f, area, "Error!".to_string(), msg, Color::Red, 120, 9);
        }
    }

    fn on_key(&mut self, app_state: &mut AppState, c: char) {
        if self.error_message.is_some() {
            if '\n' == c {
                self.error_message = None;
            }
            return;
        }

        if self.success_message.is_some() {
            if '\n' == c {
                self.success_message = None;
            }
            return;
        }

        if self.offline_message.is_some() {
            if '\n' == c {
                self.offline_message = None;
            }
            return;
        }

        if self.result_watch.is_some() {
            return;
        }

        if self.on_key_confirmation_dialog(c, app_state) == KeyHandled::Handled {
            return;
        }

        if self.on_key_send_input(c) == KeyHandled::Handled {
            return;
        }

        match c {
            'f' => self.input_mode = InputMode::FeePerGram,
            'n' => self.input_mode = InputMode::TemplateName,
            'v' => self.input_mode = InputMode::TemplateVersion,
            't' => self.input_mode = InputMode::TemplateType,
            'b' => self.input_mode = InputMode::BinaryUrl,
            'c' => self.input_mode = InputMode::BinaryChecksum,
            'u' => self.input_mode = InputMode::RepositoryUrl,
            'h' => self.input_mode = InputMode::RepositoryCommitHash,
            's' => {
                // ----------------------------------------------------------------------------
                // basic field value validation
                // ----------------------------------------------------------------------------

                if self.template_name.is_empty() {
                    self.error_message = Some("Template Name is empty\nPress Enter to continue.".to_string());
                    return;
                }

                if self.template_version.is_empty() {
                    self.error_message = Some("Template Version is empty\nPress Enter to continue.".to_string());
                    return;
                }

                if self.template_type.is_empty() {
                    self.error_message = Some("Template Type is empty\nPress Enter to continue.".to_string());
                    return;
                }

                if self.binary_url.is_empty() {
                    self.error_message = Some("Binary URL is empty\nPress Enter to continue.".to_string());
                    return;
                }

                if self.binary_checksum.is_empty() {
                    self.error_message = Some("Binary Checksum is empty\nPress Enter to continue.".to_string());
                    return;
                }

                if self.repository_url.is_empty() {
                    self.error_message = Some("Repository URL is empty\nPress Enter to continue.".to_string());
                    return;
                }

                if self.repository_commit_hash.is_empty() {
                    self.error_message = Some("Repository Commit Hash is empty\nPress Enter to continue.".to_string());
                    return;
                }

                if self.fee_per_gram.parse::<MicroTari>().is_err() {
                    self.error_message =
                        Some("Fee-Per-Gram should be a valid amount of Tari\nPress Enter to continue.".to_string());
                    return;
                }

                self.confirmation_dialog = Some(ConfirmationDialogType::Normal);
            },
            _ => {},
        }
    }

    fn on_up(&mut self, _app_state: &mut AppState) {}

    fn on_down(&mut self, _app_state: &mut AppState) {}

    fn on_esc(&mut self, _: &mut AppState) {
        if self.confirmation_dialog.is_some() {
            return;
        }

        self.input_mode = InputMode::None;
    }

    fn on_backspace(&mut self, _app_state: &mut AppState) {
        match self.input_mode {
            InputMode::TemplateName => {
                let _ = self.template_name.pop();
            },
            InputMode::TemplateVersion => {
                let _ = self.template_version.pop();
            },
            InputMode::TemplateType => {
                let _ = self.template_type.pop();
            },
            InputMode::BinaryUrl => {
                let _ = self.binary_url.pop();
            },
            InputMode::BinaryChecksum => {
                let _ = self.binary_checksum.pop();
            },
            InputMode::RepositoryUrl => {
                let _ = self.repository_url.pop();
            },
            InputMode::RepositoryCommitHash => {
                let _ = self.repository_commit_hash.pop();
            },
            InputMode::FeePerGram => {
                let _ = self.fee_per_gram.pop();
            },
            InputMode::None => {},
        }
    }
}

#[derive(PartialEq, Debug)]
enum InputMode {
    None,
    TemplateName,
    TemplateVersion,
    TemplateType,
    BinaryUrl,
    BinaryChecksum,
    RepositoryUrl,
    RepositoryCommitHash,
    FeePerGram,
}

#[derive(PartialEq, Debug)]
enum ConfirmationDialogType {
    Normal,
}
