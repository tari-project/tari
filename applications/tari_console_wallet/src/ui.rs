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

use crate::{
    app::{App, SelectedTransactionList, SendInputMode},
    dummy_data::get_dummy_base_node_status,
    utils::{formatting::display_compressed_string, widgets::MultiColumnList},
};
use tari_core::transactions::tari_amount::MicroTari;
use tari_wallet::transaction_service::storage::database::{TransactionDirection, TransactionStatus};
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, ListItem, Paragraph, Row, Table, Tabs, Wrap},
    Frame,
};
use unicode_width::UnicodeWidthStr;

const MAX_WIDTH: u16 = 133;

pub fn draw<B: Backend>(f: &mut Frame<B>, app: &mut App) {
    let max_width_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(MAX_WIDTH), Constraint::Min(0)].as_ref())
        .split(f.size());
    let title_chunks = Layout::default()
        .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
        .split(max_width_layout[0]);
    let title_halves = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
        .split(title_chunks[0]);

    let titles = app
        .tabs
        .titles
        .iter()
        .map(|t| Spans::from(Span::styled(*t, Style::default().fg(Color::White))))
        .collect();
    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title(Span::styled(
            app.title,
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        )))
        .highlight_style(Style::default().fg(Color::Magenta))
        .select(app.tabs.index);
    f.render_widget(tabs, title_halves[0]);

    let chain_meta_data = match get_dummy_base_node_status() {
        None => Spans::from(vec![
            Span::styled("Base Node Chain Tip:", Style::default().fg(Color::Magenta)),
            Span::raw(" "),
            Span::styled(" *Not Connected*", Style::default().fg(Color::Red)),
        ]),
        Some(tip) => Spans::from(vec![
            Span::styled("Base Node Chain Tip:", Style::default().fg(Color::Magenta)),
            Span::raw(" "),
            Span::styled(format!("{}", tip), Style::default().fg(Color::Green)),
        ]),
    };
    let chain_meta_data_paragraph =
        Paragraph::new(chain_meta_data).block(Block::default().borders(Borders::ALL).title(Span::styled(
            "Base Node Status:",
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        )));
    f.render_widget(chain_meta_data_paragraph, title_halves[1]);

    match app.tabs.index {
        0 => draw_first_tab(f, app, title_chunks[1]),
        1 => draw_second_tab(f, app, title_chunks[1]),
        2 => draw_third_tab(f, app, title_chunks[1]),
        _ => {},
    };
}

fn draw_first_tab<B>(f: &mut Frame<B>, app: &mut App, area: Rect)
where B: Backend {
    let balance_main_area = Layout::default()
        .constraints([Constraint::Length(3), Constraint::Min(10), Constraint::Length(11)].as_ref())
        .split(area);

    draw_balance(f, app, balance_main_area[0]);
    draw_transaction_lists(f, app, balance_main_area[1]);
    draw_detailed_transaction(f, app, balance_main_area[2]);
}

fn draw_second_tab<B>(f: &mut Frame<B>, app: &mut App, area: Rect)
where B: Backend {
    let balance_main_area = Layout::default()
        .constraints(
            [
                Constraint::Length(3),
                Constraint::Min(10),
                Constraint::Min(42),
                Constraint::Min(1),
            ]
            .as_ref(),
        )
        .split(area);

    draw_balance(f, app, balance_main_area[0]);
    draw_send_form(f, app, balance_main_area[1]);

    if app.show_contacts {
        draw_contacts(f, app, balance_main_area[2]);
    } else {
        draw_whoami(f, app, balance_main_area[2]);
    }
}

