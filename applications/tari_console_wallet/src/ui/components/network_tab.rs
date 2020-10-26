use crate::ui::{components::Component, state::AppState, widgets::MultiColumnList, MAX_WIDTH};
use tui::{
    backend::Backend,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

pub struct NetworkTab {}

impl NetworkTab {
    pub fn new() -> Self {
        Self {}
    }

    pub fn draw_connected_peers_list<B>(&self, f: &mut Frame<B>, area: Rect, app_state: &AppState)
    where B: Backend {
        let block = Block::default().borders(Borders::ALL).title(Span::styled(
            "Connected Peers",
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ));
        f.render_widget(block, area);

        let list_areas = Layout::default()
            .constraints([Constraint::Min(1)].as_ref())
            .margin(1)
            .split(area);

        let peers = app_state.get_connected_peers();
        let mut column0_items = Vec::with_capacity(peers.len());
        let mut column1_items = Vec::with_capacity(peers.len());
        let mut column2_items = Vec::with_capacity(peers.len());
        for p in peers.iter() {
            column0_items.push(ListItem::new(Span::raw(p.node_id.to_string())));
            column1_items.push(ListItem::new(Span::raw(p.public_key.to_string())));
            column2_items.push(ListItem::new(Span::raw(p.user_agent.clone())));
        }
        let column_list = MultiColumnList::new()
            .heading_style(Style::default().fg(Color::Magenta))
            .max_width(MAX_WIDTH)
            .add_column(Some("NodeID"), Some(27), column0_items)
            .add_column(Some("Public Key"), Some(65), column1_items)
            .add_column(Some("User Agent"), Some(MAX_WIDTH.saturating_sub(93)), column2_items);
        column_list.render(f, list_areas[0], &mut ListState::default());
    }
}

impl<B: Backend> Component<B> for NetworkTab {
    fn draw(&mut self, f: &mut Frame<B>, area: Rect, app_state: &AppState) {
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

        self.draw_connected_peers_list(f, main_chunks[2], app_state);
    }
}
