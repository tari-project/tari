// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use ratatui::{
    backend::Backend,
    layout::{Constraint, Rect},
    widgets::{Block, Borders, Row, Table, TableState},
    Frame,
};

use crate::ui::{
    components::{styles, Component},
    state::AppState,
};

pub struct EventsComponent {
    table_state: TableState,
}

impl EventsComponent {
    pub fn new() -> Self {
        Self {
            table_state: TableState::default(),
        }
    }
}

impl<B: Backend> Component<B> for EventsComponent {
    fn draw(&mut self, f: &mut Frame<B>, area: Rect, app_state: &AppState) {
        let events = app_state.get_all_events();
        let rows: Vec<_> = events
            .iter()
            .map(|e| Row::new(vec![e.event_type.as_str(), e.desc.as_str()]))
            .collect();
        let table = Table::new(rows)
            .header(Row::new(vec!["Type", "Desc"]).style(styles::header_row()))
            .block(Block::default().title("Events").borders(Borders::ALL))
            .widths(&[Constraint::Length(20), Constraint::Length(120)])
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
        let events = app_state.get_all_events();
        if index > events.len() - 1 {
            self.table_state.select(None);
        } else {
            self.table_state.select(Some(index));
        }
    }
}
