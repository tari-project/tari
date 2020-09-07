use crate::ui::{components::Component, state::AppState};
use tui::{
    backend::Backend,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    text::Span,
    widgets::{Block, Borders, Paragraph, Row, Table, Wrap},
    Frame,
};

pub struct NetworkTab {}

impl NetworkTab {
    pub fn new() -> Self {
        Self {}
    }
}

impl<B: Backend> Component<B> for NetworkTab {
    fn draw(&mut self, f: &mut Frame<B>, area: Rect, _app_state: &AppState) {
        // This is all dummy content and layout for review
        let main_chunks = Layout::default()
            .constraints([Constraint::Length(1), Constraint::Length(8), Constraint::Min(10)].as_ref())
            .split(area);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(Span::styled("Base Node Peer", Style::default().fg(Color::White)));
        f.render_widget(block, main_chunks[1]);
        let base_node_layout = Layout::default()
            .constraints([Constraint::Length(3), Constraint::Length(3)].as_ref())
            .margin(1)
            .split(main_chunks[1]);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(Span::styled("Public Key", Style::default().fg(Color::White)));
        f.render_widget(block, base_node_layout[0]);
        let label_layout = Layout::default()
            .constraints([Constraint::Length(1)].as_ref())
            .margin(1)
            .split(base_node_layout[0]);
        let public_key = Paragraph::new("92b34a4dc815531af8aeb8a1f1c8d18b927ddd7feabc706df6a1f87cf5014e54")
            .wrap(Wrap { trim: true });
        f.render_widget(public_key, label_layout[0]);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(Span::styled("Public Address", Style::default().fg(Color::White)));
        f.render_widget(block, base_node_layout[1]);
        let label_layout = Layout::default()
            .constraints([Constraint::Length(1)].as_ref())
            .margin(1)
            .split(base_node_layout[1]);
        let public_address = Paragraph::new("/onion3/mqsfoi62gonulivatrhitugwil3hcxf23eisaieetgyw7x2pdi2bzpyd:18142")
            .wrap(Wrap { trim: true });
        f.render_widget(public_address, label_layout[0]);

        let header = ["Public Key", "User Agent"];
        let rows = vec![
            Row::Data(
                vec![
                    "dc77cae83d06cca0a6912cd93eb04e13345811e94e44d9bf4941495b7a35e644",
                    "tari/basenode/0.2.4",
                ]
                .into_iter(),
            ),
            Row::Data(
                vec![
                    "fe3c7797045d6850c5b3969649f77f93d7dc46e77e293dfa90f1ac36ba8d8501",
                    "tari/basenode/0.3.1",
                ]
                .into_iter(),
            ),
            Row::Data(
                vec![
                    "fe3c7797045d6850c5b3969649f77f93d7dc46e77e293dfa90f1ac36ba8d8501",
                    "tari/basenode/0.3.1",
                ]
                .into_iter(),
            ),
            Row::Data(
                vec![
                    "d440b328e69b20dd8ee6c4a61aeb18888939f0f67cf96668840b7f72055d834c",
                    "tari/wallet/0.2.3",
                ]
                .into_iter(),
            ),
        ];

        let table = Table::new(header.iter(), rows.into_iter())
            .block(Block::default().title("Connected Peers").borders(Borders::ALL))
            .header_style(Style::default().fg(Color::Magenta))
            .widths(&[Constraint::Length(65), Constraint::Length(65), Constraint::Min(1)]);
        f.render_widget(table, main_chunks[2]);
    }
}
