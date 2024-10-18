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

use minotari_wallet::{error::WalletError, util::wallet_identity::WalletIdentity, WalletConfig, WalletSqlite};
use tari_common::exit_codes::{ExitCode, ExitError};
use tari_network::Peer;
use tokio::runtime::Handle;
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout},
    Frame,
};

use crate::{
    notifier::Notifier,
    ui::{
        components::{
            base_node::BaseNode,
            burn_tab::BurnTab,
            contacts_tab::ContactsTab,
            events_component::EventsComponent,
            log_tab::LogTab,
            menu::Menu,
            network_tab::NetworkTab,
            notification_tab::NotificationTab,
            receive_tab::ReceiveTab,
            register_template_tab::RegisterTemplateTab,
            send_tab::SendTab,
            tabs_container::TabsContainer,
            transactions_tab::TransactionsTab,
            Component,
        },
        state::AppState,
        MAX_WIDTH,
    },
    wallet_modes::PeerConfig,
};

pub const LOG_TARGET: &str = "wallet::ui::app";

pub struct App<B: Backend> {
    #[allow(dead_code)]
    pub title: String,
    pub should_quit: bool,
    // Cached state this will need to be cleaned up into a threadsafe container
    pub app_state: AppState,
    // Ui working state
    pub tabs: TabsContainer<B>,
    pub base_node_status: BaseNode,
    pub menu: Menu,
    pub notifier: Notifier,
}

impl<B: Backend> App<B> {
    pub async fn new(
        title: String,
        wallet: WalletSqlite,
        wallet_config: WalletConfig,
        base_node_selected: Option<Peer>,
        base_node_config: PeerConfig,
        notifier: Notifier,
    ) -> Result<Self, ExitError> {
        let wallet_address_interactive = wallet
            .get_wallet_interactive_address()
            .await
            .map_err(WalletError::KeyManagerServiceError)?;
        let wallet_address_one_sided = wallet
            .get_wallet_one_sided_address()
            .await
            .map_err(WalletError::KeyManagerServiceError)?;
        let wallet_id = WalletIdentity::new(
            wallet.network_public_key.clone(),
            wallet_address_interactive,
            wallet_address_one_sided,
        );
        let app_state = AppState::new(
            &wallet_id,
            wallet,
            base_node_selected.clone(),
            base_node_config,
            wallet_config,
        );

        let tabs = TabsContainer::<B>::new(title.clone())
            .add("Transactions".into(), Box::new(TransactionsTab::new()))
            .add(
                "Send".into(),
                Box::new(SendTab::new(
                    &app_state,
                    app_state
                        .get_wallet_type()
                        .await
                        .map_err(|e| ExitError::new(ExitCode::WalletError, e))?,
                )),
            )
            .add("Receive".into(), Box::new(ReceiveTab::new()))
            .add("Burn".into(), Box::new(BurnTab::new(&app_state)))
            .add("Templates".into(), Box::new(RegisterTemplateTab::new(&app_state)))
            .add("Contacts".into(), Box::new(ContactsTab::new()))
            .add("Network".into(), Box::new(NetworkTab::new(base_node_selected)))
            .add("Events".into(), Box::new(EventsComponent::new()))
            .add("Log".into(), Box::new(LogTab::new()))
            .add("Notifications".into(), Box::new(NotificationTab::new()));

        let base_node_status = BaseNode::new();
        let menu = Menu::new();

        Ok(Self {
            title,
            should_quit: false,
            app_state,
            tabs,
            base_node_status,
            menu,
            notifier,
        })
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

    pub fn on_backtab(&mut self) {
        self.tabs.previous();
    }

    pub fn on_up(&mut self) {
        self.tabs.on_up(&mut self.app_state);
    }

    pub fn on_down(&mut self) {
        self.tabs.on_down(&mut self.app_state);
    }

    pub fn on_f10(&mut self) {
        self.should_quit = true;
    }

    pub fn on_right(&mut self) {
        self.tabs.next();
    }

    pub fn on_left(&mut self) {
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
            .constraints(
                [
                    Constraint::Length(3),
                    Constraint::Min(0),
                    Constraint::Length(2),
                    Constraint::Length(1),
                ]
                .as_ref(),
            )
            .split(max_width_layout[0]);

        self.tabs.draw_titles(f, title_chunks[0], &self.app_state);
        self.tabs.draw_content(f, title_chunks[1], &mut self.app_state);
        self.base_node_status.draw(f, title_chunks[2], &self.app_state);
        self.menu.draw(f, title_chunks[3], &self.app_state);
    }
}
