// Copyright 2020. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use crate::ui::{
    components::{
        base_node::BaseNode,
        network_tab::NetworkTab,
        send_receive_tab::SendReceiveTab,
        tabs_container::TabsContainer,
        transactions_tab::TransactionsTab,
        Component,
    },
    state::AppState,
    MAX_WIDTH,
};
use tari_common::Network;
use tari_comms::{peer_manager::Peer, NodeIdentity};
use tari_wallet::WalletSqlite;
use tokio::runtime::Handle;
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout},
    Frame,
};

pub const LOG_TARGET: &str = "wallet::ui::app";

pub struct App<B: Backend> {
    pub title: String,
    pub should_quit: bool,
    // Cached state this will need to be cleaned up into a threadsafe container
    pub app_state: AppState,
    // Ui working state
    pub tabs: TabsContainer<B>,
    pub base_node_status: BaseNode,
}

impl<B: Backend> App<B> {
    pub fn new(
        title: String,
        node_identity: &NodeIdentity,
        wallet: WalletSqlite,
        network: Network,
        base_node: Peer,
    ) -> Self
    {
        // TODO: It's probably better to read the node_identity from the wallet, but that requires
        // taking a read lock and making this method async, which adds some read/write cycles,
        // so it's easier to just ask for it right now
        let app_state = AppState::new(&node_identity, network, wallet, base_node);

        let tabs = TabsContainer::<B>::new(title.clone())
            .add("Transactions".into(), Box::new(TransactionsTab::new()))
            .add("Send/Receive".into(), Box::new(SendReceiveTab::new()))
            .add("Network".into(), Box::new(NetworkTab::new()));

        let base_node_status = BaseNode::new();

        Self {
            title,

            should_quit: false,
            app_state,
            tabs,
            base_node_status,
        }
    }

    pub fn on_control_key(&mut self, c: char) {
        match c {
            'q' | 'c' => {
                self.should_quit = true;
            },
            _ => (),
        }
    }

    pub fn on_key(&mut self, c: char) {
        match c {
            '\t' => {
                self.tabs.next();
            },
            _ => self.tabs.on_key(&mut self.app_state, c),
        }
    }

    pub fn on_up(&mut self) {
        self.tabs.on_up(&mut self.app_state);
    }

    pub fn on_down(&mut self) {
        self.tabs.on_down(&mut self.app_state);
    }

    pub fn on_right(&mut self) {
        // This currently doesn't need app_state, but is async
        // to match others
        self.tabs.next();
    }

    pub fn on_left(&mut self) {
        // This currently doesn't need app_state, but is async
        // to match others
        self.tabs.previous();
    }

    pub fn on_esc(&mut self) {
        self.tabs.on_esc(&mut self.app_state);
    }

    pub fn on_backspace(&mut self) {
        self.tabs.on_backspace(&mut self.app_state);
    }

    pub fn on_tick(&mut self) {
        Handle::current().block_on(self.app_state.update_cache());
        self.tabs.on_tick(&mut self.app_state);
    }

    pub fn draw(&mut self, f: &mut Frame<'_, B>) {
        let max_width_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(MAX_WIDTH), Constraint::Min(0)].as_ref())
            .split(f.size());
        let title_chunks = Layout::default()
            .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
            .split(max_width_layout[0]);
        let title_halves = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
            .split(title_chunks[0]);

        self.tabs.draw_titles(f, title_halves[0]);

        self.base_node_status.draw(f, title_halves[1], &self.app_state);
        self.tabs.draw_content(f, title_chunks[1], &mut self.app_state);
    }
}
