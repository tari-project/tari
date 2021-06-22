use tui::backend::Backend;
use crate::ui::components::{Component, styles};
use tui::Frame;
use tui::layout::{Rect, Constraint};
use crate::ui::state::AppState;
use tui::widgets::{Row, Table, Block, Borders, TableState};

pub struct EventsComponent {
    table_state: TableState,
}


impl EventsComponent {
    pub fn new() -> Self{
        Self {
            table_state: TableState::default(),
        }
    }
}

impl<B:Backend> Component<B> for EventsComponent {
    fn draw(&mut self, f: &mut Frame<B>, area: Rect, app_state: &AppState) {
        let events = app_state.get_all_events();
        let rows :Vec<_>= events.iter().map(|e| Row::new(vec![e.event_type.as_str(), e.desc.as_str()])).collect();
        let table = Table::new(rows)
            .header(Row::new(vec!["Type", "Desc"]).style(styles::header_row())).block(Block::default().title("Events").borders(Borders::ALL)).widths(&[Constraint::Length(20), Constraint::Length(120)]).highlight_style(styles::highlight()).highlight_symbol(">>");
        f.render_stateful_widget(table, area, &mut self.table_state)
    }}
