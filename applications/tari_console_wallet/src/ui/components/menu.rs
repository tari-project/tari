use crate::ui::{components::Component, state::AppState};
use tari_app_utilities::consts;
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Span, Spans},
    widgets::{Block, Paragraph},
    Frame,
};

pub struct Menu {}

impl Menu {
    pub fn new() -> Self {
        Self {}
    }
}

impl<B: Backend> Component<B> for Menu {
    fn draw(&mut self, f: &mut Frame<B>, area: Rect, _app_state: &AppState)
    where B: Backend {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(
                [
                    Constraint::Ratio(1, 5),
                    Constraint::Ratio(3, 5),
                    Constraint::Ratio(1, 5),
                ]
                .as_ref(),
            )
            .split(area);

        let version = Spans::from(vec![
            Span::styled(" Version: ", Style::default().fg(Color::White)),
            Span::styled(consts::APP_VERSION_NUMBER, Style::default().fg(Color::Magenta)),
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

        let paragraph = Paragraph::new(version).block(Block::default());
        f.render_widget(paragraph, columns[0]);
        let paragraph = Paragraph::new(tabs).block(Block::default());
        f.render_widget(paragraph, columns[1]);
        let paragraph = Paragraph::new(quit).block(Block::default());
        f.render_widget(paragraph, columns[2]);
    }
}
