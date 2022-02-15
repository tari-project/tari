use std::collections::HashMap;

use log::*;
use tari_comms::peer_manager::Peer;
use tari_crypto::tari_utilities::hex::Hex;
use tokio::runtime::Handle;
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

use crate::{
    ui::{
        components::{balance::Balance, Component, KeyHandled},
        state::AppState,
        widgets::{draw_dialog, MultiColumnList, WindowedListState},
        MAX_WIDTH,
    },
    utils::formatting::display_address,
};

const LOG_TARGET: &str = "wallet::console_wallet::network_tab";

pub struct NetworkTab {
    balance: Balance,
    base_node_edit_mode: BaseNodeInputMode,
    public_key_field: String,
    previous_public_key_field: String,
    address_field: String,
    previous_address_field: String,
    error_message: Option<String>,
    confirmation_dialog: bool,
    base_node_list_state: WindowedListState,
    detailed_base_node: Option<Peer>,
}

impl NetworkTab {
    pub fn new(base_node_selected: Peer) -> Self {
        let public_key = base_node_selected.public_key.to_hex();
        let address = display_address(&base_node_selected);

        Self {
            balance: Balance::new(),
            base_node_edit_mode: BaseNodeInputMode::None,
            public_key_field: public_key.clone(),
            previous_public_key_field: public_key,
            address_field: address.clone(),
            previous_address_field: address,
            error_message: None,
            confirmation_dialog: false,
            base_node_list_state: WindowedListState::new(),
            detailed_base_node: Some(base_node_selected),
        }
    }

    pub fn draw_base_node_selection<B>(&mut self, f: &mut Frame<B>, area: Rect, app_state: &AppState)
    where B: Backend {
        let block = Block::default().borders(Borders::ALL).title(Span::styled(
            "Base Node Selection",
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ));
        f.render_widget(block, area);

        let areas = Layout::default()
            .constraints([Constraint::Length(2), Constraint::Min(8)].as_ref())
            .margin(1)
            .split(area);

        let instructions = Paragraph::new(Spans::from(vec![
            Span::raw("Press "),
            Span::styled("B", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" and use "),
            Span::styled("Up↑/Down↓ Keys", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" to select a new Base Node, "),
            Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" to set."),
        ]))
        .block(Block::default());
        f.render_widget(instructions, areas[0]);

        let selected_peer = app_state.get_selected_base_node();
        let base_node_list = app_state.get_base_node_list();
        let capacity = base_node_list.len();

        let mut column0_items = Vec::with_capacity(capacity);
        let mut column1_items = Vec::with_capacity(capacity);

        let styles: HashMap<bool, Style> = [
            (true, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            (false, Style::default().fg(Color::Reset)),
        ]
        .iter()
        .cloned()
        .collect();

        for (peer_type, peer) in base_node_list {
            let selected = peer == selected_peer;
            let style = styles
                .get(&selected)
                .unwrap_or(&Style::default().fg(Color::Reset))
                .to_owned();
            column0_items.push(ListItem::new(Span::styled(peer_type, style)));
            column1_items.push(ListItem::new(Span::styled(peer.public_key.to_string(), style)));
        }

        self.base_node_list_state.set_num_items(capacity);
        let mut base_node_list_state = self
            .base_node_list_state
            .get_list_state((areas[1].height as usize).saturating_sub(3));

        let column_list = MultiColumnList::new()
            .highlight_style(Style::default().add_modifier(Modifier::BOLD).fg(Color::Magenta))
            .heading_style(Style::default().fg(Color::Magenta))
            .max_width(MAX_WIDTH)
            .add_column(Some("Type"), Some(28), column0_items)
            .add_column(Some("Public Key"), Some(65), column1_items);
        column_list.render(f, areas[1], &mut base_node_list_state);
    }

