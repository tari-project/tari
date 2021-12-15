//  Copyright 2021, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{fmt, fmt::Display};

use chrono::Local;

#[derive(Debug, Clone, Default)]
pub struct StatusLine {
    fields: Vec<(&'static str, String)>,
}

impl StatusLine {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn add_field<T: ToString>(&mut self, name: &'static str, value: T) -> &mut Self {
        self.fields.push((name, value.to_string()));
        self
    }
}

impl Display for StatusLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ", Local::now().format("%H:%M"))?;
        let s = self.fields.iter().map(|(k, v)| format(k, v)).collect::<Vec<_>>();

        write!(f, "{}", s.join(", "))
    }
}

fn format(k: &&str, v: &str) -> String {
    if k.is_empty() {
        v.to_string()
    } else {
        format!("{}: {}", k, v)
    }
}

#[cfg(test)]
mod test {
    use super::StatusLine;

    #[test]
    fn test_do_not_display_empty_keys() {
        let mut status = StatusLine::new();
        status.add_field("key", "val");
        let display = status.to_string();
        assert!(display.contains("key: val"));
        assert_eq!(display.matches(':').count(), 2);

        let mut status = StatusLine::new();
        status.add_field("", "val");
        let display = status.to_string();
        assert!(display.contains("val"));
        assert_eq!(display.matches(':').count(), 1);
    }
}
