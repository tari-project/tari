use tokio::runtime::Handle;
use tui::{
    backend::Backend,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, Clear, ListItem, Paragraph, Wrap},
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::{
    ui::{
        components::{Component, KeyHandled},
        state::AppState,
        widgets::{centered_rect_absolute, draw_dialog, MultiColumnList, WindowedListState},
        MAX_WIDTH,
    },
    utils::formatting::display_compressed_string,
};

pub struct ContactsTab {
    edit_contact_mode: ContactInputMode,
    show_edit_contact: bool,
    alias_field: String,
    public_key_field: String,
    error_message: Option<String>,
    contacts_list_state: WindowedListState,
    confirmation_dialog: Option<ConfirmationDialogType>,
}

impl ContactsTab {
    pub fn new() -> Self {
        Self {
            edit_contact_mode: ContactInputMode::None,
            show_edit_contact: false,
            alias_field: String::new(),
            public_key_field: String::new(),
            error_message: None,
            contacts_list_state: WindowedListState::new(),
            confirmation_dialog: None,
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
            Span::raw("Use "),
            Span::styled("Up↑/Down↓ Keys", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" to select a contact, "),
            Span::styled("E", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" or "),
            Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" to (e)dit a contact, "),
            Span::styled("D", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" to (d)elete a contact and "),
            Span::styled("N", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" to create a (n)ew contact."),
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
                match self.confirmation_dialog {
                    None => (),
                    Some(ConfirmationDialogType::DeleteContact) => {
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

    fn on_key_show_contacts(&mut self, c: char, _app_state: &mut AppState) -> KeyHandled {
        match c {
            'd' => {
                if self.contacts_list_state.selected().is_none() {
                    return KeyHandled::NotHandled;
                }
                self.confirmation_dialog = Some(ConfirmationDialogType::DeleteContact);
                return KeyHandled::Handled;
            },
            'n' => {
                self.show_edit_contact = true;
                self.edit_contact_mode = ContactInputMode::Alias;
                return KeyHandled::Handled;
            },
            _ => (),
        }

        KeyHandled::NotHandled
    }
}

impl<B: Backend> Component<B> for ContactsTab {
    fn draw(&mut self, f: &mut Frame<B>, area: Rect, app_state: &AppState) {
        self.draw_contacts(f, area, app_state);
        if self.show_edit_contact {
            self.draw_edit_contact(f, area, app_state);
        }

        match self.confirmation_dialog {
            None => (),
            Some(ConfirmationDialogType::DeleteContact) => {
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
        if self.error_message.is_some() {
            if '\n' == c {
                self.error_message = None;
            }
            return;
        }

        if self.on_key_confirmation_dialog(c, app_state) == KeyHandled::Handled {
            return;
        }

        if self.on_key_edit_contact(c, app_state) == KeyHandled::Handled {
            return;
        }

        if self.on_key_show_contacts(c, app_state) == KeyHandled::Handled {
            return;
        }

        match c {
            'e' | '\n' => {
                if let Some(c) = self
                    .contacts_list_state
                    .selected()
                    .and_then(|i| app_state.get_contact(i))
                {
                    self.public_key_field = c.public_key.clone();
                    self.alias_field = c.alias.clone();
                    self.show_edit_contact = true;
                    self.edit_contact_mode = ContactInputMode::Alias;
                }
            },
            _ => {
                self.show_edit_contact = false;
                self.edit_contact_mode = ContactInputMode::Alias;
                self.public_key_field = "".to_string();
            },
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
        if self.confirmation_dialog.is_some() {
            return;
        }
        self.edit_contact_mode = ContactInputMode::None;
        if self.show_edit_contact {
            self.show_edit_contact = false;
        } else {
            self.contacts_list_state.select(None);
        }
    }

    fn on_backspace(&mut self, _app_state: &mut AppState) {
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
pub enum ContactInputMode {
    None,
    Alias,
    PubkeyEmojiId,
}

#[derive(PartialEq, Debug)]
pub enum ConfirmationDialogType {
    DeleteContact,
}
