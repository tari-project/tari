// Copyright 2019. The Tari Project
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

use std::io::stdout;

use chrono::{Datelike, Utc};
use crossterm::{execute, terminal::SetSize};
use tari_app_utilities::consts;

/// returns the top or bottom box line of the specified length
fn box_line(length: usize, is_top: bool) -> String {
    if length < 2 {
        return String::new();
    }
    if is_top {
        format!("{}{}{}", "┌", "─".repeat(length - 2), "┐")
    } else {
        format!("{}{}{}", "└", "─".repeat(length - 2), "┘")
    }
}

/// returns a horizontal rule of the box of the specified length
fn box_separator(length: usize) -> String {
    if length < 2 {
        return String::new();
    }
    format!("{}{}{}", "├", "─".repeat(length - 2), "┤")
}

/// returns a line in the box, with box borders at the beginning and end, contents centered.
fn box_data(data: String, target_length: usize) -> String {
    let padding = if ((target_length - 2) / 2) > (data.chars().count() / 2) {
        ((target_length - 2) / 2) - (data.chars().count() / 2)
    } else {
        0
    };
    let mut s = format!("{}{}{}{}", " ".repeat(padding), data, " ".repeat(padding), "│");
    // for integer rounding error, usually only 1 char, to ensure border lines up
    while s.chars().count() < target_length - 1 {
        s = format!("{}{}", " ", s);
    }
    format!("{}{}", "│", s)
}

/// returns a vector of strings, for each vector of strings, the strings are combined (padded and spaced as necessary),
/// then the result is sent to `box_data` before being added to the result
fn box_tabular_data_rows(
    data: Vec<Vec<String>>,
    sizes: Vec<usize>,
    target_length: usize,
    spacing: usize,
) -> Vec<String> {
    let max_cell_length = sizes.iter().max().unwrap();
    let mut result = Vec::new();
    for items in data {
        let mut s = " ".repeat(spacing);
        for string in items {
            if &string.chars().count() < max_cell_length {
                let padding = (max_cell_length / 2) - (&string.chars().count() / 2);
                s = format!(
                    "{}{}{}{}{}",
                    s,
                    " ".repeat(padding),
                    string,
                    " ".repeat(padding),
                    " ".repeat(spacing)
                );
            } else {
                s = format!("{}{}{}", s, string, " ".repeat(spacing));
            }
        }
        result.push(box_data(s, target_length));
    }
    result
}

fn multiline_find_display_length(lines: &str) -> usize {
    let mut result = 0;
    if let Some(line) = lines.lines().max_by(|x, y| x.chars().count().cmp(&y.chars().count())) {
        result = line.as_bytes().len();
        result /= 2;
        result -= result / 10;
    }
    result
}

/// Try to resize terminal to make sure the width is enough.
/// In case of error, just simply print out the error.
fn resize_terminal_to_fit_the_box(width: usize, height: usize) {
    if let Err(e) = execute!(stdout(), SetSize(width as u16, height as u16)) {
        println!("Can't resize terminal to fit the box. Error: {}", e)
    }
}

/// Prints a pretty banner on the console as well as the list of available commands
pub fn print_banner(commands: Vec<String>, chunk_size: i32) {
    let chunks: Vec<Vec<String>> = commands.chunks(chunk_size as usize).map(|x| x.to_vec()).collect();
    let mut cell_sizes = Vec::new();

    let mut row_cell_count: i32 = 0;
    let mut command_data: Vec<Vec<String>> = Vec::new();
    for chunk in chunks {
        let mut cells: Vec<String> = Vec::new();
        for item in chunk {
            cells.push(item.clone());
            cell_sizes.push(item.chars().count());
            row_cell_count += 1;
        }
        if row_cell_count < chunk_size {
            while row_cell_count < chunk_size {
                cells.push(" ".to_string());
                cell_sizes.push(1);
                row_cell_count += 1;
            }
        } else {
            row_cell_count = 0;
        }
        command_data.push(cells);
    }

    let row_cell_sizes: Vec<Vec<usize>> = cell_sizes.chunks(chunk_size as usize).map(|x| x.to_vec()).collect();
    let mut row_cell_size = Vec::new();
    let mut max_cell_size: usize = 0;
    for sizes in row_cell_sizes {
        for size in sizes {
            if size > max_cell_size {
                max_cell_size = size;
            }
        }
        row_cell_size.push(max_cell_size);
        max_cell_size = 0;
    }

    let banner = include!("../../assets/tari_banner.rs");
    let target_line_length = multiline_find_display_length(banner);

    for line in banner.lines() {
        println!("{}", line);
    }
    println!("\n{}", box_line(target_line_length, true));
    let logo = include!("../../assets/tari_logo.rs");
    for line in logo.lines() {
        println!("{}", box_data(line.to_string(), target_line_length));
    }
    println!("{}", box_data(" ".to_string(), target_line_length));
    println!("{}", box_data("Tari Base Node".to_string(), target_line_length));
    println!("{}", box_data("~~~~~~~~~~~~~~".to_string(), target_line_length));
    println!(
        "{}",
        box_data(
            format!("Copyright 2019-{}. {}", Utc::now().year(), consts::APP_AUTHOR),
            target_line_length
        )
    );
    println!(
        "{}",
        box_data(format!("Version {}", consts::APP_VERSION), target_line_length)
    );
    println!("{}", box_separator(target_line_length));
    println!("{}", box_data("Commands".to_string(), target_line_length));
    println!("{}", box_data("~~~~~~~~".to_string(), target_line_length));
    println!("{}", box_separator(target_line_length));
    let rows = box_tabular_data_rows(command_data, row_cell_size, target_line_length, 10);
    // There are 24 fixed rows besides the possible changed "Commands" rows
    // and plus 2 more blank rows for better layout.
    let height_to_resize = &rows.len() + 24 + 2;
    for row in rows {
        println!("{}", row);
    }
    println!("{}", box_line(target_line_length, false));

    resize_terminal_to_fit_the_box(target_line_length, height_to_resize);
}
