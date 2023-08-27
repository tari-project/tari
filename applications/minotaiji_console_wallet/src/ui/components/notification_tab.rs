// Copyright 2022 The Taiji Project
// SPDX-License-Identifier: BSD-3-Clause

// This tab will show all the notifications. With timestamp automatically added.
// The tab title will turn green with notifications count (when there are any).
// The notifications lives as long as the app. Once the app is closed, the notifications
// are cleared.
// Currently notifications are only added from the wallet_event_monitor which has
// add_notification method.

use tokio::runtime::Handle;
use tui::{
    backend::Backend,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::ui::{components::Component, state::AppState};

pub struct NotificationTab {}

impl NotificationTab {
    pub fn new() -> Self {
        Self {}
    }

    fn draw_notifications<B>(&mut self, f: &mut Frame<B>, area: Rect, app_state: &AppState)
    where B: Backend {
        let span_vec = vec![
            Span::raw("Press "),
            Span::styled("C", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" to clear notifications"),
        ];

        let instructions = Paragraph::new(Spans::from(span_vec)).wrap(Wrap { trim: false });

        let block = Block::default().borders(Borders::ALL).title(Span::styled(
            "Notifications",
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ));
        f.render_widget(block, area);
        let notifications_area = Layout::default()
            .constraints([Constraint::Length(1), Constraint::Min(42)].as_ref())
            .margin(1)
            .split(area);
        let text = app_state
            .get_notifications()
            .iter()
            .rev()
            .map(|(time, line)| {
                Spans::from(vec![
                    Span::styled(
                        time.format("%Y-%m-%d %H:%M:%S ").to_string(),
                        Style::default().fg(Color::LightGreen),
                    ),
                    Span::raw(line),
                ])
            })
            .collect::<Vec<_>>();
        let paragraph = Paragraph::new(text).wrap(Wrap { trim: true });

        f.render_widget(instructions, notifications_area[0]);
        f.render_widget(paragraph, notifications_area[1]);
    }
}

impl<B: Backend> Component<B> for NotificationTab {
    fn draw(&mut self, f: &mut Frame<B>, area: Rect, app_state: &AppState) {
        let areas = Layout::default()
            .constraints([Constraint::Min(42)].as_ref())
            .split(area);
        self.draw_notifications(f, areas[0], app_state);
    }

    fn on_tick(&mut self, app_state: &mut AppState) {
        // Constantly read the messages when in this tab.
        Handle::current().block_on(app_state.mark_notifications_as_read());
    }

    fn format_title(&self, title: &str, app_state: &AppState) -> Spans {
        // Create custom title based on notifications count.
        if app_state.unread_notifications_count() > 0 {
            Spans::from(Span::styled(
                format!("{}({})", title, app_state.unread_notifications_count()),
                Style::default().fg(Color::LightGreen),
            ))
        } else {
            Spans::from(Span::styled(title.to_owned(), Style::default().fg(Color::White)))
        }
    }

    fn on_key(&mut self, app_state: &mut AppState, c: char) {
        if c == 'c' {
            Handle::current().block_on(app_state.clear_notifications());
        }
    }
}
