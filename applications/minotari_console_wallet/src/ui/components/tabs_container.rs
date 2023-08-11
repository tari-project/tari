// Copyright 2020. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use tui::{
    backend::Backend,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders, Tabs},
    Frame,
};

use crate::ui::{components::Component, state::AppState};

pub struct TabsContainer<B: Backend> {
    title: String,
    tabs: Vec<Box<dyn Component<B>>>,
    titles: Vec<String>,
    index: usize,
}

impl<B: Backend> TabsContainer<B> {
    pub fn new(title: String) -> Self {
        Self {
            title,
            tabs: vec![],
            titles: vec![],
            index: 0,
        }
    }

    pub fn add(mut self, title: String, tab: Box<dyn Component<B>>) -> Self {
        self.tabs.push(tab);
        self.titles.push(title);
        self
    }

    pub fn next(&mut self) {
        self.index = (self.index + 1) % self.titles.len();
    }

    pub fn previous(&mut self) {
        if self.index > 0 {
            self.index -= 1;
        } else {
            self.index = self.titles.len() - 1;
        }
    }

    pub fn draw_titles(&self, f: &mut Frame<B>, area: Rect, app_state: &AppState) {
        let titles = self
            .titles
            .iter()
            .enumerate()
            .map(|(i, title)| self.tabs[i].format_title(title, app_state))
            .collect();
        let tabs = Tabs::new(titles)
            .block(Block::default().borders(Borders::ALL).title(Span::styled(
                &self.title,
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            )))
            .highlight_style(Style::default().fg(Color::Magenta))
            .select(self.index);
        f.render_widget(tabs, area);
    }

    pub fn draw_content(&mut self, f: &mut Frame<B>, area: Rect, app_state: &mut AppState) {
        self.tabs[self.index].draw(f, area, app_state);
    }
}

impl<B: Backend> Component<B> for TabsContainer<B> {
    fn draw(&mut self, _: &mut Frame<B>, _: Rect, _: &AppState) {
        // Use draw_titles and draw_content instead,
        unimplemented!()
    }

    fn on_key(&mut self, app_state: &mut AppState, c: char) {
        self.tabs[self.index].on_key(app_state, c);
    }

    fn on_up(&mut self, app_state: &mut AppState) {
        self.tabs[self.index].on_up(app_state);
    }

    fn on_down(&mut self, app_state: &mut AppState) {
        self.tabs[self.index].on_down(app_state);
    }

    fn on_esc(&mut self, app_state: &mut AppState) {
        self.tabs[self.index].on_esc(app_state);
    }

    fn on_backspace(&mut self, app_state: &mut AppState) {
        self.tabs[self.index].on_backspace(app_state);
    }

    fn on_tick(&mut self, app_state: &mut AppState) {
        self.tabs[self.index].on_tick(app_state);
    }
}
