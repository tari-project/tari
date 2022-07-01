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

use tari_utilities::hex::Hex;
use tui::{
    backend::Backend,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, Paragraph, Row, Table, TableState, Wrap},
    Frame,
};

use crate::ui::{
    components::{styles, Component},
    state::AppState,
};

pub struct AssetsTab {
    table_state: TableState,
}

impl AssetsTab {
    pub fn new() -> Self {
        Self {
            table_state: TableState::default(),
        }
    }
}

impl<B: Backend> Component<B> for AssetsTab {
    fn draw(&mut self, f: &mut Frame<B>, area: Rect, app_state: &AppState) {
        let list_areas = Layout::default()
            .constraints([Constraint::Length(1), Constraint::Min(42)].as_ref())
            .split(area);

        let instructions = Paragraph::new(Spans::from(vec![
            Span::raw("Use "),
            Span::styled("Up↑/Down↓ Keys", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" to select a contract."),
        ]))
        .wrap(Wrap { trim: true });

        f.render_widget(instructions, list_areas[0]);
        let constitutions = app_state.get_owned_constitutions();

        let constitutions: Vec<_> = constitutions
            .iter()
            .map(|r| {
                (r.unblinded_output
                    .features
                    .sidechain_features
                    .clone()
                    .unwrap()
                    .contract_id
                    .to_hex(),)
            })
            .collect();
        let rows: Vec<_> = constitutions.iter().map(|v| Row::new(vec![v.0.as_str()])).collect();
        let table = Table::new(rows)
            .header(Row::new(vec!["Name", "Status", "Pub Key", "Owner"]).style(styles::header_row()))
            .block(Block::default().title("Assets").borders(Borders::ALL))
            .widths(&[Constraint::Length(30 + 20 + 64 + 64)])
            .highlight_style(styles::highlight())
            .highlight_symbol(">>");
        f.render_stateful_widget(table, list_areas[1], &mut self.table_state);
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
        let constitutions = app_state.get_owned_constitutions();
        if index > constitutions.len().saturating_sub(1) {
            self.table_state.select(None);
        } else {
            self.table_state.select(Some(index));
        }
    }
}
