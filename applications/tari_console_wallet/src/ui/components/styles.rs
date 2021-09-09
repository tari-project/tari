use tui::style::{Color, Modifier, Style};

pub fn header_row() -> Style {
    Style::default().fg(Color::Magenta)
}

pub fn highlight() -> Style {
    Style::default().add_modifier(Modifier::BOLD).fg(Color::Magenta)
}
