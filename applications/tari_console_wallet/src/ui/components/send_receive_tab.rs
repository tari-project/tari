use crate::{
    ui::{
        components::{balance::Balance, Component},
        multi_column_list::MultiColumnList,
        state::AppState,
        SendInputMode,
        MAX_WIDTH,
    },
    utils::formatting::display_compressed_string,
};
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, ListItem, ListState, Paragraph, Wrap},
    Frame,
};
use unicode_width::UnicodeWidthStr;

pub struct SendReceiveTab {
    balance: Balance,
    send_input_mode: SendInputMode,
    show_contacts: bool,
    contacts_state: ListState,
}

impl SendReceiveTab {
    pub fn new() -> Self {
        Self {
            balance: Balance::new(),
            send_input_mode: SendInputMode::None,
            show_contacts: false,
            contacts_state: Default::default(),
        }
    }

    fn draw_send_form<B>(&self, f: &mut Frame<B>, area: Rect, app_state: &AppState)
    where B: Backend {
        let block = Block::default().borders(Borders::ALL).title(Span::styled(
            "Send Transaction",
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ));
        f.render_widget(block, area);
        let vert_chunks = Layout::default()
            .constraints([Constraint::Length(2), Constraint::Length(3), Constraint::Length(3)].as_ref())
            .margin(1)
            .split(area);
        let instructions = Paragraph::new(Spans::from(vec![
            Span::raw("Press "),
            Span::styled("T", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" to edit "),
            Span::styled("To", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" field, "),
            Span::styled("A", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" to edit "),
            Span::styled("Amount", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" field, "),
            Span::styled("C", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" to select a contact, "),
            Span::styled("S", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" to send transaction."),
        ]))
        .block(Block::default());
        f.render_widget(instructions, vert_chunks[0]);

        let to_input = Paragraph::new(app_state.to_field.as_ref())
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

        let amount_input = Paragraph::new(app_state.amount_field.as_ref())
            .style(match self.send_input_mode {
                SendInputMode::Amount => Style::default().fg(Color::Magenta),
                _ => Style::default(),
            })
            .block(Block::default().borders(Borders::ALL).title("(A)mount (uT):"));
        f.render_widget(amount_input, vert_chunks[2]);

        match self.send_input_mode {
            SendInputMode::None => (),
            SendInputMode::To => f.set_cursor(
                // Put cursor past the end of the input text
                vert_chunks[1].x + app_state.to_field.width() as u16 + 1,
                // Move one line down, from the border to the input line
                vert_chunks[1].y + 1,
            ),
            SendInputMode::Amount => f.set_cursor(
                // Put cursor past the end of the input text
                vert_chunks[2].x + app_state.amount_field.width() as u16 + 1,
                // Move one line down, from the border to the input line
                vert_chunks[2].y + 1,
            ),
        }
    }

