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

use tui::widgets::ListState;
#[derive(Debug)]
pub struct WindowedListState {
    offset: usize,
    start: usize,
    end: usize,
    selected: Option<usize>,
    num_items: usize,
}

impl WindowedListState {
    pub fn new() -> Self {
        Self {
            offset: 0,
            start: 0,
            end: 0,
            selected: None,
            num_items: 0,
        }
    }

    pub fn update_list_state(&mut self, height: usize) -> ListState {
        // Update the offset based on current offset, selected value and height
        self.start = self.offset;
        let view_height = height.min(self.num_items);
        self.end = self.offset + view_height;
        let mut list_state = ListState::default();
        if let Some(selected) = self.selected {
            if selected >= self.end {
                let diff = selected - self.end + 1;
                self.start += diff;
                self.end += diff;
            } else if selected < self.start {
                let diff = self.start - selected;
                self.start -= diff;
                self.end -= diff;
            } else {
                // dont care
            }
            self.offset = self.start;
            list_state.select(Some(selected - self.start));
        }
        // If the window was resized make sure we are within bounds of the list.
        if self.end > self.num_items {
            let diff = self.end - self.num_items;
            self.start -= diff;
            self.end -= diff;
            if let Some(selected) = self.selected {
                list_state.select(Some(selected - self.start));
            }
            self.offset = self.start;
        }
        list_state
    }

    pub fn get_start_end(&self) -> (usize, usize) {
        (self.start, self.end)
    }

    pub fn next(&mut self) {
        if self.num_items == 0 {
            self.selected = None;
        } else {
            let i = match self.selected {
                Some(i) => {
                    if i >= self.num_items - 1 {
                        0
                    } else {
                        i + 1
                    }
                },
                None => 0,
            };
            self.selected = Some(i);
        }
    }

    pub fn previous(&mut self) {
        if self.num_items == 0 {
            self.selected = None;
        } else {
            let i = match self.selected {
                Some(i) => {
                    if i == 0 {
                        self.num_items - 1
                    } else {
                        i - 1
                    }
                },
                None => 0,
            };
            self.selected = Some(i);
        }
    }

    pub fn _unselect(&mut self) {
        self.selected = None;
    }

    pub fn select(&mut self, idx: Option<usize>) {
        self.selected = idx;
    }

    pub fn select_first(&mut self) {
        if self.num_items == 0 {
            self.selected = None;
        } else {
            self.selected = Some(0);
        }
    }

    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    pub fn set_num_items(&mut self, num_items: usize) {
        if num_items < self.num_items {
            self.offset = self.offset.saturating_sub(self.num_items - num_items);
        }
        self.num_items = num_items;
        if num_items > 0 {
            if let Some(p) = self.selected {
                if p > num_items - 1 {
                    self.selected = Some(num_items - 1);
                }
            }
        } else {
            self.selected = None;
        }
    }
}

#[cfg(test)]
mod test {
    use std::convert::TryFrom;

    use crate::ui::widgets::WindowedListState;

    #[test]
    fn test_zero_items() {
        let mut list_state = WindowedListState::new();
        list_state.previous();
        assert_eq!(list_state.selected(), None);
        list_state.next();
        assert_eq!(list_state.selected(), None);
        list_state.update_list_state(5);
        assert_eq!(list_state.selected(), None);
        assert_eq!(list_state.get_start_end(), (0, 0));

        list_state.set_num_items(5);
        list_state.set_num_items(0);
        list_state.previous();
        assert_eq!(list_state.selected(), None);
        list_state.next();
        assert_eq!(list_state.selected(), None);
        list_state.update_list_state(5);
        assert_eq!(list_state.selected(), None);
        assert_eq!(list_state.get_start_end(), (0, 0));
    }

    #[test]
    fn test_select_first() {
        let mut list_state = WindowedListState::new();
        list_state.set_num_items(0);
        list_state.select_first();
        list_state.update_list_state(5);
        assert_eq!(list_state.selected(), None);
    }

    #[test]
    fn test_list_offset_update() {
        let slist = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let mut list_state = WindowedListState::new();
        list_state.set_num_items(slist.len());
        let height = 4;
        for i in 0..6 {
            list_state.next();
            let state = list_state.update_list_state(height);
            assert_eq!(state.selected(), Some(i.min(height - 1)));
        }
        list_state.update_list_state(height);
        let window = list_state.get_start_end();
        assert_eq!(slist[window.0..window.1], [2, 3, 4, 5]);

        for i in (0..5).rev() {
            list_state.previous();
            let state = list_state.update_list_state(height);
            assert_eq!(state.selected(), Some(usize::try_from((i - 2i32).max(0)).unwrap()));
        }
        list_state.update_list_state(height);
        let window = list_state.get_start_end();
        assert_eq!(slist[window.0..window.1], [0, 1, 2, 3]);

        list_state.previous();
        let state = list_state.update_list_state(height);
        assert_eq!(state.selected(), Some(height - 1));
        let window = list_state.get_start_end();
        assert_eq!(slist[window.0..window.1], [7, 8, 9, 10]);
    }

    #[test]
    fn test_removing_last_selected_items() {
        let mut list_state = WindowedListState::new();
        list_state.set_num_items(11);
        for _ in 0..11 {
            list_state.next();
            let _state = list_state.update_list_state(4);
        }

        list_state.set_num_items(9);

        let _state = list_state.update_list_state(4);
        let window = list_state.get_start_end();
        assert_eq!(window, (5, 9));
    }

    #[test]
    fn test_console_resize() {
        let mut list_state = WindowedListState::new();
        // Start with 20 items and console size of 5
        list_state.set_num_items(20);
        // Go to the last item (2 times previous).
        list_state.previous();
        list_state.previous();
        list_state.update_list_state(5);
        assert_eq!(list_state.get_start_end(), (15, 20));
        // Resize to 10.
        list_state.update_list_state(10);
        assert_eq!(list_state.get_start_end(), (10, 20));
        // Resize to 50.
        list_state.update_list_state(50);
        assert_eq!(list_state.get_start_end(), (0, 20));
    }
}
