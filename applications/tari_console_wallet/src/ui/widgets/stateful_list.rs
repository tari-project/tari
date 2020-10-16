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

pub struct StatefulList<T>
where T: Clone
{
    pub items: Vec<T>,
    selected: Option<usize>,
}

impl<T> StatefulList<T>
where T: Clone
{
    pub fn new() -> StatefulList<T> {
        StatefulList {
            items: Vec::new(),
            selected: None,
        }
    }

    pub fn next(&mut self) {
        if !self.items.is_empty() {
            let i = match self.selected {
                Some(i) => {
                    if i >= self.items.len() - 1 {
                        0
                    } else {
                        i + 1
                    }
                },
                None => 0,
            };
            self.selected = Some(i);
        } else {
            self.selected = None;
        }
    }

    pub fn previous(&mut self) {
        if !self.items.is_empty() {
            let i = match self.selected {
                Some(i) => {
                    if i == 0 {
                        self.items.len() - 1
                    } else {
                        i - 1
                    }
                },
                None => 0,
            };
            self.selected = Some(i);
        } else {
            self.selected = None;
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn _unselect(&mut self) {
        self.selected = None;
    }

    pub fn select_first(&mut self) {
        if self.items.is_empty() {
            self.selected = None;
        } else {
            self.selected = Some(0);
        }
    }

    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    pub fn selected_item(&self) -> Option<&T> {
        self.selected.and_then(|i| self.items.get(i))
    }

    pub fn get_item_slice(&self, start: usize, end: usize) -> &[T] {
        if self.items.is_empty() || start > end || end > self.items.len() {
            return &[];
        }

        &self.items[start..end]
    }
}
