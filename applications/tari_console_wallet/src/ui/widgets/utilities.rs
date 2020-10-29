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
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

/// Help function to create a centered rectangle with absolute dimensions
pub fn centered_rect_absolute(width: u16, height: u16, r: Rect) -> Rect {
    let vertical_pad = r.height.saturating_sub(height) / 2;
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(vertical_pad),
                Constraint::Length(height.min(r.height)),
                Constraint::Min(1),
            ]
            .as_ref(),
        )
        .split(r);

    let horizontal_pad = r.width.saturating_sub(width) / 2;
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Length(horizontal_pad),
                Constraint::Length(width.min(r.width)),
                Constraint::Min(1),
            ]
            .as_ref(),
        )
        .split(popup_layout[1])[1]
}

pub fn draw_dialog<B>(
    f: &mut Frame<B>,
    full_area: Rect,
    title: String,
    message: String,
    color: Color,
    width: u16,
    height: u16,
) where
    B: Backend,
{
    let popup_area = centered_rect_absolute(width.min(full_area.width), height.min(full_area.height), full_area);

    f.render_widget(Clear, popup_area);

    let block = Block::default().borders(Borders::ALL).title(Span::styled(
        title.as_str(),
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    ));
    f.render_widget(block, popup_area);

    let lines = message.as_str().lines();

    let mut spans = Vec::new();
    for l in lines {
        spans.push(Spans::from(Span::styled(
            l,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        )));
    }
    let center_area = centered_rect_absolute(
        width.min(full_area.width).saturating_sub(2),
        ((spans.len()) as u16).max(2).min(full_area.height),
        full_area,
    );

    let text = Paragraph::new(spans)
        .style(Style::default().fg(color))
        .block(Block::default())
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });

    f.render_widget(text, center_area);
}
