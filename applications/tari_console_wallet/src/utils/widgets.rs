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
    style::Style,
    text::Span,
    widgets::{List, ListItem, ListState, Paragraph},
    Frame,
};

/// A Tui-rs list with columns
pub struct MultiColumnList<'a, T>
where T: Into<Vec<ListItem<'a>>>
{
    columns: Vec<ListColumn<'a, T>>,
    highlight_style: Option<Style>,
    heading_style: Option<Style>,
    highlight_symbol: Option<&'a str>,
    max_width: Option<u16>,
}

impl<'a, T> MultiColumnList<'a, T>
where T: Into<Vec<ListItem<'a>>>
{
    pub fn new() -> Self {
        Self {
            columns: vec![],
            highlight_style: None,
            heading_style: None,
            highlight_symbol: None,
            max_width: None,
        }
    }

    pub fn add_column(mut self, heading: Option<&'a str>, width: Option<u16>, items: T) -> Self {
        self.columns.push(ListColumn { heading, width, items });
        self
    }

    pub fn highlight_style(mut self, style: Style) -> Self {
        self.highlight_style = Some(style);
        self
    }

    pub fn heading_style(mut self, style: Style) -> Self {
        self.heading_style = Some(style);
        self
    }

    pub fn highlight_symbol(mut self, symbol: &'a str) -> Self {
        self.highlight_symbol = Some(symbol);
        self
    }

    pub fn max_width(mut self, max_width: u16) -> Self {
        self.max_width = Some(max_width);
        self
    }

    pub fn render<B: Backend>(mut self, f: &mut Frame<B>, area: Rect, state: &mut ListState) {
        let mut constraints = Vec::new();
        // This accounts for the box border
        constraints.push(Constraint::Length(1));
        let mut sum_width = 0;
        for i in 0..self.columns.len() - 1 {
            if let Some(w) = self.columns[i].width {
                constraints.push(Constraint::Length(w));
                sum_width += w;
            } else {
                constraints.push(Constraint::Length(self.columns[i].heading.unwrap_or(" ").len() as u16));
            }
        }

        if let Some(w) = self.max_width {
            if w - 2 > sum_width {
                constraints.push(Constraint::Length(w - sum_width - 2));
            } else {
                constraints.push(Constraint::Min(0));
            }
        } else {
            constraints.push(Constraint::Min(0));
        }
        // This accounts for the other box border
        constraints.push(Constraint::Length(1));
        let column_areas = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(constraints.as_ref())
            .margin(1)
            .split(area);

        for c in 0..self.columns.len() {
            let column = self.columns.remove(0);
            let list_area = match column.heading {
                None => column_areas[c + 1],
                Some(heading) => {
                    let padded_heading = if c == 0 {
                        format!("  {}", heading)
                    } else {
                        heading.to_string()
                    };

                    let column_heading_list_area = Layout::default()
                        .constraints([Constraint::Length(1), Constraint::Min(0)].as_ref())
                        .split(column_areas[c + 1]);
                    let span = match self.heading_style {
                        None => Span::raw(padded_heading.as_str()),
                        Some(s) => Span::styled(padded_heading.as_str(), s),
                    };

                    f.render_widget(Paragraph::new(span), column_heading_list_area[0]);
                    column_heading_list_area[1]
                },
            };
            if c == 0 {
                let column_list = List::new(column.items)
                    .highlight_style(self.highlight_style.unwrap_or_default())
                    .highlight_symbol(self.highlight_symbol.unwrap_or("> "));
                f.render_stateful_widget(column_list, list_area, state);
            } else {
                let column_list = List::new(column.items).highlight_style(self.highlight_style.unwrap_or_default());
                f.render_stateful_widget(column_list, list_area, state);
            }
        }
    }
}
struct ListColumn<'a, T>
where T: Into<Vec<ListItem<'a>>>
{
    pub heading: Option<&'a str>,
    pub width: Option<u16>,
    pub items: T,
}