fn draw_third_tab<B>(f: &mut Frame<B>, _app: &mut App, area: Rect)
where B: Backend {
    // This is all dummy content and layout for review
    let main_chunks = Layout::default()
        .constraints([Constraint::Length(1), Constraint::Length(8), Constraint::Min(10)].as_ref())
        .split(area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled("Base Node Peer", Style::default().fg(Color::White)));
    f.render_widget(block, main_chunks[1]);
    let base_node_layout = Layout::default()
        .constraints([Constraint::Length(3), Constraint::Length(3)].as_ref())
        .margin(1)
        .split(main_chunks[1]);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled("Public Key", Style::default().fg(Color::White)));
    f.render_widget(block, base_node_layout[0]);
    let label_layout = Layout::default()
        .constraints([Constraint::Length(1)].as_ref())
        .margin(1)
        .split(base_node_layout[0]);
    let public_key =
        Paragraph::new("92b34a4dc815531af8aeb8a1f1c8d18b927ddd7feabc706df6a1f87cf5014e54").wrap(Wrap { trim: true });
    f.render_widget(public_key, label_layout[0]);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled("Public Address", Style::default().fg(Color::White)));
    f.render_widget(block, base_node_layout[1]);
    let label_layout = Layout::default()
        .constraints([Constraint::Length(1)].as_ref())
        .margin(1)
        .split(base_node_layout[1]);
    let public_address = Paragraph::new("/onion3/mqsfoi62gonulivatrhitugwil3hcxf23eisaieetgyw7x2pdi2bzpyd:18142")
        .wrap(Wrap { trim: true });
    f.render_widget(public_address, label_layout[0]);

    let header = ["Public Key", "User Agent"];
    let rows = vec![
        Row::Data(
            vec![
                "dc77cae83d06cca0a6912cd93eb04e13345811e94e44d9bf4941495b7a35e644",
                "tari/basenode/0.2.4",
            ]
            .into_iter(),
        ),
        Row::Data(
            vec![
                "fe3c7797045d6850c5b3969649f77f93d7dc46e77e293dfa90f1ac36ba8d8501",
                "tari/basenode/0.3.1",
            ]
            .into_iter(),
        ),
        Row::Data(
            vec![
                "fe3c7797045d6850c5b3969649f77f93d7dc46e77e293dfa90f1ac36ba8d8501",
                "tari/basenode/0.3.1",
            ]
            .into_iter(),
        ),
        Row::Data(
            vec![
                "d440b328e69b20dd8ee6c4a61aeb18888939f0f67cf96668840b7f72055d834c",
                "tari/wallet/0.2.3",
            ]
            .into_iter(),
        ),
    ];

    let table = Table::new(header.iter(), rows.into_iter())
        .block(Block::default().title("Connected Peers").borders(Borders::ALL))
        .header_style(Style::default().fg(Color::Magenta))
        .widths(&[Constraint::Length(65), Constraint::Length(65), Constraint::Min(1)]);
    f.render_widget(table, main_chunks[2]);
}

fn draw_balance<B>(f: &mut Frame<B>, _app: &mut App, area: Rect)
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

fn draw_transaction_lists<B>(f: &mut Frame<B>, app: &mut App, area: Rect)
where B: Backend {
    let total = app.pending_txs.len() + app.completed_txs.len();
    let (pending_constraint, completed_constaint) = if app.pending_txs.len() == 0 {
        (Constraint::Min(4), Constraint::Min(4))
    } else if app.pending_txs.len() as f32 / total as f32 > 0.25 {
        (
            Constraint::Ratio(app.pending_txs.len() as u32, total as u32),
            Constraint::Ratio(app.completed_txs.len() as u32, total as u32),
        )
    } else {
        (Constraint::Max(5), Constraint::Min(4))
    };

    let list_areas = Layout::default()
        .constraints([pending_constraint, completed_constaint].as_ref())
        .split(area);

    let instructions = Paragraph::new(Spans::from(vec![
        Span::raw(" Use "),
        Span::styled("P/C", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" to move between transaction lists, "),
        Span::styled("Up/Down Arrow Keys", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" to select a transaction, "),
        Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" to open Transaction Details."),
    ]))
    .wrap(Wrap { trim: true });
    f.render_widget(instructions, list_areas[0]);

    // Pending Transactions
    let style = if app.selected_tx_list == SelectedTransactionList::PendingTxs {
        Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled("(P)ending Transactions", style));
    f.render_widget(block, list_areas[0]);

    let mut column0_items = Vec::new();
    let mut column1_items = Vec::new();
    let mut column2_items = Vec::new();
    let mut column3_items = Vec::new();
    for t in app.pending_txs.items.iter() {
        if t.direction == TransactionDirection::Outbound {
            column0_items.push(ListItem::new(Span::raw(format!("{}", t.destination_public_key))));
            column1_items.push(ListItem::new(Span::styled(
                format!("{}", t.amount),
                Style::default().fg(Color::Red),
            )));
        } else {
            column0_items.push(ListItem::new(Span::raw(format!("{}", t.source_public_key))));
            column1_items.push(ListItem::new(Span::styled(
                format!("{}", t.amount),
                Style::default().fg(Color::Green),
            )));
        }
        column2_items.push(ListItem::new(Span::raw(format!(
            "{}",
            t.timestamp.format("%Y-%m-%d %H:%M:%S")
        ))));
        column3_items.push(ListItem::new(Span::raw(t.message.as_str())));
    }

    let column_list = MultiColumnList::new()
        .highlight_style(Style::default().add_modifier(Modifier::BOLD).fg(Color::Magenta))
        .heading_style(Style::default().fg(Color::Magenta))
        .max_width(MAX_WIDTH)
        .add_column(Some("Source/Destination Public Key"), Some(67), column0_items)
        .add_column(Some("Amount"), Some(18), column1_items)
        .add_column(Some("Timestamp"), Some(20), column2_items)
        .add_column(Some("Message"), None, column3_items);
    column_list.render(f, list_areas[0], &mut app.pending_txs.state);

    //  Completed Transactions
    let style = if app.selected_tx_list == SelectedTransactionList::CompletedTxs {
        Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled("(C)ompleted Transactions", style));
    f.render_widget(block, list_areas[1]);

    let mut column0_items = Vec::new();
    let mut column1_items = Vec::new();
    let mut column2_items = Vec::new();
    let mut column3_items = Vec::new();
    for t in app.completed_txs.items.iter() {
        if t.direction == TransactionDirection::Outbound {
            column0_items.push(ListItem::new(Span::raw(format!("{}", t.destination_public_key))));
            column1_items.push(ListItem::new(Span::styled(
                format!("{}", t.amount),
                Style::default().fg(Color::Red),
            )));
        } else {
            column0_items.push(ListItem::new(Span::raw(format!("{}", t.source_public_key))));
            column1_items.push(ListItem::new(Span::styled(
                format!("{}", t.amount),
                Style::default().fg(Color::Green),
            )));
        }
        column2_items.push(ListItem::new(Span::raw(format!(
            "{}",
            t.timestamp.format("%Y-%m-%d %H:%M:%S")
        ))));
        column3_items.push(ListItem::new(Span::raw(format!("{}", t.status))));
    }

    let column_list = MultiColumnList::new()
        .highlight_style(Style::default().add_modifier(Modifier::BOLD).fg(Color::Magenta))
        .heading_style(Style::default().fg(Color::Magenta))
        .max_width(MAX_WIDTH)
        .add_column(Some("Source/Destination Public Key"), Some(67), column0_items)
        .add_column(Some("Amount"), Some(18), column1_items)
        .add_column(Some("Timestamp"), Some(20), column2_items)
        .add_column(Some("Status"), None, column3_items);
    column_list.render(f, list_areas[1], &mut app.completed_txs.state);
}

