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
    // casting here is okay as this only is only draw widths and heights.
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::too_many_lines)]
    fn draw(&mut self, f: &mut Frame<B>, area: Rect, app_state: &AppState)
    where B: Backend {
        let title = Spans::from(vec![Span::styled(
            " Base Node Status  -     ",
            Style::default().fg(Color::White),
        )]);

        let scanned_height = app_state.get_wallet_scanned_height();
        let tip_height = app_state.get_wallet_tip_height();
        let mut chain_info = vec![
            Span::styled("Chain Tip:", Style::default().fg(Color::Magenta)),
            Span::raw(" "),
            Span::styled(
                format!("#{}({})", tip_height, scanned_height),
                Style::default().fg(Color::Green),
            ),
            Span::raw("   "),
        ];

        let latency = app_state.get_base_node_latency().unwrap_or_default().as_millis();
        let latency_color = match latency {
            0 => Color::Gray, // offline? default duration is 0
            1..=800 => Color::Green,
            801..=1200 => Color::Yellow,
            _ => Color::Red,
        };

        let mut latency_span = vec![
            Span::styled("Latency", Style::default().fg(Color::White)),
            Span::raw(" "),
            Span::styled(latency.to_string(), Style::default().fg(latency_color)),
            Span::styled(" ms", Style::default().fg(Color::DarkGray)),
        ];
        chain_info.append(&mut latency_span);

        let chain_info = Spans::from(chain_info);

        let base_node_id = Spans::from(vec![
            Span::styled(" Connected Base Node ID: ", Style::default().fg(Color::Magenta)),
            Span::styled(app_state.get_http_node_url(), Style::default().fg(Color::White)),
            Span::styled(" ", Style::default().fg(Color::White)),
        ]);

        let chunks = Layout::default()
            .constraints([Constraint::Length(1), Constraint::Length(1)].as_ref())
            .split(area);

        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(
                [
                    Constraint::Ratio(title.width() as u32, u32::from(MAX_WIDTH)),
                    Constraint::Ratio(
                        u32::from(MAX_WIDTH.saturating_sub((title.width() + base_node_id.width()) as u16)),
                        u32::from(MAX_WIDTH),
                    ),
                    Constraint::Ratio(base_node_id.width() as u32, u32::from(MAX_WIDTH)),
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
