// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::ui::{components::Component, state::AppState};

pub struct ReceiveTab {}

impl ReceiveTab {
    pub fn new() -> Self {
        Self {}
    }

    fn draw_whoami<B>(&self, f: &mut Frame<B>, area: Rect, app_state: &AppState)
    where B: Backend {
        let block = Block::default().borders(Borders::ALL).title(Span::styled(
            "Who Am I?",
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ));
        f.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(8), Constraint::Length(23)].as_ref())
            .margin(1)
            .split(area);

        // QR Code
        let qr_code = Paragraph::new(app_state.get_identity().qr_code.as_str()).block(Block::default());
        f.render_widget(qr_code, chunks[1]);

        // Connection details
        let details_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Length(1),
                ]
                .as_ref(),
            )
            .margin(1)
            .split(chunks[0]);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(Span::styled("Connection Details", Style::default().fg(Color::White)));
        f.render_widget(block, chunks[0]);

        const ITEM_01: &str = "Tari Address interactive:   ";
        const ITEM_02: &str = "Tari Address one-sided:     ";
        const ITEM_03: &str = "Node ID / Public key:       ";
        const ITEM_04: &str = "Network Address:            ";
        const ITEM_05: &str = "Interactive emoji address:  ";
        const ITEM_06: &str = "One-sided emoji address:    ";

        // Tari address
        let tari_address_interactive_text = Spans::from(vec![
            Span::styled(ITEM_01, Style::default().fg(Color::Magenta)),
            Span::styled(
                app_state.get_identity().tari_address_interactive.to_base58(),
                Style::default().fg(Color::White),
            ),
        ]);
        let paragraph = Paragraph::new(tari_address_interactive_text).block(Block::default());
        f.render_widget(paragraph, details_chunks[0]);

        let tari_address_one_sided_text = Spans::from(vec![
            Span::styled(ITEM_02, Style::default().fg(Color::Magenta)),
            Span::styled(
                app_state.get_identity().tari_address_one_sided.to_base58(),
                Style::default().fg(Color::White),
            ),
        ]);
        let paragraph = Paragraph::new(tari_address_one_sided_text).block(Block::default());
        f.render_widget(paragraph, details_chunks[1]);

        // NodeId
        let node_id_text = Spans::from(vec![
            Span::styled(ITEM_03, Style::default().fg(Color::Magenta)),
            Span::styled(
                app_state.get_identity().node_id.clone(),
                Style::default().fg(Color::White),
            ),
            Span::styled(" / ", Style::default().fg(Color::White)),
            Span::styled(
                app_state.get_identity().public_key.clone(),
                Style::default().fg(Color::White),
            ),
        ]);
        let paragraph = Paragraph::new(node_id_text).block(Block::default());
        f.render_widget(paragraph, details_chunks[2]);

        // Public Address
        let public_address_text = Spans::from(vec![
            Span::styled(ITEM_04, Style::default().fg(Color::Magenta)),
            Span::styled(
                app_state.get_identity().network_address.clone(),
                Style::default().fg(Color::White),
            ),
        ]);
        let paragraph = Paragraph::new(public_address_text).block(Block::default());
        f.render_widget(paragraph, details_chunks[3]);

        // Emoji ID
        let emoji_id_text = Spans::from(vec![
            Span::styled(ITEM_05, Style::default().fg(Color::Magenta)),
            Span::styled(
                app_state.get_identity().tari_address_interactive.to_emoji_string(),
                Style::default().fg(Color::White),
            ),
        ]);
        let paragraph = Paragraph::new(emoji_id_text).block(Block::default());
        f.render_widget(paragraph, details_chunks[4]);

        let emoji_id_text = Spans::from(vec![
            Span::styled(ITEM_06, Style::default().fg(Color::Magenta)),
            Span::styled(
                app_state.get_identity().tari_address_one_sided.to_emoji_string(),
                Style::default().fg(Color::White),
            ),
        ]);
        let paragraph = Paragraph::new(emoji_id_text).block(Block::default());
        f.render_widget(paragraph, details_chunks[5]);
    }
}

impl<B: Backend> Component<B> for ReceiveTab {
    fn draw(&mut self, f: &mut Frame<B>, area: Rect, app_state: &AppState) {
        let areas = Layout::default()
            .constraints([Constraint::Min(42)].as_ref())
            .split(area);

        self.draw_whoami(f, areas[0], app_state);
    }

    fn on_key(&mut self, _app_state: &mut AppState, _c: char) {}

    fn on_up(&mut self, _app_state: &mut AppState) {}

    fn on_down(&mut self, _app_state: &mut AppState) {}

    fn on_esc(&mut self, _: &mut AppState) {}

    fn on_backspace(&mut self, _app_state: &mut AppState) {}
}