    fn draw_whoami<B>(&self, f: &mut Frame<B>, area: Rect, app_state: &AppState)
    where B: Backend {
        let block = Block::default().borders(Borders::ALL).title(Span::styled(
            "Who Am I?",
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ));
        f.render_widget(block, area);

        let help_body_area = Layout::default()
            .constraints([Constraint::Min(42)].as_ref())
            .margin(1)
            .split(area);

        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(46), Constraint::Min(1)].as_ref())
            .margin(1)
            .split(help_body_area[0]);

        let qr_code = Paragraph::new(app_state.my_identity.qr_code.as_str())
            .block(Block::default())
            .wrap(Wrap { trim: true });
        f.render_widget(qr_code, chunks[0]);

        let info_chunks = Layout::default()
            .constraints(
                [
                    Constraint::Length(3),
                    Constraint::Length(3),
                    Constraint::Length(3),
                    Constraint::Min(1),
                ]
                .as_ref(),
            )
            .horizontal_margin(1)
            .split(chunks[1]);

        // Public Key
        let block = Block::default()
            .borders(Borders::ALL)
            .title(Span::styled("Public Key", Style::default().fg(Color::White)));
        f.render_widget(block, info_chunks[0]);
        let label_layout = Layout::default()
            .constraints([Constraint::Length(1)].as_ref())
            .margin(1)
            .split(info_chunks[0]);
        let public_key = Paragraph::new(app_state.my_identity.public_key.as_str());
        f.render_widget(public_key, label_layout[0]);

        // Public Address
        let block = Block::default()
            .borders(Borders::ALL)
            .title(Span::styled("Public Address", Style::default().fg(Color::White)));
        f.render_widget(block, info_chunks[1]);
        let label_layout = Layout::default()
            .constraints([Constraint::Length(1)].as_ref())
            .margin(1)
            .split(info_chunks[1]);
        let public_address = Paragraph::new(app_state.my_identity.public_address.as_str());
        f.render_widget(public_address, label_layout[0]);

        // Emoji ID
        let block = Block::default()
            .borders(Borders::ALL)
            .title(Span::styled("Emoji ID", Style::default().fg(Color::White)));
        f.render_widget(block, info_chunks[2]);
        let label_layout = Layout::default()
            .constraints([Constraint::Length(1)].as_ref())
            .margin(1)
            .split(info_chunks[2]);
        let emoji_id = Paragraph::new(app_state.my_identity.emoji_id.as_str());
        f.render_widget(emoji_id, label_layout[0]);
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
            Span::raw(" to select a contact, "),
            Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" to select that contact as a recipient."),
        ]))
        .wrap(Wrap { trim: true });
        f.render_widget(instructions, list_areas[0]);

        let mut column0_items = Vec::new();
        let mut column1_items = Vec::new();
        let mut column2_items = Vec::new();
        for c in app_state.contacts.items.iter() {
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
        column_list.render(f, list_areas[1], &mut self.contacts_state);
    }
}

impl<B: Backend> Component<B> for SendReceiveTab {
    fn draw(&mut self, f: &mut Frame<B>, area: Rect, app_state: &AppState) {
        let balance_main_area = Layout::default()
            .constraints(
                [
                    Constraint::Length(3),
                    Constraint::Min(10),
                    Constraint::Min(42),
                    Constraint::Min(1),
                ]
                .as_ref(),
            )
            .split(area);

        self.balance.draw(f, balance_main_area[0], app_state);
        self.draw_send_form(f, balance_main_area[1], app_state);

        if self.show_contacts {
            self.draw_contacts(f, balance_main_area[2], app_state);
        } else {
            self.draw_whoami(f, balance_main_area[2], app_state);
        }
    }

    fn on_key(&mut self, app_state: &mut AppState, c: char) {
        match self.send_input_mode {
            SendInputMode::None => match c {
                'c' => self.show_contacts = !self.show_contacts,
                't' => self.send_input_mode = SendInputMode::To,
                'a' => self.send_input_mode = SendInputMode::Amount,
                '\n' => {
                    if self.show_contacts {
                        if let Some(c) = app_state.contacts.selected_item() {
                            app_state.to_field = c.public_key.clone();
                            self.show_contacts = false;
                        }
                    }
                },
                _ => {},
            },
            SendInputMode::To => match c {
                '\n' | '\t' => {
                    self.send_input_mode = SendInputMode::None;
                    self.send_input_mode = SendInputMode::Amount;
                },
                c => {
                    app_state.to_field.push(c);
                },
            },
            SendInputMode::Amount => match c {
                '\n' | '\t' => self.send_input_mode = SendInputMode::None,
                c => {
                    if c.is_numeric() {
                        app_state.amount_field.push(c);
                    }
                },
            },
        }
    }

    fn on_up(&mut self, app_state: &mut AppState) {
        app_state.contacts.previous();
    }

    fn on_down(&mut self, app_state: &mut AppState) {
        app_state.contacts.next();
    }

    fn on_esc(&mut self, _: &mut AppState) {
        self.send_input_mode = SendInputMode::None;
        self.show_contacts = false;
    }

    fn on_backspace(&mut self, app_state: &mut AppState) {
        match self.send_input_mode {
            SendInputMode::To => {
                let _ = app_state.to_field.pop();
            },
            SendInputMode::Amount => {
                let _ = app_state.amount_field.pop();
            },
            SendInputMode::None => {},
        }
    }
}
