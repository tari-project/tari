// Copyright 2021. The Tari Project
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

use tari_crypto::tari_utilities::hex::Hex;
use tui::{
    backend::Backend,
    layout::{Constraint, Rect},
    widgets::{Block, Borders, Row, Table, TableState},
    Frame,
};

use crate::ui::{
    components::{styles, Component},
    state::AppState,
};

pub struct TokensComponent {
    table_state: TableState,
}

impl TokensComponent {
    pub fn new() -> Self {
        Self {
            table_state: TableState::default(),
        }
    }
}

impl<B: Backend> Component<B> for TokensComponent {
    fn draw(&mut self, f: &mut Frame<B>, area: Rect, app_state: &AppState) {
        let tokens = app_state.get_owned_tokens();

        let tokens: Vec<_> = tokens
            .iter()
            .map(|r| {
                (
                    r.name().to_string(),
                    r.output_status().to_string(),
                    r.asset_public_key().to_hex(),
                    Vec::from(r.unique_id()).to_hex(),
                    r.owner_commitment().to_hex(),
                )
            })
            .collect();
        let rows: Vec<_> = tokens
            .iter()
            .map(|v| {
                Row::new(vec![
                    v.0.as_str(),
                    v.1.as_str(),
                    v.2.as_str(),
                    v.3.as_str(),
                    v.4.as_str(),
                ])
            })
            .collect();
        let table = Table::new(rows)
            .header(Row::new(vec!["Name", "Status", "Asset Pub Key", "Unique ID", "Owner"]).style(styles::header_row()))
            .block(Block::default().title("Tokens").borders(Borders::ALL))
            .widths(&[
                Constraint::Length(30),
                Constraint::Length(20),
                Constraint::Length(32),
                Constraint::Length(32),
                Constraint::Length(64),
            ])
            .highlight_style(styles::highlight())
            .highlight_symbol(">>");
        f.render_stateful_widget(table, area, &mut self.table_state)
    }

    fn on_up(&mut self, _app_state: &mut AppState) {
        let index = self.table_state.selected().unwrap_or_default();
        if index == 0 {
            self.table_state.select(None);
        } else {
            self.table_state.select(Some(index - 1));
        }
    }

    fn on_down(&mut self, app_state: &mut AppState) {
        let index = self.table_state.selected().map(|s| s + 1).unwrap_or_default();
        let tokens = app_state.get_owned_tokens();
        if index > tokens.len().saturating_sub(1) {
            self.table_state.select(None);
        } else {
            self.table_state.select(Some(index));
        }
    }
}
