use crate::{
    ui::{
        components::{balance::Balance, Component, KeyHandled},
        state::{AppState, UiTransactionSendStatus},
        widgets::{centered_rect_absolute, draw_dialog, MultiColumnList, WindowedListState},
        MAX_WIDTH,
    },
    utils::formatting::display_compressed_string,
};
use tari_wallet::types::DEFAULT_FEE_PER_GRAM;
use tokio::{runtime::Handle, sync::watch};
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, Clear, ListItem, Paragraph, Wrap},
    Frame,
};
use unicode_width::UnicodeWidthStr;

pub struct SendTab {
    balance: Balance,
    send_input_mode: SendInputMode,
    edit_contact_mode: ContactInputMode,
    show_contacts: bool,
    show_edit_contact: bool,
    to_field: String,
    amount_field: String,
    fee_field: String,
    message_field: String,
    alias_field: String,
    public_key_field: String,
    error_message: Option<String>,
    success_message: Option<String>,
    contacts_list_state: WindowedListState,
    send_result_watch: Option<watch::Receiver<UiTransactionSendStatus>>,
    confirmation_dialog: Option<ConfirmationDialogType>,
}

impl SendTab {
    pub fn new() -> Self {
        Self {
            balance: Balance::new(),
            send_input_mode: SendInputMode::None,
            edit_contact_mode: ContactInputMode::None,
            show_contacts: false,
            show_edit_contact: false,
            to_field: "".to_string(),
            amount_field: "".to_string(),
            fee_field: u64::from(DEFAULT_FEE_PER_GRAM).to_string(),
            message_field: "".to_string(),
            alias_field: "".to_string(),
            public_key_field: "".to_string(),
            error_message: None,
            success_message: None,
            contacts_list_state: WindowedListState::new(),
            send_result_watch: None,
            confirmation_dialog: None,
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
                Span::styled("Amount", Style::default().add_modifier(Modifier::BOLD)),
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
                Span::raw(" to send normal a transaction, "),
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

        let amount_input = Paragraph::new(self.amount_field.as_ref())
            .style(match self.send_input_mode {
                SendInputMode::Amount => Style::default().fg(Color::Magenta),
                _ => Style::default(),
            })
            .block(Block::default().borders(Borders::ALL).title("(A)mount (uT):"));
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
            SendInputMode::Amount => f.set_cursor(
                // Put cursor past the end of the input text
                amount_fee_layout[0].x + self.amount_field.width() as u16 + 1,
                // Move one line down, from the border to the input line
                amount_fee_layout[0].y + 1,
            ),
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
            Span::styled("Up/Down Arrow Keys", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" to choose a contact, "),
            Span::styled("E", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" to (e)dit and "),
            Span::styled("D", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" to (d)elete a contact, "),
            Span::styled("N", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" to create a (n)ew contact, "),
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
            .add_column(Some("Alias"), Some(12), column0_items)
            .add_column(Some("Public Key"), Some(67), column1_items)
            .add_column(Some("Emoji ID"), None, column2_items);
        column_list.render(f, list_areas[1], &mut list_state);
    }

    fn draw_edit_contact<B>(&mut self, f: &mut Frame<B>, area: Rect, _app_state: &AppState)
    where B: Backend {
        let popup_area = centered_rect_absolute(120, 10, area);

        f.render_widget(Clear, popup_area);

        let block = Block::default().borders(Borders::ALL).title(Span::styled(
            "Add/Edit Contact",
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ));
        f.render_widget(block, popup_area);
        let vert_chunks = Layout::default()
            .constraints([Constraint::Length(2), Constraint::Length(3), Constraint::Length(3)].as_ref())
            .margin(1)
            .split(popup_area);

        let instructions = Paragraph::new(Spans::from(vec![
            Span::raw("Press "),
            Span::styled("L", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" to edit "),
            Span::styled("Alias", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" field, "),
            Span::styled("K", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" to edit "),
            Span::styled("Public Key/Emoji ID", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" field, "),
            Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" to save Contact."),
        ]))
        .block(Block::default());
        f.render_widget(instructions, vert_chunks[0]);

        let alias_input = Paragraph::new(self.alias_field.as_ref())
            .style(match self.edit_contact_mode {
                ContactInputMode::Alias => Style::default().fg(Color::Magenta),
                _ => Style::default(),
            })
            .block(Block::default().borders(Borders::ALL).title("A(l)ias:"));
        f.render_widget(alias_input, vert_chunks[1]);

        let pubkey_input = Paragraph::new(self.public_key_field.as_ref())
            .style(match self.edit_contact_mode {
                ContactInputMode::PubkeyEmojiId => Style::default().fg(Color::Magenta),
                _ => Style::default(),
            })
            .block(Block::default().borders(Borders::ALL).title("Public (K)ey / Emoji Id:"));
        f.render_widget(pubkey_input, vert_chunks[2]);

        match self.edit_contact_mode {
            ContactInputMode::None => (),
            ContactInputMode::Alias => f.set_cursor(
                // Put cursor past the end of the input text
                vert_chunks[1].x + self.alias_field.width() as u16 + 1,
                // Move one line down, from the border to the input line
                vert_chunks[1].y + 1,
            ),
            ContactInputMode::PubkeyEmojiId => f.set_cursor(
                // Put cursor past the end of the input text
                vert_chunks[2].x + self.public_key_field.width() as u16 + 1,
                // Move one line down, from the border to the input line
                vert_chunks[2].y + 1,
            ),
        }
    }

