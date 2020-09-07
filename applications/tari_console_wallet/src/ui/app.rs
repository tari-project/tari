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

use crate::{
    dummy_data::get_dummy_base_node_status,
    ui::{
        components::{
            network_tab::NetworkTab,
            send_receive_tab::SendReceiveTab,
            tabs_container::TabsContainer,
            transactions_tab::TransactionsTab,
            Component,
        },
        state::AppState,
        UiError,
        MAX_WIDTH,
    },
};
use std::sync::Arc;
use tari_common::Network;
use tari_comms::NodeIdentity;
use tari_wallet::{
    contacts_service::storage::sqlite_db::ContactsServiceSqliteDatabase,
    output_manager_service::storage::sqlite_db::OutputManagerSqliteDatabase,
    storage::sqlite_db::WalletSqliteDatabase,
    transaction_service::storage::{database::CompletedTransaction, sqlite_db::TransactionServiceSqliteDatabase},
    Wallet,
};
use tokio::sync::RwLock;
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

pub const LOG_TARGET: &str = "wallet::ui::app";

pub struct App<B: Backend> {
    pub title: String,
    pub should_quit: bool,
    pub wallet: Arc<
        RwLock<
            Wallet<
                WalletSqliteDatabase,
                TransactionServiceSqliteDatabase,
                OutputManagerSqliteDatabase,
                ContactsServiceSqliteDatabase,
            >,
        >,
    >,
    // Cached state this will need to be cleaned up into a threadsafe container
    pub app_state: Arc<RwLock<AppState>>,
    // Ui working state
    pub tabs: TabsContainer<B>,
}

impl<B: Backend> App<B> {
    pub fn new(
        title: String,
        node_identity: &NodeIdentity,
        wallet: Arc<
            RwLock<
                Wallet<
                    WalletSqliteDatabase,
                    TransactionServiceSqliteDatabase,
                    OutputManagerSqliteDatabase,
                    ContactsServiceSqliteDatabase,
                >,
            >,
        >,
        network: Network,
    ) -> Self
    {
        // TODO: It's probably better to read the node_identity from the wallet, but that requires
        // taking a read lock and making this method async, which adds some read/write cycles,
        // so it's easier to just ask for it right now
        let app_state = Arc::new(RwLock::new(AppState::new(&node_identity, network)));

        let tabs = TabsContainer::<B>::new(title.clone())
            .add("Transactions".into(), Box::new(TransactionsTab::new()))
            .add("Send/Receive".into(), Box::new(SendReceiveTab::new()))
            .add("Network".into(), Box::new(NetworkTab::new()));

        Self {
            title,
            wallet,
            should_quit: false,
            app_state,
            // tabs: TabsState::new(vec!["Transactions".into(), "Send/Receive".into(), "Network".into()]),
            tabs,
        }
    }

    pub async fn on_control_key(&mut self, c: char) {
        match c {
            'c' => {
                self.should_quit = true;
            },
            _ => {},
        }
    }

    pub async fn on_key(&mut self, c: char) {
        match c {
            'q' => {
                self.should_quit = true;
            },
            '\t' => {
                self.tabs.next();
                return;
            },
            _ => {
                let mut app_state = self.app_state.write().await;
                self.tabs.on_key(&mut app_state, c)
            },
        }
    }

    pub async fn on_up(&mut self) {
        let mut app_state = self.app_state.write().await;
        self.tabs.on_up(&mut app_state);
    }

    pub async fn on_down(&mut self) {
        let mut app_state = self.app_state.write().await;
        self.tabs.on_down(&mut app_state);
    }

    pub async fn on_right(&mut self) {
        // This currently doesn't need app_state, but is async
        // to match others
        self.tabs.next();
    }

    pub async fn on_left(&mut self) {
        // This currently doesn't need app_state, but is async
        // to match others
        self.tabs.previous();
    }

    pub async fn on_esc(&mut self) {
        let mut app_state = self.app_state.write().await;
        self.tabs.on_esc(&mut app_state);
    }

    pub async fn on_backspace(&mut self) {
        let mut app_state = self.app_state.write().await;
        self.tabs.on_backspace(&mut app_state);
    }

    pub fn on_tick(&mut self) {}

    pub async fn refresh_state(&mut self) -> Result<(), UiError> {
        let mut pending_transactions: Vec<CompletedTransaction> = Vec::new();
        pending_transactions.extend(
            self.wallet
                .write()
                .await
                .transaction_service
                .get_pending_inbound_transactions()
                .await?
                .values()
                .map(|t| CompletedTransaction::from(t.clone()))
                .collect::<Vec<CompletedTransaction>>(),
        );
        pending_transactions.extend(
            self.wallet
                .write()
                .await
                .transaction_service
                .get_pending_inbound_transactions()
                .await?
                .values()
                .map(|t| CompletedTransaction::from(t.clone()))
                .collect::<Vec<CompletedTransaction>>(),
        );

        pending_transactions.sort_by(|a: &CompletedTransaction, b: &CompletedTransaction| {
            b.timestamp.partial_cmp(&a.timestamp).unwrap()
        });
        self.app_state.write().await.pending_txs.items = pending_transactions;
        let completed_transactions = self
            .wallet
            .write()
            .await
            .transaction_service
            .get_completed_transactions()
            .await?
            .values()
            .map(|t| t.clone())
            .collect();
        self.app_state.write().await.completed_txs.items = completed_transactions;
        Ok(())
    }

    pub async fn draw(&mut self, f: &mut Frame<'_, B>) {
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
        let app_state = self.app_state.read().await;

        let chain_meta_data = match get_dummy_base_node_status() {
            None => Spans::from(vec![
                Span::styled("Base Node Chain Tip:", Style::default().fg(Color::Magenta)),
                Span::raw(" "),
                Span::styled(" *Not Connected*", Style::default().fg(Color::Red)),
            ]),
            Some(tip) => Spans::from(vec![
                Span::styled("Base Node Chain Tip:", Style::default().fg(Color::Magenta)),
                Span::raw(" "),
                Span::styled(format!("{}", tip), Style::default().fg(Color::Green)),
            ]),
        };
        let chain_meta_data_paragraph =
            Paragraph::new(chain_meta_data).block(Block::default().borders(Borders::ALL).title(Span::styled(
                "Base Node Status:",
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            )));
        f.render_widget(chain_meta_data_paragraph, title_halves[1]);

        self.tabs.draw_content(f, title_chunks[1], &app_state);
    }
}
