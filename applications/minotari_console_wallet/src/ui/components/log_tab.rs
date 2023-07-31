// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::fs;

use regex::Regex;
use tui::{
    backend::Backend,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::ui::{components::Component, state::AppState};

pub struct LogTab {
    scroll: u16,
    re: Regex,
}

impl LogTab {
    pub fn new() -> Self {
        Self { scroll: 1,
        re : Regex::new(
                r"(?P<timestamp>\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}.\d*) \[(?P<target>[^\]]*)\] (?P<level>INFO|WARN|DEBUG|ERROR|TRACE)\s*(?P<message> .*)",
            )
            .unwrap()
        }
    }

    // Format the log line nicely. If it cannot be parsed then return raw line
    fn format_line(&self, line: String) -> Spans {
        match self.re.captures(line.as_str()) {
            Some(caps) => Spans::from(vec![
                Span::styled(caps["timestamp"].to_string(), Style::default().fg(Color::LightGreen)),
                Span::raw(" ["),
                Span::styled(caps["target"].to_string(), Style::default().fg(Color::LightMagenta)),
                Span::raw("] "),
                Span::styled(
                    caps["level"].to_string(),
                    Style::default().fg(match &caps["level"] {
                        "ERROR" => Color::LightRed,
                        "WARN" => Color::LightYellow,
                        _ => Color::LightMagenta,
                    }),
                ),
                Span::raw(caps["message"].to_string()),
            ]),
            // In case the line is not well formatted, just print as it is
            None => Spans::from(vec![Span::raw(line)]),
        }
    }

    fn draw_logs<B>(&mut self, f: &mut Frame<B>, area: Rect, _app_state: &AppState)
    where B: Backend {
        // First render the border and calculate the inner area
        let block = Block::default().borders(Borders::ALL).title(Span::styled(
            "StdOut log",
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ));
        f.render_widget(block, area);
        let log_area = Layout::default()
            .constraints([Constraint::Min(42)].as_ref())
            .margin(1)
            .split(area);
        // Read the log file
        let content = match fs::read_to_string("log/wallet/stdout.log") {
            Ok(content) => content,
            Err(err) => format!("Error reading log: {}", err),
        };
        // Convert the content into Spans
        let mut text: Vec<Spans> = content.lines().map(|line| self.format_line(line.to_string())).collect();
        // We want newest at the top
        text.reverse();
        // Render the Paragraph
        let paragraph = Paragraph::new(text.clone())
            .wrap(Wrap { trim: true })
            .scroll((self.scroll, 0));
        f.render_widget(paragraph, log_area[0]);
    }
}

impl<B: Backend> Component<B> for LogTab {
    fn draw(&mut self, f: &mut Frame<B>, area: Rect, app_state: &AppState) {
        let areas = Layout::default()
            .constraints([Constraint::Min(42)].as_ref())
            .split(area);

        self.draw_logs(f, areas[0], app_state);
    }

    fn on_key(&mut self, _app_state: &mut AppState, _c: char) {}

    fn on_up(&mut self, _app_state: &mut AppState) {
        if self.scroll > 1 {
            self.scroll -= 1;
        }
    }

    fn on_down(&mut self, _app_state: &mut AppState) {
        self.scroll += 1;
    }

    fn on_esc(&mut self, _: &mut AppState) {}

    fn on_backspace(&mut self, _app_state: &mut AppState) {}
}
