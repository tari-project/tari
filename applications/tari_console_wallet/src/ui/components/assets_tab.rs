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


use crate::ui::components::{Component, styles};
use tui::backend::Backend;
use tui::Frame;
use tui::layout::{Rect, Constraint};
use crate::ui::state::AppState;
use tui::widgets::{Table, Block, Row, TableState, Borders};

pub struct AssetsTab {
    table_state: TableState,
    assets: Vec<AssetListItem>
}

impl AssetsTab {

    pub fn new() -> Self {
        Self{  table_state: TableState::default(),
        assets: vec![ AssetListItem{ name: "Yat".into(), pub_key: "pub".into() }]}
    }
}

impl<B:Backend> Component<B> for AssetsTab {
    fn draw(&mut self, f: &mut Frame<B>, area: Rect, app_state: &AppState) {

        let assets = app_state.get_owned_assets();


        let rows :Vec<_>= self.assets.iter().map(|r| Row::new(vec![r.name.as_str(), r.pub_key.as_str()])).collect();
        let table = Table::new(rows)
            .header(Row::new(vec!["Name", "Pub Key"]).style(styles::header_row())).block(Block::default().title("Assets").borders(Borders::ALL)).widths(&[Constraint::Length(10), Constraint::Length(20)]).highlight_style(styles::highlight()).highlight_symbol(">>");
        f.render_stateful_widget(table, area, &mut self.table_state)
    }

    fn on_up(&mut self, _app_state: &mut AppState) {
        let index =self.table_state.selected().unwrap_or_default();
        if index ==  0 {
            self.table_state.select(None);
        } else {
            self.table_state.select(Some(index - 1));
        }
    }

    fn on_down(&mut self, _app_state: &mut AppState) {
        let index =self.table_state.selected().map(|s| s + 1).unwrap_or_default();
        if index > self.assets.len() - 1  {
            self.table_state.select(None);
        } else {
            self.table_state.select(Some(index));
        }
    }
}


pub struct AssetListItem {
    name: String,
    pub_key: String,
}
