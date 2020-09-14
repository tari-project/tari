use crate::ui::{
    components::{balance::Balance, Component},
    multi_column_list::MultiColumnList,
    state::AppState,
    SelectedTransactionList,
    MAX_WIDTH,
};
use tari_wallet::transaction_service::storage::database::{TransactionDirection, TransactionStatus};
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

pub struct TransactionsTab {
    balance: Balance,
    selected_tx_list: SelectedTransactionList,
    pending_txs_state: ListState,
    completed_txs_state: ListState,
}

impl TransactionsTab {
    pub fn new() -> Self {
        Self {
            balance: Balance::new(),
            selected_tx_list: SelectedTransactionList::PendingTxs,
            pending_txs_state: ListState::default(),
            completed_txs_state: ListState::default(),
        }
    }

    fn draw_transaction_lists<B>(&mut self, f: &mut Frame<B>, area: Rect, app_state: &AppState)
    where B: Backend {
        let total = app_state.pending_txs.len() + app_state.completed_txs.len();
        let (pending_constraint, completed_constaint) = if app_state.pending_txs.len() == 0 {
            (Constraint::Min(4), Constraint::Min(4))
        } else {
            if app_state.pending_txs.len() as f32 / total as f32 > 0.25 {
                (
                    Constraint::Ratio(app_state.pending_txs.len() as u32, total as u32),
                    Constraint::Ratio(app_state.completed_txs.len() as u32, total as u32),
                )
            } else {
                (Constraint::Max(5), Constraint::Min(4))
            }
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
        let style = if self.selected_tx_list == SelectedTransactionList::PendingTxs {
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
        for t in app_state.pending_txs.items.iter() {
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
        column_list.render(f, list_areas[0], &mut self.pending_txs_state);

        //  Completed Transactions
        let style = if self.selected_tx_list == SelectedTransactionList::CompletedTxs {
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
        for t in app_state.completed_txs.items.iter() {
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
        column_list.render(f, list_areas[1], &mut self.completed_txs_state);
    }

    fn draw_detailed_transaction<B>(&self, f: &mut Frame<B>, area: Rect, app_state: &AppState)
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

        if let Some(tx) = app_state.detailed_transaction.as_ref() {
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
}

impl<B: Backend> Component<B> for TransactionsTab {
    fn draw(&mut self, f: &mut Frame<B>, area: Rect, app_state: &AppState) {
        let balance_main_area = Layout::default()
            .constraints([Constraint::Length(3), Constraint::Min(10), Constraint::Length(11)].as_ref())
            .split(area);

        self.balance.draw(f, balance_main_area[0], app_state);
        self.draw_transaction_lists(f, balance_main_area[1], app_state);
        self.draw_detailed_transaction(f, balance_main_area[2], app_state);
    }

    fn on_key(&mut self, app_state: &mut AppState, c: char) {
        match c {
            'p' => {
                self.selected_tx_list = SelectedTransactionList::PendingTxs;
                app_state.pending_txs.select_first();
                app_state.completed_txs.unselect();
            },
            'c' => {
                self.selected_tx_list = SelectedTransactionList::CompletedTxs;
                app_state.pending_txs.unselect();
                app_state.completed_txs.select_first();
            },
            '\n' => match self.selected_tx_list {
                SelectedTransactionList::PendingTxs => {
                    app_state.detailed_transaction = app_state.pending_txs.selected_item().map(|i| i.clone());
                },
                SelectedTransactionList::CompletedTxs => {
                    app_state.detailed_transaction = app_state.completed_txs.selected_item().map(|i| i.clone());
                },
            },
            _ => {},
        }
    }

    fn on_up(&mut self, app_state: &mut AppState) {
        match self.selected_tx_list {
            SelectedTransactionList::PendingTxs => {
                app_state.pending_txs.previous();
                app_state.detailed_transaction = app_state.pending_txs.selected_item().map(|i| i.clone());
            },
            SelectedTransactionList::CompletedTxs => {
                app_state.completed_txs.previous();
                app_state.detailed_transaction = app_state.completed_txs.selected_item().map(|i| i.clone());
            },
        }
    }

    fn on_down(&mut self, app_state: &mut AppState) {
        match self.selected_tx_list {
            SelectedTransactionList::PendingTxs => {
                app_state.pending_txs.next();
                app_state.detailed_transaction = app_state.pending_txs.selected_item().map(|i| i.clone());
            },
            SelectedTransactionList::CompletedTxs => {
                app_state.completed_txs.next();
                app_state.detailed_transaction = app_state.completed_txs.selected_item().map(|i| i.clone());
            },
        }
    }
}
