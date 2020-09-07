use crate::ui::{components::Component, state::AppState};
use tari_core::transactions::tari_amount::MicroTari;
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

pub struct Balance {}

impl Balance {
    pub fn new() -> Self {
        Self {}
    }
}

impl<B: Backend> Component<B> for Balance {
    fn draw(&mut self, f: &mut Frame<B>, area: Rect, _app_state: &AppState)
    where B: Backend {
        // This is a hack to produce only a top margin and not a bottom margin
        let block_title_body = Layout::default()
            .constraints([Constraint::Length(1), Constraint::Length(1)].as_ref())
            .split(area);

        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(
                [
                    Constraint::Ratio(1, 2),
                    Constraint::Ratio(1, 4),
                    Constraint::Ratio(1, 4),
                ]
                .as_ref(),
            )
            .horizontal_margin(1)
            .split(block_title_body[1]);

        let block = Block::default().borders(Borders::ALL).title(Span::styled(
            "Balance",
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ));
        f.render_widget(block, area);

        let available_balance = Spans::from(vec![
            Span::styled("Available:", Style::default().fg(Color::Magenta)),
            Span::raw(" "),
            Span::raw(format!("{}", MicroTari::from(1234567000))),
            Span::raw(format!(" (Time Locked: {})", MicroTari::from(20000000))),
        ]);
        let incoming_balance = Spans::from(vec![
            Span::styled("Pending Incoming:", Style::default().fg(Color::Magenta)),
            Span::raw(" "),
            Span::raw(format!("{}", MicroTari::from(12345670500))),
        ]);
        let outgoing_balance = Spans::from(vec![
            Span::styled("Pending Outgoing:", Style::default().fg(Color::Magenta)),
            Span::raw(" "),
            Span::raw(format!("{}", MicroTari::from(98754))),
        ]);

        let paragraph1 = Paragraph::new(available_balance).block(Block::default());
        f.render_widget(paragraph1, columns[0]);
        let paragraph2 = Paragraph::new(incoming_balance).block(Block::default());
        f.render_widget(paragraph2, columns[1]);
        let paragraph3 = Paragraph::new(outgoing_balance).block(Block::default());
        f.render_widget(paragraph3, columns[2]);
    }
}
