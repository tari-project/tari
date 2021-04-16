use crate::ui::{components::Component, state::AppState};
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
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
            .constraints([Constraint::Ratio(4, 5), Constraint::Ratio(1, 5)].as_ref())
            .split(area);

        let others = Spans::from(vec![
            Span::styled("LeftArrow", Style::default().fg(Color::Green)),
            Span::styled(":", Style::default().fg(Color::White)),
            Span::styled(
                " PrevTab ",
                Style::default()
                    .fg(Color::Magenta)
                    .bg(Color::LightGreen)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled("Tab/RightArrow", Style::default().fg(Color::Green)),
            Span::styled(":", Style::default().fg(Color::White)),
            Span::styled(
                " NextTab ",
                Style::default()
                    .fg(Color::Magenta)
                    .bg(Color::LightGreen)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);
        let quit = Spans::from(vec![
            Span::styled("F10/Ctrl-Q", Style::default().fg(Color::Green)),
            Span::styled(":", Style::default().fg(Color::White)),
            Span::styled(
                " Quit    ",
                Style::default()
                    .fg(Color::Magenta)
                    .bg(Color::LightGreen)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);

        let paragraph1 = Paragraph::new(others).block(Block::default());
        f.render_widget(paragraph1, columns[0]);
        let paragraph2 = Paragraph::new(quit).block(Block::default());
        f.render_widget(paragraph2, columns[1]);
    }
}
