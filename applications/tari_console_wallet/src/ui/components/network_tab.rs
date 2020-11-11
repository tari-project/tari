use crate::ui::{
    components::Component,
    state::AppState,
    widgets::{draw_dialog, MultiColumnList},
    MAX_WIDTH,
};
use log::*;
use tari_crypto::tari_utilities::hex::Hex;
use tokio::runtime::Handle;
use tui::{
    backend::Backend,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, ListItem, ListState, Paragraph},
    Frame,
};

const LOG_TARGET: &str = "wallet::console_wallet::network_tab";

pub struct NetworkTab {
    base_node_edit_mode: BaseNodeInputMode,
    public_key_field: String,
    previous_public_key_field: String,
    address_field: String,
    previous_address_field: String,
    error_message: Option<String>,
    confirmation_dialog: bool,
}

impl NetworkTab {
    pub fn new(public_key_field: String, address_field: String) -> Self {
        Self {
            base_node_edit_mode: BaseNodeInputMode::None,
            public_key_field: public_key_field.clone(),
            previous_public_key_field: public_key_field,
            address_field: address_field.clone(),
            previous_address_field: address_field,
            error_message: None,
            confirmation_dialog: false,
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

    pub fn draw_base_node_peer<B>(&self, f: &mut Frame<B>, area: Rect, _app_state: &AppState)
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

        let instructions = Paragraph::new(Spans::from(vec![
            Span::raw("Press "),
            Span::styled("P", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" to edit "),
            Span::styled(
                "Base Node Public Key and Address",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(" fields, "),
            Span::styled("C", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" to clear the clear the custom base node & revert to config"),
        ]))
        .block(Block::default());
        f.render_widget(instructions, base_node_layout[0]);

        let pubkey_input = Paragraph::new(self.public_key_field.as_ref())
            .style(match self.base_node_edit_mode {
                BaseNodeInputMode::PublicKey => Style::default().fg(Color::Magenta),
                _ => Style::default(),
            })
            .block(Block::default().borders(Borders::ALL).title("(P)ublic Key:"));
        f.render_widget(pubkey_input, base_node_layout[1]);

        let address_input = Paragraph::new(self.address_field.as_ref())
            .style(match self.base_node_edit_mode {
                BaseNodeInputMode::Address => Style::default().fg(Color::Magenta),
                _ => Style::default(),
            })
            .block(Block::default().borders(Borders::ALL).title("Address:"));
        f.render_widget(address_input, base_node_layout[2]);
    }
}

impl<B: Backend> Component<B> for NetworkTab {
    fn draw(&mut self, f: &mut Frame<B>, area: Rect, app_state: &AppState) {
        let main_chunks = Layout::default()
            .constraints([Constraint::Length(1), Constraint::Length(10), Constraint::Min(10)].as_ref())
            .split(area);

        self.draw_base_node_peer(f, main_chunks[1], app_state);
        self.draw_connected_peers_list(f, main_chunks[2], app_state);

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

        if self.confirmation_dialog {
            if 'n' == c {
                self.confirmation_dialog = false;
                return;
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
                let config_base_node = app_state.get_config_base_node().clone();
                let public_key = config_base_node.public_key.to_hex();
                let public_address = match config_base_node.addresses.first() {
                    Some(address) => address.to_string(),
                    None => "".to_string(),
                };
                self.public_key_field = public_key;
                self.address_field = public_address;
                self.confirmation_dialog = false;
                return;
            }
        }

        if self.base_node_edit_mode != BaseNodeInputMode::None {
            match self.base_node_edit_mode {
                BaseNodeInputMode::None => (),
                BaseNodeInputMode::PublicKey => match c {
                    '\n' => {
                        self.previous_address_field = self.address_field.clone();
                        self.address_field = "".to_string();
                        self.base_node_edit_mode = BaseNodeInputMode::Address;
                        return;
                    },
                    c => {
                        self.public_key_field.push(c);
                        return;
                    },
                },
                BaseNodeInputMode::Address => match c {
                    '\n' => {
                        if let Err(e) = Handle::current().block_on(
                            app_state.set_custom_base_node(self.public_key_field.clone(), self.address_field.clone()),
                        ) {
                            warn!(target: LOG_TARGET, "Could not set custom base node peer: {}", e);
                            self.error_message =
                                Some(format!("Error setting new Base Node Address:\n{}", e.to_string()));
                            self.address_field = self.previous_address_field.clone();
                            self.public_key_field = self.previous_public_key_field.clone();
                        } else {
                            self.previous_address_field = self.address_field.clone();
                            self.previous_public_key_field = self.public_key_field.clone();
                        }

                        self.base_node_edit_mode = BaseNodeInputMode::None;
                        return;
                    },
                    c => {
                        self.address_field.push(c);
                        return;
                    },
                },
            }
        }

        match c {
            'p' => {
                self.previous_public_key_field = self.public_key_field.clone();
                self.public_key_field = "".to_string();
                self.base_node_edit_mode = BaseNodeInputMode::PublicKey;
            },
            'c' => {
                self.confirmation_dialog = true;
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
            BaseNodeInputMode::None => {},
        }
    }
}

#[derive(PartialEq, Debug)]
pub enum BaseNodeInputMode {
    None,
    PublicKey,
    Address,
}