    fn draw_detailed_base_node<B>(&self, f: &mut Frame<B>, area: Rect, _app_state: &AppState)
    where B: Backend {
        let block = Block::default().borders(Borders::ALL).title(Span::styled(
            "Base Node Detail",
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ));
        f.render_widget(block, area);

        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(24), Constraint::Min(2)].as_ref())
            .margin(1)
            .split(area);

        // Labels:
        let label_layout = Layout::default()
            .constraints([Constraint::Length(1), Constraint::Length(1), Constraint::Length(1)].as_ref())
            .split(columns[0]);

        let node_id = Span::styled("Node ID:", Style::default().fg(Color::Magenta));
        let public_key = Span::styled("Public Key:", Style::default().fg(Color::Magenta));
        let address = Span::styled("Address:", Style::default().fg(Color::Magenta));

        let paragraph = Paragraph::new(node_id).wrap(Wrap { trim: true });
        f.render_widget(paragraph, label_layout[0]);
        let paragraph = Paragraph::new(public_key).wrap(Wrap { trim: true });
        f.render_widget(paragraph, label_layout[1]);
        let paragraph = Paragraph::new(address).wrap(Wrap { trim: true });
        f.render_widget(paragraph, label_layout[2]);

        // Content:
        if let Some(peer) = self.detailed_base_node.as_ref() {
            let content_layout = Layout::default()
                .constraints([Constraint::Length(1), Constraint::Length(1), Constraint::Length(1)].as_ref())
                .split(columns[1]);

            let node_id = Span::styled(format!("{}", peer.node_id), Style::default().fg(Color::White));
            let public_key = Span::styled(peer.public_key.to_hex(), Style::default().fg(Color::White));
            let address = Span::styled(display_address(peer), Style::default().fg(Color::White));

            let paragraph = Paragraph::new(node_id).wrap(Wrap { trim: true });
            f.render_widget(paragraph, content_layout[0]);
            let paragraph = Paragraph::new(public_key).wrap(Wrap { trim: true });
            f.render_widget(paragraph, content_layout[1]);
            let paragraph = Paragraph::new(address).wrap(Wrap { trim: true });
            f.render_widget(paragraph, content_layout[2]);
        }
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

        let base_node_layout = Layout::default()
            .constraints([Constraint::Length(2), Constraint::Length(3), Constraint::Length(3)].as_ref())
            .margin(1)
            .split(area);

        let mut instructions = vec![
            Span::raw("Press "),
            Span::styled("P", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" to set custom "),
            Span::styled(
                "Base Node Public Key and Address",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(" fields. "),
        ];

        if app_state.get_custom_base_node().is_some() {
            instructions.extend(vec![
                Span::raw("Press "),
                Span::styled("C", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to clear the custom base node & revert to config."),
            ]);
        }

        let instructions_p = Paragraph::new(Spans::from(instructions)).block(Block::default());
        f.render_widget(instructions_p, base_node_layout[0]);

        let peer = app_state.get_selected_base_node();
        let (public_key, style) = match self.base_node_edit_mode {
            BaseNodeInputMode::PublicKey => (self.public_key_field.clone(), Style::default().fg(Color::Magenta)),
            BaseNodeInputMode::Address => (self.public_key_field.clone(), Style::default()),
            _ => (peer.public_key.to_hex(), Style::default()),
        };

        let pubkey_input = Paragraph::new(public_key)
            .style(style)
            .block(Block::default().borders(Borders::ALL).title("(P)ublic Key:"));
        f.render_widget(pubkey_input, base_node_layout[1]);

        let (public_address, style) = match self.base_node_edit_mode {
            BaseNodeInputMode::PublicKey => (self.address_field.clone(), Style::default()),
            BaseNodeInputMode::Address => (self.address_field.clone(), Style::default().fg(Color::Magenta)),
            _ => (display_address(peer), Style::default()),
        };

        let address_input = Paragraph::new(public_address)
            .style(style)
            .block(Block::default().borders(Borders::ALL).title("Address:"));
        f.render_widget(address_input, base_node_layout[2]);
    }

    fn on_key_confirm_dialog(&mut self, c: char, app_state: &mut AppState) -> KeyHandled {
        if self.confirmation_dialog {
            if 'n' == c {
                self.confirmation_dialog = false;
                return KeyHandled::Handled;
            } else if 'y' == c {
                match Handle::current().block_on(app_state.clear_custom_base_node()) {
                    Ok(_) => info!(
                        target: LOG_TARGET,
                        "Cleared custom base node and reverted to Config base node"
                    ),
                    Err(e) => warn!(target: LOG_TARGET, "Error clearing custom base node data: {}", e),
                }

                self.previous_public_key_field = self.public_key_field.clone();
                self.previous_address_field = self.address_field.clone();
                let base_node_previous = app_state.get_previous_base_node().clone();
                let public_key = base_node_previous.public_key.to_hex();
                let public_address = display_address(&base_node_previous);
                self.public_key_field = public_key;
                self.address_field = public_address;
                self.confirmation_dialog = false;
                self.base_node_edit_mode = BaseNodeInputMode::Selection;
                return KeyHandled::Handled;
            }
        }
        KeyHandled::NotHandled
    }

    fn on_key_base_node_edit(&mut self, c: char, app_state: &mut AppState) -> KeyHandled {
        if self.base_node_edit_mode != BaseNodeInputMode::None {
            match self.base_node_edit_mode {
                BaseNodeInputMode::PublicKey => match c {
                    '\n' => {
                        self.previous_address_field = self.address_field.clone();
                        self.address_field = "".to_string();
                        self.base_node_edit_mode = BaseNodeInputMode::Address;
                        return KeyHandled::Handled;
                    },
                    c => {
                        self.public_key_field.push(c);
                        return KeyHandled::Handled;
                    },
                },
                BaseNodeInputMode::Address => match c {
                    '\n' => {
                        match Handle::current().block_on(
                            app_state.set_custom_base_node(self.public_key_field.clone(), self.address_field.clone()),
                        ) {
                            Ok(peer) => {
                                self.previous_address_field = self.address_field.clone();
                                self.previous_public_key_field = self.public_key_field.clone();
                                self.detailed_base_node = Some(peer);
                            },
                            Err(e) => {
                                warn!(target: LOG_TARGET, "Could not set custom base node peer: {}", e);
                                self.error_message = Some(format!("Error setting new Base Node Address:\n{}", e));
                                self.address_field = self.previous_address_field.clone();
                                self.public_key_field = self.previous_public_key_field.clone();
                            },
                        }

                        self.base_node_edit_mode = BaseNodeInputMode::None;
                        return KeyHandled::Handled;
                    },
                    c => {
                        self.address_field.push(c);
                        return KeyHandled::Handled;
                    },
                },
                BaseNodeInputMode::Selection => match c {
                    '\n' => {
                        if let Some(peer) = self.detailed_base_node.clone() {
                            if let Err(e) = Handle::current().block_on(app_state.set_base_node_peer(peer)) {
                                warn!(target: LOG_TARGET, "Could not set new base node peer: {}", e);
                                self.error_message = Some(format!("Error setting new Base Node Address:\n{}", e));
                            }
                        }

                        self.base_node_list_state.select(None);
                        self.base_node_edit_mode = BaseNodeInputMode::None;
                        return KeyHandled::Handled;
                    },
                    _ => return KeyHandled::Handled,
                },
                BaseNodeInputMode::None => (),
            }
        }
        KeyHandled::NotHandled
    }
}

