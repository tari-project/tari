// This tab will show all the notifications. With timestamp automatically added.
// The tab title will turn green with notifications count (when there are any).
// The notifications lives as long as the app. Once the app is closed, the notifications
// are cleared.
// Currently notifications are only added from the wallet_event_monitor which has
// add_notification method.
// TODO: auto delete old notifications. #LOGGED
// TODO: add interaction with the notifications, e.g. if I have a pending transaction
//       notification, the UI should go there if I click on it. #LOGGED

use tari_comms::runtime::Handle;
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
        let block = Block::default().borders(Borders::ALL).title(Span::styled(
            "Notifications",
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ));
        f.render_widget(block, area);
        let notifications_area = Layout::default()
            .constraints([Constraint::Min(42)].as_ref())
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
        f.render_widget(paragraph, notifications_area[0]);
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
        match app_state.unread_notifications_count() > 0 {
            true => Spans::from(Span::styled(
                format!("{}({})", title, app_state.unread_notifications_count()),
                Style::default().fg(Color::LightGreen),
            )),
            false => Spans::from(Span::styled(title.to_owned(), Style::default().fg(Color::White))),
        }
    }
}