fn draw_detailed_transaction<B>(f: &mut Frame<B>, app: &mut App, area: Rect)
where B: Backend {
    let block = Block::default().borders(Borders::ALL).title(Span::styled(
        "Transaction Details",
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
    ));
    f.render_widget(block, area);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(24), Constraint::Min(2)].as_ref())
        .margin(1)
        .split(area);

    // Labels:
    let label_layout = Layout::default()
        .constraints(
            [
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
            ]
            .as_ref(),
        )
        .split(columns[0]);

    let tx_id = Span::styled("TxID:", Style::default().fg(Color::Magenta));
    let source_public_key = Span::styled("Source Public Key:", Style::default().fg(Color::Magenta));
    let destination_public_key = Span::styled("Destination Public Key:", Style::default().fg(Color::Magenta));
    let direction = Span::styled("Direction:", Style::default().fg(Color::Magenta));
    let amount = Span::styled("Amount:", Style::default().fg(Color::Magenta));
    let fee = Span::styled("Fee:", Style::default().fg(Color::Magenta));
    let status = Span::styled("Status:", Style::default().fg(Color::Magenta));
    let message = Span::styled("Message:", Style::default().fg(Color::Magenta));
    let timestamp = Span::styled("Timestamp:", Style::default().fg(Color::Magenta));
    let paragraph = Paragraph::new(tx_id).wrap(Wrap { trim: true });
    f.render_widget(paragraph, label_layout[0]);
    let paragraph = Paragraph::new(source_public_key).wrap(Wrap { trim: true });
    f.render_widget(paragraph, label_layout[1]);
    let paragraph = Paragraph::new(destination_public_key).wrap(Wrap { trim: true });
    f.render_widget(paragraph, label_layout[2]);
    let paragraph = Paragraph::new(direction).wrap(Wrap { trim: true });
    f.render_widget(paragraph, label_layout[3]);
    let paragraph = Paragraph::new(amount).wrap(Wrap { trim: true });
    f.render_widget(paragraph, label_layout[4]);
    let paragraph = Paragraph::new(fee).wrap(Wrap { trim: true });
    f.render_widget(paragraph, label_layout[5]);
    let paragraph = Paragraph::new(status).wrap(Wrap { trim: true });
    f.render_widget(paragraph, label_layout[6]);
    let paragraph = Paragraph::new(message).wrap(Wrap { trim: true });
    f.render_widget(paragraph, label_layout[7]);
    let paragraph = Paragraph::new(timestamp).wrap(Wrap { trim: true });
    f.render_widget(paragraph, label_layout[8]);

    // Content:

    if let Some(tx) = app.detailed_transaction.as_ref() {
        let content_layout = Layout::default()
            .constraints(
                [
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Length(1),
                ]
                .as_ref(),
            )
            .split(columns[1]);
        let tx_id = Span::styled(format!("{}", tx.tx_id), Style::default().fg(Color::White));

        let source_public_key =
            if tx.status == TransactionStatus::Pending && tx.direction == TransactionDirection::Outbound {
                Span::raw("")
            } else {
                Span::styled(format!("{}", tx.source_public_key), Style::default().fg(Color::White))
            };
        let destination_public_key =
            if tx.status == TransactionStatus::Pending && tx.direction == TransactionDirection::Inbound {
                Span::raw("")
            } else {
                Span::styled(
                    format!("{}", tx.destination_public_key),
                    Style::default().fg(Color::White),
                )
            };
        let direction = Span::styled(format!("{}", tx.direction), Style::default().fg(Color::White));
        let amount = Span::styled(format!("{}", tx.amount), Style::default().fg(Color::White));
        let fee = Span::styled(format!("{}", tx.fee), Style::default().fg(Color::White));
        let status = Span::styled(format!("{}", tx.status), Style::default().fg(Color::White));
        let message = Span::styled(tx.message.as_str(), Style::default().fg(Color::White));
        let timestamp = Span::styled(
            format!("{}", tx.timestamp.format("%Y-%m-%d %H:%M:%S")),
            Style::default().fg(Color::White),
        );
        let paragraph = Paragraph::new(tx_id).wrap(Wrap { trim: true });
        f.render_widget(paragraph, content_layout[0]);
        let paragraph = Paragraph::new(source_public_key).wrap(Wrap { trim: true });
        f.render_widget(paragraph, content_layout[1]);
        let paragraph = Paragraph::new(destination_public_key).wrap(Wrap { trim: true });
        f.render_widget(paragraph, content_layout[2]);
        let paragraph = Paragraph::new(direction).wrap(Wrap { trim: true });
        f.render_widget(paragraph, content_layout[3]);
        let paragraph = Paragraph::new(amount).wrap(Wrap { trim: true });
        f.render_widget(paragraph, content_layout[4]);
        let paragraph = Paragraph::new(fee).wrap(Wrap { trim: true });
        f.render_widget(paragraph, content_layout[5]);
        let paragraph = Paragraph::new(status).wrap(Wrap { trim: true });
        f.render_widget(paragraph, content_layout[6]);
        let paragraph = Paragraph::new(message).wrap(Wrap { trim: true });
        f.render_widget(paragraph, content_layout[7]);
        let paragraph = Paragraph::new(timestamp).wrap(Wrap { trim: true });
        f.render_widget(paragraph, content_layout[8]);
    }
}

