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
            .constraints([Constraint::Length(6), Constraint::Length(23)].as_ref())
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
                ]
                .as_ref(),
            )
            .margin(1)
            .split(chunks[0]);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(Span::styled("Connection Details", Style::default().fg(Color::White)));
        f.render_widget(block, chunks[0]);

        const ITEM_01: &str = "Public Key:     ";
        const ITEM_02: &str = "Node ID:        ";
        const ITEM_03: &str = "Public Address: ";
        const ITEM_04: &str = "Emoji ID:       ";

        // Public Key
        let public_key_text = Spans::from(vec![
            Span::styled(ITEM_01, Style::default().fg(Color::Magenta)),
            Span::styled(
                app_state.get_identity().public_key.clone(),
                Style::default().fg(Color::White),
            ),
        ]);
        let paragraph = Paragraph::new(public_key_text).block(Block::default());
        f.render_widget(paragraph, details_chunks[0]);

        // NodeId
        let node_id_text = Spans::from(vec![
            Span::styled(ITEM_02, Style::default().fg(Color::Magenta)),
            Span::styled(
                app_state.get_identity().node_id.clone(),
                Style::default().fg(Color::White),
            ),
        ]);
        let paragraph = Paragraph::new(node_id_text).block(Block::default());
        f.render_widget(paragraph, details_chunks[1]);

        // Public Address
        let public_ddress_text = Spans::from(vec![
            Span::styled(ITEM_03, Style::default().fg(Color::Magenta)),
            Span::styled(
                app_state.get_identity().public_address.clone(),
                Style::default().fg(Color::White),
            ),
        ]);
        let paragraph = Paragraph::new(public_ddress_text).block(Block::default());
        f.render_widget(paragraph, details_chunks[2]);

        // Emoji ID
        let emoji_id_text = Spans::from(vec![
            Span::styled(ITEM_04, Style::default().fg(Color::Magenta)),
            Span::styled(
                app_state.get_identity().emoji_id.clone(),
                Style::default().fg(Color::White),
            ),
        ]);
        let paragraph = Paragraph::new(emoji_id_text).block(Block::default());
        f.render_widget(paragraph, details_chunks[3]);
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
