use tui::{
    backend::Backend,
    layout::Rect,
    style::{Color, Style},
    text::{Span, Spans},
    Frame,
};

use crate::ui::state::AppState;

pub trait Component<B: Backend> {
    fn draw(&mut self, f: &mut Frame<B>, area: Rect, app_state: &AppState);

    fn on_key(&mut self, _app_state: &mut AppState, _c: char) {}

    fn on_up(&mut self, _app_state: &mut AppState) {}

    fn on_down(&mut self, _app_state: &mut AppState) {}

    fn on_esc(&mut self, _app_state: &mut AppState) {}
    fn on_backspace(&mut self, _app_state: &mut AppState) {}
    fn on_tick(&mut self, _app_state: &mut AppState) {}

    // Create custom title based on data in AppState.
    fn format_title(&self, title: &str, _app_state: &AppState) -> Spans {
        Spans::from(Span::styled(title.to_owned(), Style::default().fg(Color::White)))
    }
}
