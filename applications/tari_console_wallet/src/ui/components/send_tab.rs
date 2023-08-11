// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use log::*;
use tari_core::transactions::tari_amount::MicroTari;
use tari_utilities::hex::Hex;
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
    components::{balance::Balance, contacts_tab::ContactsTab, Component, KeyHandled},
    state::{AppState, UiTransactionSendStatus},
    widgets::{draw_dialog, WindowedListState},
};

const LOG_TARGET: &str = "wallet::console_wallet::send_tab ";

pub struct SendTab {
    balance: Balance,
    send_input_mode: SendInputMode,
    show_contacts: bool,
    to_field: String,
    amount_field: String,
    fee_field: String,
    message_field: String,
    error_message: Option<String>,
    success_message: Option<String>,
    offline_message: Option<String>,
    contacts_list_state: WindowedListState,
    send_result_watch: Option<watch::Receiver<UiTransactionSendStatus>>,
    confirmation_dialog: Option<ConfirmationDialogType>,
    selected_unique_id: Option<Vec<u8>>,
    table_state: TableState,
}

impl SendTab {
    pub fn new(app_state: &AppState) -> Self {
        Self {
            balance: Balance::new(),
            send_input_mode: SendInputMode::None,
            show_contacts: false,
            to_field: String::new(),
            amount_field: String::new(),
            fee_field: app_state.get_default_fee_per_gram().as_u64().to_string(),
            message_field: String::new(),
            error_message: None,
            success_message: None,
            offline_message: None,
            contacts_list_state: WindowedListState::new(),
            send_result_watch: None,
            confirmation_dialog: None,
            selected_unique_id: None,
            table_state: TableState::default(),
        }
    }

