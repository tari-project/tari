// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::path::Path;

use log::*;
use tari_core::transactions::tari_amount::MicroTari;
use tari_wallet::output_manager_service::UtxoSelectionCriteria;
use tokio::{runtime::Handle, sync::watch};
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, Paragraph, TableState, Wrap},
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::ui::{
    components::{balance::Balance, Component, KeyHandled},
    state::{AppState, UiTransactionBurnStatus},
    widgets::draw_dialog,
};

const LOG_TARGET: &str = "wallet::console_wallet::burn_tab ";

pub struct BurnTab {
    balance: Balance,
    burn_input_mode: BurnInputMode,
    burnt_proof_filepath_field: String,
    claim_public_key_field: String,
    amount_field: String,
    fee_field: String,
    message_field: String,
    error_message: Option<String>,
    success_message: Option<String>,
    offline_message: Option<String>,
    burn_result_watch: Option<watch::Receiver<UiTransactionBurnStatus>>,
    confirmation_dialog: Option<BurnConfirmationDialogType>,
    table_state: TableState,
}

impl BurnTab {
    pub fn new(app_state: &AppState) -> Self {
        Self {
            balance: Balance::new(),
            burn_input_mode: BurnInputMode::None,
            burnt_proof_filepath_field: String::new(),
            claim_public_key_field: String::new(),
            amount_field: String::new(),
            fee_field: app_state.get_default_fee_per_gram().as_u64().to_string(),
            message_field: String::new(),
            error_message: None,
            success_message: None,
            offline_message: None,
            burn_result_watch: None,
            confirmation_dialog: None,
            table_state: TableState::default(),
        }
    }

