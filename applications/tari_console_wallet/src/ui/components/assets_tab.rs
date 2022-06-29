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
use tari_wallet::output_manager_service::storage::models::DbUnblindedOutput;
use tokio::{runtime::Handle, sync::watch};
use tui::{
    backend::Backend,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, Paragraph, Row, Table, TableState, Wrap},
    Frame,
};

use crate::ui::{
    components::{styles, Component},
    state::{AppState, UiTransactionSendStatus},
    widgets::draw_dialog,
};

pub struct AssetsTab {
    table_state: TableState,
    confirmation_dialog: bool,
    error_message: Option<String>,
}

impl AssetsTab {
    pub fn new() -> Self {
        Self {
            table_state: TableState::default(),
            confirmation_dialog: false,
            error_message: None,
        }
    }

    fn is_reclaimable(&self, app_state: &AppState, constitution: &DbUnblindedOutput) -> bool {
        if let Some(mined_height) = constitution.mined_height {
            if let Some(sidechain_features) = &constitution.unblinded_output.features.sidechain_features {
                if let Some(constitution) = &sidechain_features.constitution {
                    let expiry = constitution.acceptance_requirements.acceptance_period_expiry;
                    let base_node_state = app_state.get_base_node_state();
                    if let Some(ref metadata) = base_node_state.chain_metadata {
                        let tip = metadata.height_of_longest_chain();
                        if mined_height + expiry * 2 <= tip {
                            return true;
                        }
                    }
                }
            }
        }
        false
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
            Span::raw(" to select a contract, "),
            Span::styled("D", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" to (d)elete a contract."),
        ]))
        .wrap(Wrap { trim: true });
        f.render_widget(instructions, list_areas[0]);

        let constitutions = app_state.get_owned_constitutions();

        let constitutions: Vec<_> = constitutions
            .iter()
            .map(|r| {
                (
                    r.unblinded_output
                        .features
                        .sidechain_features
                        .clone()
                        .unwrap()
                        .contract_id
                        .to_hex(),
                    self.is_reclaimable(app_state, r),
                )
            })
            .collect();
        let rows: Vec<_> = constitutions
            .iter()
            .map(|v| Row::new(vec![v.0.as_str(), if v.1 { "Yes" } else { "No" }]))
            .collect();
        let table = Table::new(rows)
            .header(Row::new(vec!["Pub key", "Reclaimable"]).style(styles::header_row()))
            .block(Block::default().title("Assets").borders(Borders::ALL))
            .widths(&[Constraint::Length(30 + 20 + 64), Constraint::Length(64)])
            .highlight_style(styles::highlight())
            .highlight_symbol(">>");
        f.render_stateful_widget(table, list_areas[1], &mut self.table_state);
        if self.confirmation_dialog {
            draw_dialog(
                f,
                area,
                "Confirm contract reclamation".to_string(),
                "Are you sure you want to reclaim this contract?\n(Y)es / (N)o".to_string(),
                Color::Red,
                120,
                9,
            );
        }
    }

    fn on_key(&mut self, app_state: &mut AppState, c: char) {
        if self.confirmation_dialog {
            match c {
                'y' => {
                    self.confirmation_dialog = false;
                    let index = self.table_state.selected().unwrap();
                    let selected = app_state.get_owned_constitutions()[index].clone();
                    if let Some(sidechain_features) = selected.unblinded_output.features.sidechain_features {
                        let (tx, _rx) = watch::channel(UiTransactionSendStatus::Initiated);
                        if let Err(e) = Handle::current()
                            .block_on(app_state.reclaim_constitution(sidechain_features.contract_id, tx))
                        {
                            self.error_message = Some(format!(
                                "Error reclaiming constitution:\n{}\nPress Enter to continue.",
                                e
                            ));
                        }
                    }
                },
                'n' => {
                    self.confirmation_dialog = false;
                },
                _ => {},
            }
        } else if c == 'd' {
            if let Some(selected) = self.table_state.selected() {
                let selected = &app_state.get_owned_constitutions()[selected];
                if self.is_reclaimable(app_state, selected) {
                    self.confirmation_dialog = true;
                }
            }
        } else {
        }
    }

    fn on_esc(&mut self, _app_state: &mut AppState) {
        if self.confirmation_dialog {
            self.confirmation_dialog = false;
        }
    }

    fn on_up(&mut self, _app_state: &mut AppState) {
        if !self.confirmation_dialog {
            let index = self.table_state.selected().unwrap_or_default();
            if index == 0 {
                self.table_state.select(None);
            } else {
                self.table_state.select(Some(index - 1));
            }
        }
    }

    fn on_down(&mut self, app_state: &mut AppState) {
        if !self.confirmation_dialog {
            let index = self.table_state.selected().map(|s| s + 1).unwrap_or_default();
            let constitutions = app_state.get_owned_constitutions();
            if index > constitutions.len().saturating_sub(1) {
                self.table_state.select(None);
            } else {
                self.table_state.select(Some(index));
            }
        }
    }
}
