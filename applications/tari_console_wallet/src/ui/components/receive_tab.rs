use crate::ui::{components::Component, state::AppState};
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders, Paragraph},
    Frame,
};

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

        let help_body_area = Layout::default()
            .constraints([Constraint::Min(42)].as_ref())
            .margin(1)
            .split(area);

        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(48), Constraint::Min(1)].as_ref())
            .margin(1)
            .split(help_body_area[0]);

        let qr_code = Paragraph::new(app_state.get_identity().qr_code.as_str()).block(Block::default());

        f.render_widget(qr_code, chunks[0]);

        let info_chunks = Layout::default()
            .constraints(
                [
                    Constraint::Length(1), // Lining up fields with Qr Code
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
        f.render_widget(block, info_chunks[1]);
        let label_layout = Layout::default()
            .constraints([Constraint::Length(1)].as_ref())
            .margin(1)
            .split(info_chunks[1]);
        let public_key = Paragraph::new(app_state.get_identity().public_key.as_str());
        f.render_widget(public_key, label_layout[0]);

        // Public Address
        let block = Block::default()
            .borders(Borders::ALL)
            .title(Span::styled("Public Address", Style::default().fg(Color::White)));
        f.render_widget(block, info_chunks[2]);
        let label_layout = Layout::default()
            .constraints([Constraint::Length(1)].as_ref())
            .margin(1)
            .split(info_chunks[2]);
        let public_address = Paragraph::new(app_state.get_identity().public_address.as_str());
        f.render_widget(public_address, label_layout[0]);

        // Emoji ID
        let block = Block::default()
            .borders(Borders::ALL)
            .title(Span::styled("Emoji ID", Style::default().fg(Color::White)));
        f.render_widget(block, info_chunks[3]);
        let label_layout = Layout::default()
            .constraints([Constraint::Length(1)].as_ref())
            .margin(1)
            .split(info_chunks[3]);
        let emoji_id = Paragraph::new(app_state.get_identity().emoji_id.as_str());
        f.render_widget(emoji_id, label_layout[0]);
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
