// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::fs;

use log::*;
use minotari_wallet::output_manager_service::UtxoSelectionCriteria;
use tari_core::transactions::tari_amount::MicroMinotari;
use tokio::{runtime::Handle, sync::watch};
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, ListItem, Paragraph, Wrap},
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::ui::{
    components::{balance::Balance, Component, KeyHandled},
    state::{AppState, UiTransactionBurnStatus},
    ui_burnt_proof::UiBurntProof,
    widgets::{draw_dialog, MultiColumnList, WindowedListState},
    MAX_WIDTH,
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
    proofs_list_state: WindowedListState,
    show_proofs: bool,
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
            proofs_list_state: WindowedListState::new(),
            burn_result_watch: None,
            confirmation_dialog: None,
            show_proofs: true,
        }
    }

    // casting here is okay as we only use it for draw widths
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::too_many_lines)]
    fn draw_burn_form<B>(&self, f: &mut Frame<B>, area: Rect, _app_state: &AppState)
    where B: Backend {
        let block = Block::default().borders(Borders::ALL).title(Span::styled(
            "Burn Minotari",
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
                Span::raw(" field,"),
                Span::styled("B", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to view burnt proofs."),
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

    fn draw_proofs<B>(&mut self, f: &mut Frame<B>, area: Rect, app_state: &AppState)
    where B: Backend {
        let block = Block::default().borders(Borders::ALL).title(Span::styled(
            "Burnt Proofs",
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ));
        f.render_widget(block, area);

        let list_areas = Layout::default()
            .constraints([Constraint::Length(1), Constraint::Min(42)].as_ref())
            .margin(1)
            .split(area);

        let instructions = Paragraph::new(Spans::from(vec![
            Span::raw(" Use "),
            Span::styled("Up↑/Down↓ Keys", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" to choose a proof, "),
            Span::styled("O", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" to save proof to a file (named after proof ID), "),
            Span::styled("D", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" to delete the selected proof."),
        ]))
        .wrap(Wrap { trim: true });
        f.render_widget(instructions, list_areas[0]);

        self.proofs_list_state.set_num_items(app_state.get_burnt_proofs().len());

        let mut list_state = self
            .proofs_list_state
            .update_list_state((list_areas[1].height as usize).saturating_sub(3));

        let window = self.proofs_list_state.get_start_end();
        let windowed_view = app_state.get_burnt_proofs_slice(window.0, window.1);

        let column_list = BurnTab::create_column_view(windowed_view);
        column_list.render(f, list_areas[1], &mut list_state);
    }

    // Helper function to create the column list to be rendered
    pub fn create_column_view(windowed_view: &[UiBurntProof]) -> MultiColumnList<Vec<ListItem>> {
        let mut column0_items = Vec::new();
        let mut column1_items = Vec::new();

        for item in windowed_view {
            column0_items.push(ListItem::new(Span::raw(item.reciprocal_claim_public_key.clone())));
            column1_items.push(ListItem::new(Span::raw(item.burned_at.to_string().clone())));
        }

        let column_list = MultiColumnList::new()
            .highlight_style(Style::default().add_modifier(Modifier::BOLD).fg(Color::Magenta))
            .heading_style(Style::default().fg(Color::Magenta))
            .max_width(MAX_WIDTH)
            .add_column(None, Some(1), Vec::new())
            .add_column(Some("Reciprocal Claim Public Key"), Some(66), column0_items)
            .add_column(None, Some(1), Vec::new())
            .add_column(Some("Burned At"), Some(11), column1_items);

        column_list
    }

    #[allow(clippy::too_many_lines)]
    fn on_key_confirmation_dialog(&mut self, c: char, app_state: &mut AppState) -> KeyHandled {
        if self.confirmation_dialog.is_some() {
            if 'n' == c {
                self.confirmation_dialog = None;
                return KeyHandled::Handled;
            } else if 'y' == c {
                if 'y' == c {
                    let amount = self
                        .amount_field
                        .parse::<MicroMinotari>()
                        .unwrap_or_else(|_| MicroMinotari::from(0));

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
                                Ok(_) => {
                                    Handle::current()
                                        .block_on(app_state.refresh_burnt_proofs_state())
                                        .unwrap();
                                    Handle::current().block_on(app_state.update_cache());

                                    reset_fields = true
                                },
                            }
                        },
                        None => {},
                        Some(BurnConfirmationDialogType::DeleteBurntProof(proof_id)) => {
                            match Handle::current().block_on(app_state.delete_burnt_proof(proof_id)) {
                                Err(e) => {
                                    self.error_message = Some(format!(
                                        "Failed to delete burnt proof (id={}):\n{}\nPress Enter to continue.",
                                        proof_id, e
                                    ))
                                },
                                Ok(_) => {
                                    Handle::current()
                                        .block_on(app_state.refresh_burnt_proofs_state())
                                        .unwrap();
                                    Handle::current().block_on(app_state.update_cache());
                                },
                            }
                        },
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
            } else {
                // dont care
            }
        }

        KeyHandled::NotHandled
    }

    fn on_key_send_input(&mut self, c: char) -> KeyHandled {
        if self.burn_input_mode != BurnInputMode::None {
            match self.burn_input_mode {
                BurnInputMode::None => (),
                BurnInputMode::BurntProofPath => match c {
                    '\n' => self.burn_input_mode = BurnInputMode::ClaimPublicKey,
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

    fn on_key_show_proofs(&mut self, c: char, app_state: &mut AppState) -> KeyHandled {
        if !self.show_proofs {
            return KeyHandled::NotHandled;
        }

        match c {
            'd' => {
                if let Some(proof) = self
                    .proofs_list_state
                    .selected()
                    .and_then(|i| app_state.get_burnt_proof_by_index(i))
                    .cloned()
                {
                    if self.proofs_list_state.selected().is_none() {
                        return KeyHandled::NotHandled;
                    }

                    self.confirmation_dialog = Some(BurnConfirmationDialogType::DeleteBurntProof(proof.id));
                }

                return KeyHandled::Handled;
            },

            'o' => {
                if let Some(proof) = self
                    .proofs_list_state
                    .selected()
                    .and_then(|i| app_state.get_burnt_proof_by_index(i))
                    .cloned()
                {
                    if self.proofs_list_state.selected().is_none() {
                        return KeyHandled::NotHandled;
                    }

                    if let Err(e) = fs::write(format!("{}.json", proof.id), proof.payload) {
                        self.error_message = Some(format!(
                            "Failed to save burnt proof payload to file {}.json: {}, Press Enter to continue.",
                            proof.id, e
                        ));
                    }
                }

                return KeyHandled::Handled;
            },

            _ => {},
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

        if self.show_proofs {
            self.draw_proofs(f, areas[2], app_state);
        };

        let rx_option = self.burn_result_watch.take();
        if let Some(rx) = rx_option {
            trace!(target: LOG_TARGET, "{:?}", (*rx.borrow()).clone());
            let status = match (*rx.borrow()).clone() {
                UiTransactionBurnStatus::Initiated => "Initiated",
                UiTransactionBurnStatus::Error(e) => {
                    self.error_message = Some(format!("Error sending transaction: {}, Press Enter to continue.", e));
                    return;
                },
                UiTransactionBurnStatus::TransactionComplete((
                    _proof_id,
                    _reciprocal_claim_public_key,
                    _serialized_proof,
                )) => {
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
                        "Are you sure you want to burn {} Minotari with a Claim Public Key {}?\n(Y)es / (N)o",
                        self.amount_field, self.claim_public_key_field
                    ),
                    Color::Red,
                    120,
                    9,
                );
            },

            Some(BurnConfirmationDialogType::DeleteBurntProof(_proof_id)) => {
                draw_dialog(
                    f,
                    area,
                    "Confirm Delete".to_string(),
                    "Are you sure you want to delete this burnt proof?\n(Y)es / (N)o".to_string(),
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

        if self.on_key_show_proofs(c, app_state) == KeyHandled::Handled {
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
            'b' => {
                self.show_proofs = !self.show_proofs;
            },
            's' => {
                if self.claim_public_key_field.is_empty() {
                    self.error_message = Some("Claim Public Key is empty\nPress Enter to continue.".to_string());
                    return;
                }

                if self.amount_field.parse::<MicroMinotari>().is_err() {
                    self.error_message =
                        Some("Amount should be a valid amount of Minotari\nPress Enter to continue.".to_string());
                    return;
                }

                self.confirmation_dialog = Some(BurnConfirmationDialogType::Normal);
            },
            _ => {},
        }
    }

    fn on_up(&mut self, app_state: &mut AppState) {
        self.proofs_list_state.set_num_items(app_state.get_burnt_proofs().len());
        self.proofs_list_state.previous();
    }

    fn on_down(&mut self, app_state: &mut AppState) {
        self.proofs_list_state.set_num_items(app_state.get_burnt_proofs().len());
        self.proofs_list_state.next();
    }

    fn on_esc(&mut self, _: &mut AppState) {
        if self.confirmation_dialog.is_some() {
            return;
        }

        self.burn_input_mode = BurnInputMode::None;
        self.proofs_list_state.select(None);
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
    DeleteBurntProof(u32),
}
