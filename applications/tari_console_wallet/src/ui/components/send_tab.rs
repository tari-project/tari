use tari_core::transactions::tari_amount::MicroTari;
use tari_utilities::hex::Hex;
use tari_wallet::tokens::Token;
use tokio::{runtime::Handle, sync::watch};
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, ListItem, Paragraph, Row, Table, TableState, Wrap},
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::{
    ui::{
        components::{balance::Balance, styles, Component, KeyHandled},
        state::{AppState, UiTransactionSendStatus},
        widgets::{draw_dialog, MultiColumnList, WindowedListState},
        MAX_WIDTH,
    },
    utils::formatting::display_compressed_string,
};

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
            contacts_list_state: WindowedListState::new(),
            send_result_watch: None,
            confirmation_dialog: None,
            selected_unique_id: None,
            table_state: TableState::default(),
        }
    }

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
                Span::raw(" to send a one-sided transaction."),
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
                    .title("(T)o (Public Key or Emoji ID) :"),
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

        let mut column0_items = Vec::new();
        let mut column1_items = Vec::new();
        let mut column2_items = Vec::new();
        for c in windowed_view.iter() {
            column0_items.push(ListItem::new(Span::raw(c.alias.clone())));
            column1_items.push(ListItem::new(Span::raw(c.public_key.to_string())));
            column2_items.push(ListItem::new(Span::raw(display_compressed_string(
                c.emoji_id.clone(),
                3,
                3,
            ))));
        }
        let column_list = MultiColumnList::new()
            .highlight_style(Style::default().add_modifier(Modifier::BOLD).fg(Color::Magenta))
            .heading_style(Style::default().fg(Color::Magenta))
            .max_width(MAX_WIDTH)
            .add_column(Some("Alias"), Some(25), column0_items)
            .add_column(None, Some(2), Vec::new())
            .add_column(Some("Public Key"), Some(64), column1_items)
            .add_column(None, Some(2), Vec::new())
            .add_column(Some("Emoji ID"), None, column2_items);
        column_list.render(f, list_areas[1], &mut list_state);
    }

    fn draw_tokens<B>(&mut self, f: &mut Frame<B>, area: Rect, app_state: &AppState)
    where B: Backend {
        let tokens = app_state.get_owned_tokens();

        let tokens: Vec<_> = tokens
            .iter()
            .filter(|&token| token.output_status() == "Unspent")
            .map(|r| {
                (
                    r.name().to_string(),
                    r.output_status().to_string(),
                    r.asset_public_key().to_hex(),
                    Vec::from(r.unique_id()).to_hex(),
                    r.owner_commitment().to_hex(),
                )
            })
            .collect();
        let rows: Vec<_> = tokens
            .iter()
            .map(|v| {
                Row::new(vec![
                    v.0.as_str(),
                    v.1.as_str(),
                    v.2.as_str(),
                    v.3.as_str(),
                    v.4.as_str(),
                ])
            })
            .collect();
        let table = Table::new(rows)
            .header(Row::new(vec!["Name", "Status", "Asset Pub Key", "Unique ID", "Owner"]).style(styles::header_row()))
            .block(Block::default().title("Tokens").borders(Borders::ALL))
            .widths(&[
                Constraint::Length(30),
                Constraint::Length(20),
                Constraint::Length(32),
                Constraint::Length(32),
                Constraint::Length(64),
            ])
            .highlight_style(styles::highlight())
            .highlight_symbol(">>");
        f.render_stateful_widget(table, area, &mut self.table_state)
    }

    fn on_key_confirmation_dialog(&mut self, c: char, app_state: &mut AppState) -> KeyHandled {
        if self.confirmation_dialog.is_some() {
            if 'n' == c {
                self.confirmation_dialog = None;
                return KeyHandled::Handled;
            } else if 'y' == c {
                let one_sided_transaction =
                    matches!(self.confirmation_dialog, Some(ConfirmationDialogType::OneSidedSend));
                match self.confirmation_dialog {
                    None => (),
                    Some(ConfirmationDialogType::NormalSend) | Some(ConfirmationDialogType::OneSidedSend) => {
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
                            if one_sided_transaction {
                                match Handle::current().block_on(app_state.send_one_sided_transaction(
                                    self.to_field.clone(),
                                    amount.into(),
                                    self.selected_unique_id.clone(),
                                    None,
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
                            } else {
                                match Handle::current().block_on(app_state.send_transaction(
                                    self.to_field.clone(),
                                    amount.into(),
                                    self.selected_unique_id.clone(),
                                    None,
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
                self.to_field = c.public_key;
                self.send_input_mode = SendInputMode::Amount;
                self.show_contacts = false;
            }
            return KeyHandled::Handled;
        }

        KeyHandled::NotHandled
    }
}

impl<B: Backend> Component<B> for SendTab {
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

        if self.send_input_mode == SendInputMode::Amount {
            self.draw_tokens(f, areas[2], app_state);
        }

        let rx_option = self.send_result_watch.take();
        if let Some(rx) = rx_option {
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

        if let Some(msg) = self.error_message.clone() {
            draw_dialog(f, area, "Error!".to_string(), msg, Color::Red, 120, 9);
        }

        match self.confirmation_dialog {
            None => (),
            Some(ConfirmationDialogType::NormalSend) => {
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
            Some(ConfirmationDialogType::OneSidedSend) => {
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
            's' | 'o' => {
                if self.to_field.is_empty() {
                    self.error_message = Some("Destination Public Key/Emoji ID\nPress Enter to continue.".to_string());
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

                if matches!(c, 'o') {
                    self.confirmation_dialog = Some(ConfirmationDialogType::OneSidedSend);
                } else {
                    self.confirmation_dialog = Some(ConfirmationDialogType::NormalSend);
                }
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
                self.selected_unique_id = None;
            } else {
                let tokens: Vec<&Token> = app_state
                    .get_owned_tokens()
                    .iter()
                    .filter(|&token| token.output_status() == "Unspent")
                    .collect();
                self.selected_unique_id = Some(Vec::from(tokens[index - 1].unique_id()));
                self.table_state.select(Some(index - 1));
            }
        }
    }

    fn on_down(&mut self, app_state: &mut AppState) {
        if self.show_contacts {
            self.contacts_list_state.set_num_items(app_state.get_contacts().len());
            self.contacts_list_state.next();
        } else if self.send_input_mode == SendInputMode::Amount {
            let index = self.table_state.selected().map(|s| s + 1).unwrap_or_default();
            let tokens: Vec<&Token> = app_state
                .get_owned_tokens()
                .iter()
                .filter(|&token| token.output_status() == "Unspent")
                .collect();
            if index > tokens.len().saturating_sub(1) {
                self.table_state.select(None);
                self.selected_unique_id = None;
            } else {
                self.selected_unique_id = Some(Vec::from(tokens[index].unique_id()));
                self.table_state.select(Some(index));
            }
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
    NormalSend,
    OneSidedSend,
}