    #[allow(clippy::too_many_lines)]
    fn draw_send_form<B>(&self, f: &mut Frame<B>, area: Rect, _app_state: &AppState)
    where B: Backend {
        let block = Block::default().borders(Borders::ALL).title(Span::styled(
            "Send Transaction",
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
                ]
                .as_ref(),
            )
            .margin(1)
            .split(area);
        let instructions = Paragraph::new(vec![
            Spans::from(vec![
                Span::raw("Press "),
                Span::styled("T", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to edit "),
                Span::styled("To", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" field, "),
                Span::styled("A", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to edit "),
                Span::styled("Amount/Token", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(", "),
                Span::styled("F", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to edit "),
                Span::styled("Fee-Per-Gram", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" field, "),
                Span::styled("C", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to select a contact."),
            ]),
            Spans::from(vec![
                Span::raw("Press "),
                Span::styled("S", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to send a normal transaction, "),
                Span::styled("O", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to send a one-sided transaction, "),
                Span::styled("X", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to send a one-sided transaction to a stealth address."),
            ]),
        ])
        .wrap(Wrap { trim: false })
        .block(Block::default());
        f.render_widget(instructions, vert_chunks[0]);

        let to_input = Paragraph::new(self.to_field.as_ref())
            .style(match self.send_input_mode {
                SendInputMode::To => Style::default().fg(Color::Magenta),
                _ => Style::default(),
            })
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("(T)o (Tari Address or Emoji ID) :"),
            );
        f.render_widget(to_input, vert_chunks[1]);

        let amount_fee_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
            .split(vert_chunks[2]);

        let amount_input = Paragraph::new(match &self.selected_unique_id {
            Some(token) => format!("Token selected : {}", token.to_hex()),
            None => self.amount_field.to_string(),
        })
        .style(match self.send_input_mode {
            SendInputMode::Amount => Style::default().fg(Color::Magenta),
            _ => Style::default(),
        })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("(A)mount (uT or T) or select Token:"),
        );
        f.render_widget(amount_input, amount_fee_layout[0]);

        let fee_input = Paragraph::new(self.fee_field.as_ref())
            .style(match self.send_input_mode {
                SendInputMode::Fee => Style::default().fg(Color::Magenta),
                _ => Style::default(),
            })
            .block(Block::default().borders(Borders::ALL).title("(F)ee-per-gram (uT):"));
        f.render_widget(fee_input, amount_fee_layout[1]);

        let message_input = Paragraph::new(self.message_field.as_ref())
            .style(match self.send_input_mode {
                SendInputMode::Message => Style::default().fg(Color::Magenta),
                _ => Style::default(),
            })
            .block(Block::default().borders(Borders::ALL).title("(M)essage:"));
        f.render_widget(message_input, vert_chunks[3]);

        match self.send_input_mode {
            SendInputMode::None => (),
            SendInputMode::To => f.set_cursor(
                // Put cursor past the end of the input text
                vert_chunks[1].x + self.to_field.width() as u16 + 1,
                // Move one line down, from the border to the input line
                vert_chunks[1].y + 1,
            ),
            SendInputMode::Amount => {
                if self.selected_unique_id.is_none() {
                    f.set_cursor(
                        // Put cursor past the end of the input text
                        amount_fee_layout[0].x + self.amount_field.width() as u16 + 1,
                        // Move one line down, from the border to the input line
                        amount_fee_layout[0].y + 1,
                    )
                }
            },
            SendInputMode::Fee => f.set_cursor(
                // Put cursor past the end of the input text
                amount_fee_layout[1].x + self.fee_field.width() as u16 + 1,
                // Move one line down, from the border to the input line
                amount_fee_layout[1].y + 1,
            ),
            SendInputMode::Message => f.set_cursor(
                // Put cursor past the end of the input text
                vert_chunks[3].x + self.message_field.width() as u16 + 1,
                // Move one line down, from the border to the input line
                vert_chunks[3].y + 1,
            ),
        }
    }

    #[allow(dead_code)]
    fn draw_contacts<B>(&mut self, f: &mut Frame<B>, area: Rect, app_state: &AppState)
    where B: Backend {
        let block = Block::default().borders(Borders::ALL).title(Span::styled(
            "Contacts",
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
            Span::raw(" to choose a contact, "),
            Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" to select."),
        ]))
        .wrap(Wrap { trim: true });
        f.render_widget(instructions, list_areas[0]);
        self.contacts_list_state.set_num_items(app_state.get_contacts().len());
        let mut list_state = self
            .contacts_list_state
            .get_list_state((list_areas[1].height as usize).saturating_sub(3));
        let window = self.contacts_list_state.get_start_end();
        let windowed_view = app_state.get_contacts_slice(window.0, window.1);

        let column_list = ContactsTab::create_column_view(windowed_view);
        column_list.render(f, list_areas[1], &mut list_state);
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
                    Some(ConfirmationDialogType::Normal) |
                    Some(ConfirmationDialogType::OneSided) |
                    Some(ConfirmationDialogType::StealthAddress) => {
                        if 'y' == c {
                            let amount = if let Ok(v) = self.amount_field.parse::<MicroTari>() {
                                v
                            } else {
                                if self.selected_unique_id.is_none() {
                                    self.error_message =
                                        Some("Amount should be an integer\nPress Enter to continue.".to_string());
                                    return KeyHandled::Handled;
                                }
                                MicroTari::from(0)
                            };

                            let fee_per_gram = if let Ok(v) = self.fee_field.parse::<u64>() {
                                v
                            } else {
                                self.error_message =
                                    Some("Fee-per-gram should be an integer\nPress Enter to continue.".to_string());
                                return KeyHandled::Handled;
                            };

                            let (tx, rx) = watch::channel(UiTransactionSendStatus::Initiated);

                            let mut reset_fields = false;
                            match self.confirmation_dialog {
                                Some(ConfirmationDialogType::OneSided) => {
                                    match Handle::current().block_on(app_state.send_one_sided_transaction(
                                        self.to_field.clone(),
                                        amount.into(),
                                        UtxoSelectionCriteria::default(),
                                        fee_per_gram,
                                        self.message_field.clone(),
                                        tx,
                                    )) {
                                        Err(e) => {
                                            self.error_message = Some(format!(
                                                "Error sending one-sided transaction:\n{}\nPress Enter to continue.",
                                                e
                                            ))
                                        },
                                        Ok(_) => reset_fields = true,
                                    }
                                },
                                Some(ConfirmationDialogType::StealthAddress) => {
                                    match Handle::current().block_on(
                                        app_state.send_one_sided_to_stealth_address_transaction(
                                            self.to_field.clone(),
                                            amount.into(),
                                            UtxoSelectionCriteria::default(),
                                            fee_per_gram,
                                            self.message_field.clone(),
                                            tx,
                                        ),
                                    ) {
                                        Err(e) => {
                                            self.error_message = Some(format!(
                                                "Error sending one-sided transaction to stealth address:\n{}\nPress \
                                                 Enter to continue.",
                                                e
                                            ))
                                        },
                                        Ok(_) => reset_fields = true,
                                    }
                                },
                                _ => {
                                    match Handle::current().block_on(app_state.send_transaction(
                                        self.to_field.clone(),
                                        amount.into(),
                                        UtxoSelectionCriteria::default(),
                                        fee_per_gram,
                                        self.message_field.clone(),
                                        tx,
                                    )) {
                                        Err(e) => {
                                            self.error_message = Some(format!(
                                                "Error sending normal transaction:\n{}\nPress Enter to continue.",
                                                e
                                            ))
                                        },
                                        Ok(_) => reset_fields = true,
                                    }
                                },
                            }
                            if reset_fields {
                                self.to_field = "".to_string();
                                self.amount_field = "".to_string();
                                self.selected_unique_id = None;
                                self.fee_field = app_state.get_default_fee_per_gram().as_u64().to_string();
                                self.message_field = "".to_string();
                                self.send_input_mode = SendInputMode::None;
                                self.send_result_watch = Some(rx);
                            }
                            self.confirmation_dialog = None;
                            return KeyHandled::Handled;
                        }
                    },
                }
            } else {
                // Dont care
            }
        }

        KeyHandled::NotHandled
    }

    fn on_key_send_input(&mut self, c: char) -> KeyHandled {
        if self.send_input_mode != SendInputMode::None {
            match self.send_input_mode {
                SendInputMode::None => (),
                SendInputMode::To => match c {
                    '\n' => self.send_input_mode = SendInputMode::Amount,
                    c => {
                        self.to_field.push(c);
                        return KeyHandled::Handled;
                    },
                },
                SendInputMode::Amount => match c {
                    '\n' => {
                        if self.selected_unique_id.is_some() {
                            self.amount_field = "".to_string();
                        }
                        self.send_input_mode = SendInputMode::Message
                    },
                    c => {
                        if self.selected_unique_id.is_none() {
                            let symbols = &['t', 'T', 'u', 'U'];
                            if c.is_numeric() || symbols.contains(&c) {
                                self.amount_field.push(c);
                            }
                        }
                        return KeyHandled::Handled;
                    },
                },
                SendInputMode::Fee => match c {
                    '\n' => self.send_input_mode = SendInputMode::None,
                    c => {
                        if c.is_numeric() {
                            self.fee_field.push(c);
                        }
                        return KeyHandled::Handled;
                    },
                },
                SendInputMode::Message => match c {
                    '\n' => self.send_input_mode = SendInputMode::None,
                    c => {
                        self.message_field.push(c);
                        return KeyHandled::Handled;
                    },
                },
            }
        }

        KeyHandled::NotHandled
    }

    fn on_key_show_contacts(&mut self, c: char, app_state: &mut AppState) -> KeyHandled {
        if self.show_contacts && c == '\n' {
            if let Some(c) = self
                .contacts_list_state
                .selected()
                .and_then(|i| app_state.get_contact(i))
                .cloned()
            {
                self.to_field = c.address;
                self.send_input_mode = SendInputMode::Amount;
                self.show_contacts = false;
            }
            return KeyHandled::Handled;
        }

        KeyHandled::NotHandled
    }
}

impl<B: Backend> Component<B> for SendTab {
    #[allow(clippy::too_many_lines)]
    fn draw(&mut self, f: &mut Frame<B>, area: Rect, app_state: &AppState) {
        let areas = Layout::default()
            .constraints(
                [
                    Constraint::Length(3),
                    Constraint::Length(14),
                    Constraint::Min(42),
                    Constraint::Length(1),
                ]
                .as_ref(),
            )
            .split(area);

        self.balance.draw(f, areas[0], app_state);
        self.draw_send_form(f, areas[1], app_state);

        if self.show_contacts {
            self.draw_contacts(f, areas[2], app_state);
        };

        let rx_option = self.send_result_watch.take();
        if let Some(rx) = rx_option {
            trace!(target: LOG_TARGET, "{:?}", (*rx.borrow()).clone());
            let status = match (*rx.borrow()).clone() {
                UiTransactionSendStatus::Initiated => "Initiated",
                UiTransactionSendStatus::DiscoveryInProgress => "Discovery In Progress",
                UiTransactionSendStatus::Error(e) => {
                    self.error_message = Some(format!("Error sending transaction: {}, Press Enter to continue.", e));
                    return;
                },
                UiTransactionSendStatus::SentDirect | UiTransactionSendStatus::SentViaSaf => {
                    self.success_message =
                        Some("Transaction successfully sent!\nPlease press Enter to continue".to_string());
                    return;
                },
                UiTransactionSendStatus::Queued => {
                    self.offline_message = Some(
                        "This wallet appears to be offline; transaction queued for further retry sending.\n Please \
                         press Enter to continue"
                            .to_string(),
                    );
                    return;
                },
                UiTransactionSendStatus::TransactionComplete => {
                    self.success_message =
                        Some("Transaction completed successfully!\nPlease press Enter to continue".to_string());
                    return;
                },
            };
            draw_dialog(
                f,
                area,
                "Please Wait".to_string(),
                format!("Transaction Send Status: {}", status),
                Color::Green,
                120,
                10,
            );
            self.send_result_watch = Some(rx);
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
            Some(ConfirmationDialogType::Normal) => {
                draw_dialog(
                    f,
                    area,
                    "Confirm Sending Transaction".to_string(),
                    "Are you sure you want to send this normal transaction?\n(Y)es / (N)o".to_string(),
                    Color::Red,
                    120,
                    9,
                );
            },
            Some(ConfirmationDialogType::OneSided) => {
                draw_dialog(
                    f,
                    area,
                    "Confirm Sending Transaction".to_string(),
                    "Are you sure you want to send this one-sided transaction?\n(Y)es / (N)o".to_string(),
                    Color::Red,
                    120,
                    9,
                );
            },
            Some(ConfirmationDialogType::StealthAddress) => {
                draw_dialog(
                    f,
                    area,
                    "Confirm Sending Transaction".to_string(),
                    "Are you sure you want to send this one-sided transaction to a stealth address?\n(Y)es / (N)o"
                        .to_string(),
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

        if self.send_result_watch.is_some() {
            return;
        }

        if self.on_key_confirmation_dialog(c, app_state) == KeyHandled::Handled {
            return;
        }

        if self.on_key_send_input(c) == KeyHandled::Handled {
            return;
        }

        if self.on_key_show_contacts(c, app_state) == KeyHandled::Handled {
            return;
        }

        match c {
            'c' => {
                self.show_contacts = !self.show_contacts;
            },
            't' => self.send_input_mode = SendInputMode::To,
            'a' => {
                self.send_input_mode = SendInputMode::Amount;
            },
            'f' => self.send_input_mode = SendInputMode::Fee,
            'm' => self.send_input_mode = SendInputMode::Message,
            's' | 'o' | 'x' => {
                if self.to_field.is_empty() {
                    self.error_message =
                        Some("Destination Tari Address/Emoji ID\nPress Enter to continue.".to_string());
                    return;
                }
                if self.amount_field.is_empty() && self.selected_unique_id.is_none() {
                    self.error_message = Some("Amount or token required\nPress Enter to continue.".to_string());
                    return;
                }
                if self.amount_field.parse::<MicroTari>().is_err() && self.selected_unique_id.is_none() {
                    self.error_message =
                        Some("Amount should be a valid amount of Tari\nPress Enter to continue.".to_string());
                    return;
                }

                self.confirmation_dialog = Some(match c {
                    'o' => ConfirmationDialogType::OneSided,
                    'x' => ConfirmationDialogType::StealthAddress,
                    _ => ConfirmationDialogType::Normal,
                });
            },
            _ => {},
        }
    }

    fn on_up(&mut self, app_state: &mut AppState) {
        if self.show_contacts {
            self.contacts_list_state.set_num_items(app_state.get_contacts().len());
            self.contacts_list_state.previous();
        } else if self.send_input_mode == SendInputMode::Amount {
            let index = self.table_state.selected().unwrap_or_default();
            if index == 0 {
                self.table_state.select(None);
            }
        } else {
            // Dont care
        }
    }

    fn on_down(&mut self, app_state: &mut AppState) {
        if self.show_contacts {
            self.contacts_list_state.set_num_items(app_state.get_contacts().len());
            self.contacts_list_state.next();
        } else if self.send_input_mode == SendInputMode::Amount {
            self.table_state.select(None);
        } else {
            // Dont care
        }
    }

    fn on_esc(&mut self, _: &mut AppState) {
        self.send_input_mode = SendInputMode::None;
        self.show_contacts = false;
    }

    fn on_backspace(&mut self, _app_state: &mut AppState) {
        match self.send_input_mode {
            SendInputMode::To => {
                let _ = self.to_field.pop();
            },
            SendInputMode::Amount => {
                if self.selected_unique_id.is_none() {
                    let _ = self.amount_field.pop();
                }
            },
            SendInputMode::Fee => {
                let _ = self.fee_field.pop();
            },
            SendInputMode::Message => {
                let _ = self.message_field.pop();
            },
            SendInputMode::None => {},
        }
    }
}

#[derive(PartialEq, Debug)]
pub enum SendInputMode {
    None,
    To,
    Amount,
    Message,
    Fee,
}

#[derive(PartialEq, Debug)]
pub enum ConfirmationDialogType {
    Normal,
    OneSided,
    StealthAddress,
}