impl<B: Backend> Component<B> for NetworkTab {
    fn draw(&mut self, f: &mut Frame<B>, area: Rect, app_state: &AppState) {
        let areas = Layout::default()
            .constraints(
                [
                    Constraint::Length(3),
                    Constraint::Length(10),
                    Constraint::Length(8 + app_state.get_base_node_list().len() as u16),
                    Constraint::Length(5),
                    Constraint::Min(12),
                ]
                .as_ref(),
            )
            .split(area);

        self.balance.draw(f, areas[0], app_state);
        self.draw_base_node_peer(f, areas[1], app_state);
        self.draw_base_node_selection(f, areas[2], app_state);
        self.draw_detailed_base_node(f, areas[3], app_state);
        self.draw_connected_peers_list(f, areas[4], app_state);

        if let Some(msg) = self.error_message.clone() {
            draw_dialog(f, area, "Error!".to_string(), msg, Color::Red, 120, 9);
        }

        if self.confirmation_dialog {
            draw_dialog(
                f,
                area,
                "Confirm clearing custom Base Node".to_string(),
                "Are you sure you want to clear the custom Base node?\n(Y)es / (N)o".to_string(),
                Color::Red,
                120,
                9,
            );
        }
    }

    fn on_key(&mut self, app_state: &mut AppState, c: char) {
        if self.error_message.is_some() && '\n' == c {
            self.error_message = None;
            return;
        }

        if self.on_key_confirm_dialog(c, app_state) == KeyHandled::Handled {
            return;
        }
        if self.on_key_base_node_edit(c, app_state) == KeyHandled::Handled {
            return;
        }

        match c {
            'p' => {
                self.previous_public_key_field = self.public_key_field.clone();
                self.public_key_field = "".to_string();
                self.base_node_edit_mode = BaseNodeInputMode::PublicKey;
            },
            'c' => {
                if app_state.get_custom_base_node().is_some() {
                    self.confirmation_dialog = true;
                }
            },
            'b' => {
                self.base_node_list_state
                    .set_num_items(app_state.get_base_node_list().len());
                self.base_node_edit_mode = BaseNodeInputMode::Selection;
                self.base_node_list_state.select_first();
                if let Some(index) = self.base_node_list_state.selected() {
                    self.detailed_base_node = app_state.get_base_node_list().get(index).map(|(_, peer)| peer.clone());
                };
            },
            's' => {
                // set the currently selected base node as a custom base node
                let base_node = app_state.get_selected_base_node();
                let public_key = base_node.public_key.to_hex();
                let address = base_node
                    .addresses
                    .first()
                    .map(|a| a.to_string())
                    .unwrap_or_else(|| "".to_string());

                match Handle::current().block_on(app_state.set_custom_base_node(public_key, address)) {
                    Ok(peer) => {
                        self.previous_address_field = self.address_field.clone();
                        self.previous_public_key_field = self.public_key_field.clone();
                        self.detailed_base_node = Some(peer);
                    },
                    Err(e) => {
                        warn!(target: LOG_TARGET, "Could not set custom base node peer: {}", e);
                        self.error_message = Some(format!("Error setting new Base Node Address:\n{}", e));
                        self.address_field = self.previous_address_field.clone();
                        self.public_key_field = self.previous_public_key_field.clone();
                    },
                }
            },
            _ => {},
        }
    }