fn draw_send_form<B>(f: &mut Frame<B>, app: &mut App, area: Rect)
where B: Backend {
    let block = Block::default().borders(Borders::ALL).title(Span::styled(
        "Send Transaction",
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
    ));
    f.render_widget(block, area);
    let vert_chunks = Layout::default()
        .constraints([Constraint::Length(2), Constraint::Length(3), Constraint::Length(3)].as_ref())
        .margin(1)
        .split(area);
    let instructions = Paragraph::new(Spans::from(vec![
        Span::raw("Press "),
        Span::styled("T", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" to edit "),
        Span::styled("To", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" field, "),
        Span::styled("A", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" to edit "),
        Span::styled("Amount", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" field, "),
        Span::styled("C", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" to select a contact, "),
        Span::styled("S", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" to send transaction."),
    ]))
    .block(Block::default());
    f.render_widget(instructions, vert_chunks[0]);

    let to_input = Paragraph::new(app.to_field.as_ref())
        .style(match app.send_input_mode {
            SendInputMode::To => Style::default().fg(Color::Magenta),
            _ => Style::default(),
        })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("(T)o (Public Key or Emoji ID) :"),
        );
    f.render_widget(to_input, vert_chunks[1]);

    let amount_input = Paragraph::new(app.amount_field.as_ref())
        .style(match app.send_input_mode {
            SendInputMode::Amount => Style::default().fg(Color::Magenta),
            _ => Style::default(),
        })
        .block(Block::default().borders(Borders::ALL).title("(A)mount (uT):"));
    f.render_widget(amount_input, vert_chunks[2]);

    match app.send_input_mode {
        SendInputMode::None => (),
        SendInputMode::To => f.set_cursor(
            // Put cursor past the end of the input text
            vert_chunks[1].x + app.to_field.width() as u16 + 1,
            // Move one line down, from the border to the input line
            vert_chunks[1].y + 1,
        ),
        SendInputMode::Amount => f.set_cursor(
            // Put cursor past the end of the input text
            vert_chunks[2].x + app.amount_field.width() as u16 + 1,
            // Move one line down, from the border to the input line
            vert_chunks[2].y + 1,
        ),
    }
}

