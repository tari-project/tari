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

use tari_wallet::connectivity_service::{OnlineStatus, WalletConnectivityInterface};
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::ui::{components::Component, state::AppState, MAX_WIDTH};

pub struct BaseNode {}

impl BaseNode {
    pub fn new() -> Self {
        Self {}
    }
}

impl<B: Backend> Component<B> for BaseNode {
    fn draw(&mut self, f: &mut Frame<B>, area: Rect, app_state: &AppState)
    where B: Backend {
        let title = Spans::from(vec![Span::styled(
            " Base Node Status  -  ",
            Style::default().fg(Color::White),
        )]);

        let current_online_status = app_state.get_wallet_connectivity().get_connectivity_status();
        let mut base_node_id_color = Color::White;
        let chain_info = match current_online_status {
            OnlineStatus::Connecting => Spans::from(vec![
                Span::styled("Chain Tip:", Style::default().fg(Color::Magenta)),
                Span::raw(" "),
                Span::styled("Connecting...", Style::default().fg(Color::Reset)),
            ]),
            OnlineStatus::Offline => Spans::from(vec![
                Span::styled("Chain Tip:", Style::default().fg(Color::Magenta)),
                Span::raw(" "),
                Span::styled("Offline", Style::default().fg(Color::Red)),
            ]),
            OnlineStatus::Online => {
                let base_node_state = app_state.get_base_node_state();
                if let Some(ref metadata) = base_node_state.chain_metadata {
                    let tip = metadata.height_of_longest_chain();

                    let synced = base_node_state.is_synced.unwrap_or_default();
                    let (tip_color, sync_text) = if synced {
                        (
                            {
                                base_node_id_color = Color::Green;
                                base_node_id_color
                            },
                            "Synced.",
                        )
                    } else {
                        (
                            {
                                base_node_id_color = Color::Yellow;
                                base_node_id_color
                            },
                            "Syncing...",
                        )
                    };

                    let latency = base_node_state.latency.unwrap_or_default().as_millis();
                    let latency_color = match latency {
                        0 => Color::Gray, // offline? default duration is 0
                        1..=800 => Color::Green,
                        801..=1200 => Color::Yellow,
                        _ => Color::Red,
                    };

                    let tip_info = vec![
                        Span::styled("Chain Tip:", Style::default().fg(Color::Magenta)),
                        Span::raw(" "),
                        Span::styled(format!("#{}", tip), Style::default().fg(tip_color)),
                        Span::raw("  "),
                        Span::styled(sync_text.to_string(), Style::default().fg(Color::White)),
                        Span::raw("  "),
                        Span::styled("Latency", Style::default().fg(Color::White)),
                        Span::raw(" "),
                        Span::styled(latency.to_string(), Style::default().fg(latency_color)),
                        Span::styled(" ms", Style::default().fg(Color::DarkGray)),
                    ];

                    Spans::from(tip_info)
                } else {
                    Spans::from(vec![
                        Span::styled("Chain Tip:", Style::default().fg(Color::Magenta)),
                        Span::raw(" "),
                        Span::styled("Waiting for data...", Style::default().fg(Color::DarkGray)),
                    ])
                }
            },
        };

        let base_node_id = Spans::from(vec![
            Span::styled(" Connected Base Node ID: ", Style::default().fg(Color::Magenta)),
            Span::styled(
                format!("{}", app_state.get_selected_base_node().node_id.clone()),
                Style::default().fg(base_node_id_color),
            ),
            Span::styled(" ", Style::default().fg(Color::White)),
        ]);

        let chunks = Layout::default()
            .constraints([Constraint::Length(1), Constraint::Length(1)].as_ref())
            .split(area);

        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(
                [
                    Constraint::Ratio(title.width() as u32, MAX_WIDTH as u32),
                    Constraint::Ratio(
                        MAX_WIDTH.saturating_sub((title.width() + base_node_id.width()) as u16) as u32,
                        MAX_WIDTH as u32,
                    ),
                    Constraint::Ratio(base_node_id.width() as u32, MAX_WIDTH as u32),
                ]
                .as_ref(),
            )
            .split(chunks[0]);

        let paragraph = Paragraph::new(title).block(Block::default());
        f.render_widget(paragraph, columns[0]);
        let paragraph = Paragraph::new(chain_info).block(Block::default());
        f.render_widget(paragraph, columns[1]);
        let paragraph = Paragraph::new(base_node_id).block(Block::default());
        f.render_widget(paragraph, columns[2]);

        let divider = Block::default().borders(Borders::BOTTOM);
        f.render_widget(divider, chunks[1]);
    }
}