    #[allow(clippy::too_many_lines)]
    fn draw_burn_form<B>(&self, f: &mut Frame<B>, area: Rect, _app_state: &AppState)
    where B: Backend {
        let block = Block::default().borders(Borders::ALL).title(Span::styled(
            "Burn Tari",
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ));
        f.render_widget(block, area);

        let vert_chunks = Layout::default()
            .constraints(
                [
                    Constraint::Length(3),
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
                Span::styled("P", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to edit "),
                Span::styled("Burn Proof Filepath", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" field, "),
                Span::styled("C", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to edit "),
                Span::styled("Claim Public Key", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" field, "),
                Span::styled("A", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to edit "),
                Span::styled("Amount", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" and "),
                Span::styled("F", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to edit "),
                Span::styled("Fee-Per-Gram", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" field."),
            ]),
            Spans::from(vec![
                Span::raw("Press "),
                Span::styled("S", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to send a burn transaction."),
            ]),
        ])
        .wrap(Wrap { trim: false })
        .block(Block::default());
        f.render_widget(instructions, vert_chunks[0]);

        let burnt_proof_filepath_input = Paragraph::new(self.burnt_proof_filepath_field.as_ref())
            .style(match self.burn_input_mode {
                BurnInputMode::BurntProofPath => Style::default().fg(Color::Magenta),
                _ => Style::default(),
            })
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Save burn proof to file(p)ath:"),
            );
        f.render_widget(burnt_proof_filepath_input, vert_chunks[1]);

        let claim_public_key_input = Paragraph::new(self.claim_public_key_field.as_ref())
            .style(match self.burn_input_mode {
                BurnInputMode::ClaimPublicKey => Style::default().fg(Color::Magenta),
                _ => Style::default(),
            })
            .block(Block::default().borders(Borders::ALL).title("To (C)laim Public Key:"));
        f.render_widget(claim_public_key_input, vert_chunks[2]);

        let amount_fee_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
            .split(vert_chunks[3]);

        let amount_input = Paragraph::new(self.amount_field.to_string())
            .style(match self.burn_input_mode {
                BurnInputMode::Amount => Style::default().fg(Color::Magenta),
                _ => Style::default(),
            })
            .block(Block::default().borders(Borders::ALL).title("(A)mount:"));
        f.render_widget(amount_input, amount_fee_layout[0]);

        let fee_input = Paragraph::new(self.fee_field.as_ref())
            .style(match self.burn_input_mode {
                BurnInputMode::Fee => Style::default().fg(Color::Magenta),
                _ => Style::default(),
            })
            .block(Block::default().borders(Borders::ALL).title("(F)ee-per-gram (uT):"));
        f.render_widget(fee_input, amount_fee_layout[1]);

        let message_input = Paragraph::new(self.message_field.as_ref())
            .style(match self.burn_input_mode {
                BurnInputMode::Message => Style::default().fg(Color::Magenta),
                _ => Style::default(),
            })
            .block(Block::default().borders(Borders::ALL).title("(M)essage:"));
        f.render_widget(message_input, vert_chunks[4]);

        match self.burn_input_mode {
            BurnInputMode::None => (),
            BurnInputMode::BurntProofPath => f.set_cursor(
                // Put cursor past the end of the input text
                vert_chunks[1].x + self.burnt_proof_filepath_field.width() as u16 + 1,
                // Move one line down, from the border to the input line
                vert_chunks[1].y + 1,
            ),
            BurnInputMode::ClaimPublicKey => f.set_cursor(
                // Put cursor past the end of the input text
                vert_chunks[2].x + self.claim_public_key_field.width() as u16 + 1,
                // Move one line down, from the border to the input line
                vert_chunks[2].y + 1,
            ),
            BurnInputMode::Amount => {
                f.set_cursor(
                    // Put cursor past the end of the input text
                    amount_fee_layout[0].x + self.amount_field.width() as u16 + 1,
                    // Move one line down, from the border to the input line
                    amount_fee_layout[0].y + 1,
                )
            },
            BurnInputMode::Fee => f.set_cursor(
                // Put cursor past the end of the input text
                amount_fee_layout[1].x + self.fee_field.width() as u16 + 1,
                // Move one line down, from the border to the input line
                amount_fee_layout[1].y + 1,
            ),
            BurnInputMode::Message => f.set_cursor(
                // Put cursor past the end of the input text
                vert_chunks[4].x + self.message_field.width() as u16 + 1,
                // Move one line down, from the border to the input line
                vert_chunks[4].y + 1,
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
                match self.confirmation_dialog {
                    None => (),
                    Some(BurnConfirmationDialogType::Normal) => {
                        if 'y' == c {
                            let amount = self.amount_field.parse::<MicroTari>().unwrap_or(MicroTari::from(0));

                            let fee_per_gram = if let Ok(v) = self.fee_field.parse::<u64>() {
                                v
                            } else {
                                self.error_message =
                                    Some("Fee-per-gram should be an integer\nPress Enter to continue.".to_string());
                                return KeyHandled::Handled;
                            };

                            let burn_proof_filepath = if self.burnt_proof_filepath_field.is_empty() {
                                None
                            } else {
                                Some(self.burnt_proof_filepath_field.clone())
                            };

                            let claim_public_key = if self.claim_public_key_field.is_empty() {
                                None
                            } else {
                                Some(self.claim_public_key_field.clone())
                            };

                            let (tx, rx) = watch::channel(UiTransactionBurnStatus::Initiated);

                            let mut reset_fields = false;
                            match self.confirmation_dialog {
                                Some(BurnConfirmationDialogType::Normal) => {
                                    match Handle::current().block_on(app_state.send_burn_transaction(
                                        burn_proof_filepath,
                                        claim_public_key,
                                        amount.into(),
                                        UtxoSelectionCriteria::default(),
                                        fee_per_gram,
                                        self.message_field.clone(),
                                        tx,
                                    )) {
                                        Err(e) => {
                                            self.error_message = Some(format!(
                                                "Error sending burn transaction (with a claim public key \
                                                 provided):\n{}\nPress Enter to continue.",
                                                e
                                            ))
                                        },
                                        Ok(_) => reset_fields = true,
                                    }
                                },
                                None => {},
                            }

                            if reset_fields {
                                self.burnt_proof_filepath_field = "".to_string();
                                self.claim_public_key_field = "".to_string();
                                self.amount_field = "".to_string();
                                self.fee_field = app_state.get_default_fee_per_gram().as_u64().to_string();
                                self.message_field = "".to_string();
                                self.burn_input_mode = BurnInputMode::None;
                                self.burn_result_watch = Some(rx);
                            }

                            self.confirmation_dialog = None;
                            return KeyHandled::Handled;
                        }
                    },
                }
            } else {
            }
        }

        KeyHandled::NotHandled
    }

    fn on_key_send_input(&mut self, c: char) -> KeyHandled {
        if self.burn_input_mode != BurnInputMode::None {
            match self.burn_input_mode {
                BurnInputMode::None => (),
                BurnInputMode::BurntProofPath => match c {
                    '\n' => self.burn_input_mode = BurnInputMode::Amount,
                    c => {
                        self.burnt_proof_filepath_field.push(c);
                        return KeyHandled::Handled;
                    },
                },
                BurnInputMode::ClaimPublicKey => match c {
                    '\n' => self.burn_input_mode = BurnInputMode::Amount,
                    c => {
                        self.claim_public_key_field.push(c);
                        return KeyHandled::Handled;
                    },
                },
                BurnInputMode::Amount => match c {
                    '\n' => self.burn_input_mode = BurnInputMode::Message,
                    c => {
                        if c.is_numeric() || ['t', 'T', 'u', 'U'].contains(&c) {
                            self.amount_field.push(c);
                        }
                        return KeyHandled::Handled;
                    },
                },
                BurnInputMode::Fee => match c {
                    '\n' => self.burn_input_mode = BurnInputMode::None,
                    c => {
                        if c.is_numeric() {
                            self.fee_field.push(c);
                        }
                        return KeyHandled::Handled;
                    },
                },
                BurnInputMode::Message => match c {
                    '\n' => self.burn_input_mode = BurnInputMode::None,
                    c => {
                        self.message_field.push(c);
                        return KeyHandled::Handled;
                    },
                },
            }
        }

        KeyHandled::NotHandled
    }
}

impl<B: Backend> Component<B> for BurnTab {
    #[allow(clippy::too_many_lines)]
    fn draw(&mut self, f: &mut Frame<B>, area: Rect, app_state: &AppState) {
        let areas = Layout::default()
            .constraints(
                [
                    Constraint::Length(3),
                    Constraint::Length(17),
                    Constraint::Min(42),
                    Constraint::Length(1),
                    Constraint::Length(1),
                ]
                .as_ref(),
            )
            .split(area);

        self.balance.draw(f, areas[0], app_state);
        self.draw_burn_form(f, areas[1], app_state);

        let rx_option = self.burn_result_watch.take();
        if let Some(rx) = rx_option {
            trace!(target: LOG_TARGET, "{:?}", (*rx.borrow()).clone());
            let status = match (*rx.borrow()).clone() {
                UiTransactionBurnStatus::Initiated => "Initiated",
                UiTransactionBurnStatus::DiscoveryInProgress => "Discovery In Progress",
                UiTransactionBurnStatus::Error(e) => {
                    self.error_message = Some(format!("Error sending transaction: {}, Press Enter to continue.", e));
                    return;
                },
                UiTransactionBurnStatus::SentDirect | UiTransactionBurnStatus::SentViaSaf => {
                    self.success_message =
                        Some("Transaction successfully sent!\nPlease press Enter to continue".to_string());
                    return;
                },
                UiTransactionBurnStatus::Queued => {
                    self.offline_message = Some(
                        "This wallet appears to be offline; transaction queued for further retry sending.\n Please \
                         press Enter to continue"
                            .to_string(),
                    );
                    return;
                },
                UiTransactionBurnStatus::TransactionComplete => {
                    self.success_message =
                        Some("Transaction completed successfully!\nPlease press Enter to continue".to_string());
                    return;
                },
            };
            draw_dialog(
                f,
                area,
                "Please Wait".to_string(),
                format!("Transaction Burn Status: {}", status),
                Color::Green,
                120,
                10,
            );
            self.burn_result_watch = Some(rx);
        }

        if let Some(msg) = self.success_message.clone() {
            draw_dialog(f, area, "Success!".to_string(), msg, Color::Green, 120, 9);
        }

        if let Some(msg) = self.offline_message.clone() {
            draw_dialog(f, area, "Offline!".to_string(), msg, Color::Green, 120, 9);
        }

        if let Some(msg) = self.error_message.clone() {
            draw_dialog(f, area, "Error!".to_string(), msg, Color::Red, 120, 9);
        }

        match self.confirmation_dialog {
            None => (),
            Some(BurnConfirmationDialogType::Normal) => {
                draw_dialog(
                    f,
                    area,
                    "Confirm Burning Transaction".to_string(),
                    format!(
                        "Are you sure you want to burn {} Tari with a Claim Public Key {}?\n(Y)es / (N)o",
                        self.amount_field, self.claim_public_key_field
                    ),
                    Color::Red,
                    120,
                    9,
                );
            },
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

        if self.burn_result_watch.is_some() {
            return;
        }

        if self.on_key_confirmation_dialog(c, app_state) == KeyHandled::Handled {
            return;
        }

        if self.on_key_send_input(c) == KeyHandled::Handled {
            return;
        }

        match c {
            'p' => self.burn_input_mode = BurnInputMode::BurntProofPath,
            'c' => self.burn_input_mode = BurnInputMode::ClaimPublicKey,
            'a' => {
                self.burn_input_mode = BurnInputMode::Amount;
            },
            'f' => self.burn_input_mode = BurnInputMode::Fee,
            'm' => self.burn_input_mode = BurnInputMode::Message,
            's' => {
                // if self.burnt_proof_filepath_field.is_empty() {
                // self.error_message = Some("Burn proof filepath is empty\nPress Enter to continue.".to_string());
                // return;
                // }

                if self.claim_public_key_field.is_empty() {
                    self.error_message = Some("Claim Public Key is empty\nPress Enter to continue.".to_string());
                    return;
                }

                if self.amount_field.parse::<MicroTari>().is_err() {
                    self.error_message =
                        Some("Amount should be a valid amount of Tari\nPress Enter to continue.".to_string());
                    return;
                }

                self.confirmation_dialog = Some(match c {
                    _ => BurnConfirmationDialogType::Normal,
                });
            },
            _ => {},
        }
    }

    fn on_up(&mut self, _app_state: &mut AppState) {
        let index = self.table_state.selected().unwrap_or_default();
        if index == 0 {
            self.table_state.select(None);
        }
    }

    fn on_down(&mut self, _app_state: &mut AppState) {
        self.table_state.select(None);
    }

    fn on_esc(&mut self, _: &mut AppState) {
        self.burn_input_mode = BurnInputMode::None;
    }

    fn on_backspace(&mut self, _app_state: &mut AppState) {
        match self.burn_input_mode {
            BurnInputMode::BurntProofPath => {
                let _ = self.burnt_proof_filepath_field.pop();
            },
            BurnInputMode::ClaimPublicKey => {
                let _ = self.claim_public_key_field.pop();
            },
            BurnInputMode::Amount => {
                let _ = self.amount_field.pop();
            },
            BurnInputMode::Fee => {
                let _ = self.fee_field.pop();
            },
            BurnInputMode::Message => {
                let _ = self.message_field.pop();
            },
            BurnInputMode::None => {},
        }
    }
}

#[derive(PartialEq, Debug)]
pub enum BurnInputMode {
    None,
    BurntProofPath,
    ClaimPublicKey,
    Amount,
    Message,
    Fee,
}

#[derive(PartialEq, Debug)]
pub enum BurnConfirmationDialogType {
    Normal,
}