fn draw_whoami<B>(f: &mut Frame<B>, app: &mut App, area: Rect)
where B: Backend {
    let block = Block::default().borders(Borders::ALL).title(Span::styled(
        "Who Am I?",
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
    ));
    f.render_widget(block, area);

    let help_body_area = Layout::default()
        .constraints([Constraint::Min(42)].as_ref())
        .margin(1)
        .split(area);

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(42), Constraint::Min(1)].as_ref())
        .margin(1)
        .split(help_body_area[0]);

    let qr_code = Paragraph::new(app.my_identity.qr_code)
        .block(Block::default())
        .wrap(Wrap { trim: true });
    f.render_widget(qr_code, chunks[0]);

    let info_chunks = Layout::default()
        .constraints(
            [
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Min(1),
            ]
            .as_ref(),
        )
        .horizontal_margin(1)
        .split(chunks[1]);

    // Public Key
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled("Public Key", Style::default().fg(Color::White)));
    f.render_widget(block, info_chunks[0]);
    let label_layout = Layout::default()
        .constraints([Constraint::Length(1)].as_ref())
        .margin(1)
        .split(info_chunks[0]);
    let public_key = Paragraph::new(app.my_identity.public_key);
    f.render_widget(public_key, label_layout[0]);

    // Public Address
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled("Public Address", Style::default().fg(Color::White)));
    f.render_widget(block, info_chunks[1]);
    let label_layout = Layout::default()
        .constraints([Constraint::Length(1)].as_ref())
        .margin(1)
        .split(info_chunks[1]);
    let public_address = Paragraph::new(app.my_identity.public_address);
    f.render_widget(public_address, label_layout[0]);

    // Emoji ID
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled("Emoji ID", Style::default().fg(Color::White)));
    f.render_widget(block, info_chunks[2]);
    let label_layout = Layout::default()
        .constraints([Constraint::Length(1)].as_ref())
        .margin(1)
        .split(info_chunks[2]);
    let emoji_id = Paragraph::new(app.my_identity.emoji_id);
    f.render_widget(emoji_id, label_layout[0]);
}

fn draw_contacts<B>(f: &mut Frame<B>, app: &mut App, area: Rect)
where B: Backend {
    let block = Block::default().borders(Borders::ALL).title(Span::styled(
        "Contacts",
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
    ));
    f.render_widget(block, area);
    let list_areas = Layout::default()
        .constraints([Constraint::Length(1), Constraint::Min(42)].as_ref())
        .margin(1)
        .split(area);

    let instructions = Paragraph::new(Spans::from(vec![
        Span::raw(" Use "),
        Span::styled("Up/Down Arrow Keys", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" to select a contact, "),
        Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" to select that contact as a recipient."),
    ]))
    .wrap(Wrap { trim: true });
    f.render_widget(instructions, list_areas[0]);

    let mut column0_items = Vec::new();
    let mut column1_items = Vec::new();
    let mut column2_items = Vec::new();
    for c in app.contacts.items.iter() {
        column0_items.push(ListItem::new(Span::raw(c.alias.clone())));
        column1_items.push(ListItem::new(Span::raw(c.public_key.to_string())));
        column2_items.push(ListItem::new(Span::raw(display_compressed_string(
            c.emoji_id.clone(),
            3,
            3,
        ))));
    }
    let column_list = MultiColumnList::new()
        .highlight_style(Style::default().add_modifier(Modifier::BOLD).fg(Color::Magenta))
        .heading_style(Style::default().fg(Color::Magenta))
        .max_width(MAX_WIDTH)
        .add_column(Some("Alias"), Some(12), column0_items)
        .add_column(Some("Public Key"), Some(67), column1_items)
        .add_column(Some("Emoji ID"), None, column2_items);
    column_list.render(f, list_areas[1], &mut app.contacts.state);
}