    fn on_key_confirmation_dialog(&mut self, c: char, app_state: &mut AppState) -> KeyHandled {
        if self.confirmation_dialog.is_some() {
            if 'n' == c {
                self.confirmation_dialog = None;
                return KeyHandled::Handled;
            } else if 'y' == c {
                let one_sided_transaction = matches!(
                    self.confirmation_dialog,
                    Some(ConfirmationDialogType::ConfirmOneSidedSend)
                );
                match self.confirmation_dialog {
                    None => (),
                    Some(ConfirmationDialogType::ConfirmNormalSend) |
                    Some(ConfirmationDialogType::ConfirmOneSidedSend) => {
                        if 'y' == c {
                            let amount = if let Ok(v) = self.amount_field.parse::<u64>() {
                                v
                            } else {
                                self.error_message =
                                    Some("Amount should be an integer\nPress Enter to continue.".to_string());
                                return KeyHandled::Handled;
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
                                    amount,
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
                                    amount,
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
                                self.fee_field = u64::from(DEFAULT_FEE_PER_GRAM).to_string();
                                self.message_field = "".to_string();
                                self.send_input_mode = SendInputMode::None;
                                self.send_result_watch = Some(rx);
                            }
                            self.confirmation_dialog = None;
                            return KeyHandled::Handled;
                        }
                    },
                    Some(ConfirmationDialogType::ConfirmDeleteContact) => {
                        if 'y' == c {
                            if let Some(c) = self
                                .contacts_list_state
                                .selected()
                                .and_then(|i| app_state.get_contact(i))
                                .cloned()
                            {
                                if let Err(_e) = Handle::current().block_on(app_state.delete_contact(c.public_key)) {
                                    self.error_message =
                                        Some("Could not delete selected contact\nPress Enter to continue.".to_string());
                                }
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
                    '\n' | '\t' => {
                        self.send_input_mode = SendInputMode::Amount;
                    },
                    c => {
                        self.to_field.push(c);
                        return KeyHandled::Handled;
                    },
                },
                SendInputMode::Amount => match c {
                    '\n' => self.send_input_mode = SendInputMode::Message,
                    c => {
                        if c.is_numeric() {
                            self.amount_field.push(c);
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

    fn on_key_edit_contact(&mut self, c: char, app_state: &mut AppState) -> KeyHandled {
        if self.show_edit_contact && self.edit_contact_mode != ContactInputMode::None {
            match self.edit_contact_mode {
                ContactInputMode::None => return KeyHandled::Handled,
                ContactInputMode::Alias => match c {
                    '\n' | '\t' => {
                        self.edit_contact_mode = ContactInputMode::PubkeyEmojiId;
                        return KeyHandled::Handled;
                    },
                    c => {
                        self.alias_field.push(c);
                        return KeyHandled::Handled;
                    },
                },
                ContactInputMode::PubkeyEmojiId => match c {
                    '\n' => {
                        self.edit_contact_mode = ContactInputMode::None;
                        self.show_edit_contact = false;

                        if let Err(_e) = Handle::current()
                            .block_on(app_state.upsert_contact(self.alias_field.clone(), self.public_key_field.clone()))
                        {
                            self.error_message =
                                Some("Invalid Public key or Emoji ID provided\n Press Enter to continue.".to_string());
                        }

                        self.alias_field = "".to_string();
                        self.public_key_field = "".to_string();
                        return KeyHandled::Handled;
                    },
                    c => {
                        self.public_key_field.push(c);
                        return KeyHandled::Handled;
                    },
                },
            }
        }

        KeyHandled::NotHandled
    }

    fn on_key_show_contacts(&mut self, c: char, app_state: &mut AppState) -> KeyHandled {
        if self.show_contacts {
            match c {
                'd' => {
                    self.confirmation_dialog = Some(ConfirmationDialogType::ConfirmDeleteContact);
                    return KeyHandled::Handled;
                },
                '\n' => {
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
                },
                'n' => {
                    self.show_edit_contact = true;
                    self.edit_contact_mode = ContactInputMode::Alias;
                    return KeyHandled::Handled;
                },
                _ => (),
            }
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
                    Constraint::Length(13),
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
            if self.show_edit_contact {
                self.draw_edit_contact(f, area, app_state);
            }
        };

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
            Some(ConfirmationDialogType::ConfirmNormalSend) => {
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
            Some(ConfirmationDialogType::ConfirmOneSidedSend) => {
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
            Some(ConfirmationDialogType::ConfirmDeleteContact) => {
                draw_dialog(
                    f,
                    area,
                    "Confirm Delete".to_string(),
                    "Are you sure you want to delete this contact?\n(Y)es / (N)o".to_string(),
                    Color::Red,
                    120,
                    9,
                );
            },
        }
    }

    fn on_key(&mut self, app_state: &mut AppState, c: char) {
        if self.error_message.is_some() && '\n' == c {
            self.error_message = None;
            return;
        }

        if self.success_message.is_some() && '\n' == c {
            self.success_message = None;
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

        if self.on_key_edit_contact(c, app_state) == KeyHandled::Handled {
            return;
        }

        if self.on_key_show_contacts(c, app_state) == KeyHandled::Handled {
            return;
        }

        match c {
            'c' => {
                self.show_contacts = !self.show_contacts;
                if self.show_contacts {
                    self.show_edit_contact = false;
                    self.edit_contact_mode = ContactInputMode::Alias;
                    self.public_key_field = "".to_string();
                    self.amount_field = "".to_string();
                    self.message_field = "".to_string();
                    self.send_input_mode = SendInputMode::None;
                }
            },

            'e' => {
                if let Some(c) = self
                    .contacts_list_state
                    .selected()
                    .and_then(|i| app_state.get_contact(i))
                {
                    self.public_key_field = c.public_key.clone();
                    self.alias_field = c.alias.clone();
                    if self.show_contacts {
                        self.show_edit_contact = true;
                        self.edit_contact_mode = ContactInputMode::Alias;
                    }
                }
            },
            't' => self.send_input_mode = SendInputMode::To,
            'a' => self.send_input_mode = SendInputMode::Amount,
            'f' => self.send_input_mode = SendInputMode::Fee,
            'm' => self.send_input_mode = SendInputMode::Message,
            's' | 'o' => {
                if self.amount_field.is_empty() || self.to_field.is_empty() {
                    self.error_message = Some(
                        "Destination Public Key/Emoji ID and Amount required\nPress Enter to continue.".to_string(),
                    );
                    return;
                }
                if self.amount_field.parse::<u64>().is_err() {
                    self.error_message = Some("Amount should be an integer\nPress Enter to continue.".to_string());
                    return;
                };

                if matches!(c, 'o') {
                    self.confirmation_dialog = Some(ConfirmationDialogType::ConfirmOneSidedSend);
                } else {
                    self.confirmation_dialog = Some(ConfirmationDialogType::ConfirmNormalSend);
                }
            },
            _ => {},
        }
    }

    fn on_up(&mut self, app_state: &mut AppState) {
        self.contacts_list_state.set_num_items(app_state.get_contacts().len());
        self.contacts_list_state.previous();
    }

    fn on_down(&mut self, app_state: &mut AppState) {
        self.contacts_list_state.set_num_items(app_state.get_contacts().len());
        self.contacts_list_state.next();
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
                let _ = self.amount_field.pop();
            },
            SendInputMode::Fee => {
                let _ = self.fee_field.pop();
            },
            SendInputMode::Message => {
                let _ = self.message_field.pop();
            },
            SendInputMode::None => {},
        }

        match self.edit_contact_mode {
            ContactInputMode::Alias => {
                let _ = self.alias_field.pop();
            },
            ContactInputMode::PubkeyEmojiId => {
                let _ = self.public_key_field.pop();
            },
            ContactInputMode::None => {},
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
pub enum ContactInputMode {
    None,
    Alias,
    PubkeyEmojiId,
}

#[derive(PartialEq, Debug)]
pub enum ConfirmationDialogType {
    ConfirmNormalSend,
    ConfirmOneSidedSend,
    ConfirmDeleteContact,
}
