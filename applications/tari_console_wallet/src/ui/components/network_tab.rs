use crate::ui::{components::Component, state::AppState, widgets::MultiColumnList, MAX_WIDTH};
use tari_core::tari_utilities::hex::Hex;
use tui::{
    backend::Backend,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
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

    pub fn draw_base_node_peer<B>(&self, f: &mut Frame<B>, area: Rect, app_state: &AppState)
    where B: Backend {
        let block = Block::default().borders(Borders::ALL).title(Span::styled(
            "Base Node Peer",
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ));
        f.render_widget(block, area);

        let base_node = app_state.get_base_node_peer();
        let public_key = base_node.public_key.to_hex();
        let public_address = match base_node.addresses.first() {
            Some(address) => address.to_string(),
            None => "".to_string(),
        };

        let base_node_layout = Layout::default()
            .constraints([Constraint::Length(1), Constraint::Length(1)].as_ref())
            .margin(1)
            .split(area);

        let public_key_paragraph = Paragraph::new(Spans::from(vec![
            Span::styled("Public Key:", Style::default().fg(Color::Magenta)),
            Span::raw(" "),
            Span::styled(public_key, Style::default().fg(Color::White)),
        ]))
        .wrap(Wrap { trim: true });
        f.render_widget(public_key_paragraph, base_node_layout[0]);

        let public_address_paragraph = Paragraph::new(Spans::from(vec![
            Span::styled("Public Address:", Style::default().fg(Color::Magenta)),
            Span::raw(" "),
            Span::styled(public_address, Style::default().fg(Color::White)),
        ]))
        .wrap(Wrap { trim: true });
        f.render_widget(public_address_paragraph, base_node_layout[1]);
    }
}

impl<B: Backend> Component<B> for NetworkTab {
    fn draw(&mut self, f: &mut Frame<B>, area: Rect, app_state: &AppState) {
        let main_chunks = Layout::default()
            .constraints([Constraint::Length(1), Constraint::Length(4), Constraint::Min(10)].as_ref())
            .split(area);

        self.draw_base_node_peer(f, main_chunks[1], app_state);
        self.draw_connected_peers_list(f, main_chunks[2], app_state);
    }
}
