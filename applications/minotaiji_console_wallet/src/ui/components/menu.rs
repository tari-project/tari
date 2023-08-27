// Copyright 2022 The Taiji Project
// SPDX-License-Identifier: BSD-3-Clause

use minotaiji_app_utilities::consts;
use tokio::runtime::Handle;
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Span, Spans},
    widgets::{Block, Paragraph},
    Frame,
};

use crate::ui::{components::Component, state::AppState};

pub struct Menu {}

impl Menu {
    pub fn new() -> Self {
        Self {}
    }
}

impl<B: Backend> Component<B> for Menu {
    fn draw(&mut self, f: &mut Frame<B>, area: Rect, app_state: &AppState)
    where B: Backend {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(
                [
                    Constraint::Ratio(1, 5),
                    Constraint::Ratio(1, 5),
                    Constraint::Ratio(2, 5),
                    Constraint::Ratio(1, 5),
                ]
                .as_ref(),
            )
            .split(area);

        let version = Spans::from(vec![
            Span::styled(" Version: ", Style::default().fg(Color::White)),
            Span::styled(consts::APP_VERSION_NUMBER, Style::default().fg(Color::Magenta)),
            Span::raw(" "),
        ]);

        let network = Spans::from(vec![
            Span::styled(" Network: ", Style::default().fg(Color::White)),
            Span::styled(
                Handle::current().block_on(app_state.get_network()).to_string(),
                Style::default().fg(Color::LightGreen),
            ),
            Span::raw(" "),
        ]);

        let tabs = Spans::from(vec![
            Span::styled("LeftArrow: ", Style::default().fg(Color::White)),
            Span::styled("Previous Tab ", Style::default().fg(Color::Magenta)),
            Span::raw(" "),
            Span::styled("Tab/RightArrow: ", Style::default().fg(Color::White)),
            Span::styled("Next Tab ", Style::default().fg(Color::Magenta)),
        ]);

        let quit = Spans::from(vec![
            Span::styled("          F10/Ctrl-Q: ", Style::default().fg(Color::White)),
            Span::styled("Quit    ", Style::default().fg(Color::Magenta)),
        ]);

        let paragraph = Paragraph::new(network).block(Block::default());
        f.render_widget(paragraph, columns[0]);
        let paragraph = Paragraph::new(version).block(Block::default());
        f.render_widget(paragraph, columns[1]);
        let paragraph = Paragraph::new(tabs).block(Block::default());
        f.render_widget(paragraph, columns[2]);
        let paragraph = Paragraph::new(quit).block(Block::default());
        f.render_widget(paragraph, columns[3]);
    }
}