    fn on_backspace(&mut self, _app_state: &mut AppState) {
        match self.base_node_edit_mode {
            BaseNodeInputMode::PublicKey => {
                let _ = self.public_key_field.pop();
            },
            BaseNodeInputMode::Address => {
                let _ = self.address_field.pop();
            },
            _ => {},
        }
    }

    fn on_esc(&mut self, app_state: &mut AppState) {
        match self.base_node_edit_mode {
            BaseNodeInputMode::PublicKey | BaseNodeInputMode::Address => {
                self.public_key_field = self.previous_public_key_field.clone();
                self.address_field = self.previous_address_field.clone();
                self.base_node_edit_mode = BaseNodeInputMode::None;
            },
            _ => {
                self.base_node_list_state.select(None);
                self.base_node_edit_mode = BaseNodeInputMode::None;
                self.detailed_base_node = Some(app_state.get_selected_base_node().clone());
            },
        }
    }

    fn on_down(&mut self, app_state: &mut AppState) {
        if matches!(self.base_node_edit_mode, BaseNodeInputMode::Selection) {
            self.base_node_list_state
                .set_num_items(app_state.get_base_node_list().len());
            self.base_node_list_state.next();
            self.detailed_base_node = match self.base_node_list_state.selected() {
                None => Some(app_state.get_selected_base_node().clone()),
                Some(i) => {
                    let (_, peer) = match app_state.get_base_node_list().get(i) {
                        None => ("".to_string(), None),
                        Some((peer_type, peer)) => (peer_type.to_owned(), Some(peer.clone())),
                    };
                    peer
                },
            };
        }
    }

    fn on_up(&mut self, app_state: &mut AppState) {
        if matches!(self.base_node_edit_mode, BaseNodeInputMode::Selection) {
            self.base_node_list_state
                .set_num_items(app_state.get_base_node_list().len());
            self.base_node_list_state.previous();
            self.detailed_base_node = match self.base_node_list_state.selected() {
                None => Some(app_state.get_selected_base_node().clone()),
                Some(i) => {
                    let (_, peer) = match app_state.get_base_node_list().get(i) {
                        None => ("".to_string(), None),
                        Some((peer_type, peer)) => (peer_type.to_owned(), Some(peer.clone())),
                    };
                    peer
                },
            };
        }
    }
}
#[derive(PartialEq, Debug)]
pub enum BaseNodeInputMode {
    None,
    PublicKey,
    Address,
    Selection,
}
