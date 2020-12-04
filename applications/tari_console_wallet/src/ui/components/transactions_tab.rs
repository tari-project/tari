use std::collections::HashMap;

use crate::ui::{
    components::{balance::Balance, Component},
    state::AppState,
    widgets::{draw_dialog, MultiColumnList, WindowedListState},
    MAX_WIDTH,
};
use tari_crypto::tari_utilities::hex::Hex;
use tari_wallet::transaction_service::storage::models::{
    CompletedTransaction,
    TransactionDirection,
    TransactionStatus,
};
use tokio::runtime::Handle;
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, ListItem, Paragraph, Wrap},
    Frame,
};

pub struct TransactionsTab {
    balance: Balance,
    selected_tx_list: SelectedTransactionList,
    pending_list_state: WindowedListState,
    completed_list_state: WindowedListState,
    detailed_transaction: Option<CompletedTransaction>,
    error_message: Option<String>,
    confirmation_dialog: bool,
}

impl TransactionsTab {
    pub fn new() -> Self {
        Self {
            balance: Balance::new(),
            selected_tx_list: SelectedTransactionList::None,
            pending_list_state: WindowedListState::new(),
            completed_list_state: WindowedListState::new(),
            detailed_transaction: None,
            error_message: None,
            confirmation_dialog: false,
        }
    }

    fn draw_transaction_lists<B>(&mut self, f: &mut Frame<B>, area: Rect, app_state: &AppState)
    where B: Backend {
        let (pending_constraint, completed_constaint) = if app_state.get_pending_txs().is_empty() {
            self.selected_tx_list = SelectedTransactionList::CompletedTxs;
            (Constraint::Max(3), Constraint::Min(4))
        } else {
            (
                Constraint::Length((3 + app_state.get_pending_txs().len()).min(7) as u16),
                Constraint::Min(4),
            )
        };
        let list_areas = Layout::default()
            .constraints([pending_constraint, completed_constaint].as_ref())
            .split(area);

        let style = if self.selected_tx_list == SelectedTransactionList::PendingTxs {
            Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .title(Span::styled("(P)ending Transactions", style));
        f.render_widget(block, list_areas[0]);

        // Pending Transactions
        self.pending_list_state.set_num_items(app_state.get_pending_txs().len());
        let mut pending_list_state = self
            .pending_list_state
            .get_list_state((list_areas[0].height as usize).saturating_sub(3));
        let window = self.pending_list_state.get_start_end();
        let windowed_view = app_state.get_pending_txs_slice(window.0, window.1);

        let text_colors: HashMap<bool, Color> = [(true, Color::DarkGray), (false, Color::Reset)]
            .iter()
            .cloned()
            .collect();

        let mut column0_items = Vec::new();
        let mut column1_items = Vec::new();
        let mut column2_items = Vec::new();
        let mut column3_items = Vec::new();
        for t in windowed_view.iter() {
            let text_color = text_colors.get(&t.cancelled).unwrap_or(&Color::Reset).to_owned();
            if t.direction == TransactionDirection::Outbound {
                column0_items.push(ListItem::new(Span::styled(
                    format!("{}", t.destination_public_key),
                    Style::default().fg(text_color),
                )));
                let amount_style = if t.cancelled {
                    Style::default().fg(Color::Red).add_modifier(Modifier::DIM)
                } else {
                    Style::default().fg(Color::Red)
                };
                column1_items.push(ListItem::new(Span::styled(format!("{}", t.amount), amount_style)));
            } else {
                column0_items.push(ListItem::new(Span::styled(
                    format!("{}", t.source_public_key),
                    Style::default().fg(text_color),
                )));
                let amount_style = if t.cancelled {
                    Style::default().fg(Color::Green).add_modifier(Modifier::DIM)
                } else {
                    Style::default().fg(Color::Green)
                };
                column1_items.push(ListItem::new(Span::styled(format!("{}", t.amount), amount_style)));
            }
            column2_items.push(ListItem::new(Span::styled(
                format!("{}", t.timestamp.format("%Y-%m-%d %H:%M:%S")),
                Style::default().fg(text_color),
            )));
            column3_items.push(ListItem::new(Span::styled(
                t.message.as_str(),
                Style::default().fg(text_color),
            )));
        }

        let column_list = MultiColumnList::new()
            .highlight_style(Style::default().add_modifier(Modifier::BOLD).fg(Color::Magenta))
            .heading_style(Style::default().fg(Color::Magenta))
            .max_width(MAX_WIDTH)
            .add_column(Some("Source/Destination Public Key"), Some(67), column0_items)
            .add_column(Some("Amount"), Some(18), column1_items)
            .add_column(Some("Timestamp"), Some(20), column2_items)
            .add_column(Some("Message"), None, column3_items);
        column_list.render(f, list_areas[0], &mut pending_list_state);

        //  Completed Transactions
        let style = if self.selected_tx_list == SelectedTransactionList::CompletedTxs {
            Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .title(Span::styled("Completed (T)ransactions", style));
        f.render_widget(block, list_areas[1]);

        self.completed_list_state
            .set_num_items(app_state.get_completed_txs().len());
        let mut completed_list_state = self
            .completed_list_state
            .get_list_state((list_areas[1].height as usize).saturating_sub(3));
        let window = self.completed_list_state.get_start_end();
        let windowed_view = app_state.get_completed_txs_slice(window.0, window.1);

        let mut column0_items = Vec::new();
        let mut column1_items = Vec::new();
        let mut column2_items = Vec::new();
        let mut column3_items = Vec::new();

        for t in windowed_view.iter() {
            let text_color = text_colors.get(&t.cancelled).unwrap_or(&Color::Reset).to_owned();
            if t.direction == TransactionDirection::Outbound {
                column0_items.push(ListItem::new(Span::styled(
                    format!("{}", t.destination_public_key),
                    Style::default().fg(text_color),
                )));
                let amount_style = if t.cancelled {
                    Style::default().fg(Color::Red).add_modifier(Modifier::DIM)
                } else {
                    Style::default().fg(Color::Red)
                };
                column1_items.push(ListItem::new(Span::styled(format!("{}", t.amount), amount_style)));
            } else {
                column0_items.push(ListItem::new(Span::styled(
                    format!("{}", t.source_public_key),
                    Style::default().fg(text_color),
                )));
                let amount_style = if t.cancelled {
                    Style::default().fg(Color::Green).add_modifier(Modifier::DIM)
                } else {
                    Style::default().fg(Color::Green)
                };
                column1_items.push(ListItem::new(Span::styled(format!("{}", t.amount), amount_style)));
            }
            column2_items.push(ListItem::new(Span::styled(
                format!("{}", t.timestamp.format("%Y-%m-%d %H:%M:%S")),
                Style::default().fg(text_color),
            )));
            let status = if t.cancelled {
                "Cancelled".to_string()
            } else {
                t.status.to_string()
            };
            column3_items.push(ListItem::new(Span::styled(status, Style::default().fg(text_color))));
        }

        let column_list = MultiColumnList::new()
            .highlight_style(Style::default().add_modifier(Modifier::BOLD).fg(Color::Magenta))
            .heading_style(Style::default().fg(Color::Magenta))
            .max_width(MAX_WIDTH)
            .add_column(Some("Source/Destination Public Key"), Some(67), column0_items)
            .add_column(Some("Amount"), Some(18), column1_items)
            .add_column(Some("Timestamp"), Some(20), column2_items)
            .add_column(Some("Status"), None, column3_items);

        column_list.render(f, list_areas[1], &mut completed_list_state);
    }

    fn draw_detailed_transaction<B>(&self, f: &mut Frame<B>, area: Rect, _app_state: &AppState)
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
        let excess = Span::styled("Excess:", Style::default().fg(Color::Magenta));
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
        let paragraph = Paragraph::new(excess).wrap(Wrap { trim: true });
        f.render_widget(paragraph, label_layout[9]);

        // Content:

        if let Some(tx) = self.detailed_transaction.as_ref() {
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
            let status_msg = if tx.cancelled {
                "Cancelled".to_string()
            } else {
                tx.status.to_string()
            };
            let status = Span::styled(status_msg, Style::default().fg(Color::White));
            let message = Span::styled(tx.message.as_str(), Style::default().fg(Color::White));
            let timestamp = Span::styled(
                format!("{}", tx.timestamp.format("%Y-%m-%d %H:%M:%S")),
                Style::default().fg(Color::White),
            );
            let excess_hex = if tx.transaction.body.kernels().is_empty() {
                "".to_string()
            } else {
                tx.transaction.body.kernels()[0].excess_sig.get_signature().to_hex()
            };
            let excess = Span::styled(excess_hex.as_str(), Style::default().fg(Color::White));
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
            let paragraph = Paragraph::new(excess).wrap(Wrap { trim: true });
            f.render_widget(paragraph, content_layout[9]);
        }
    }
}

impl<B: Backend> Component<B> for TransactionsTab {
    fn draw(&mut self, f: &mut Frame<B>, area: Rect, app_state: &AppState) {
        let areas = Layout::default()
            .constraints(
                [
                    Constraint::Length(3),
                    Constraint::Length(1),
                    Constraint::Min(10),
                    Constraint::Length(12),
                ]
                .as_ref(),
            )
            .split(area);

        self.balance.draw(f, areas[0], app_state);

        let mut span_vec = if app_state.get_pending_txs().is_empty() {
            vec![]
        } else {
            vec![
                Span::raw(" Use "),
                Span::styled("P/T", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to move between transaction lists, "),
            ]
        };

        span_vec.push(Span::styled(
            "Up/Down Arrow Keys",
            Style::default().add_modifier(Modifier::BOLD),
        ));
        span_vec.push(Span::raw(" to select a transaction, "));
        span_vec.push(Span::styled("C", Style::default().add_modifier(Modifier::BOLD)));
        span_vec.push(Span::raw(" to cancel a selected Pending Tx."));

        let instructions = Paragraph::new(Spans::from(span_vec)).wrap(Wrap { trim: true });
        f.render_widget(instructions, areas[1]);

        self.draw_transaction_lists(f, areas[2], app_state);
        self.draw_detailed_transaction(f, areas[3], app_state);

        if let Some(msg) = self.error_message.clone() {
            draw_dialog(f, area, "Error!".to_string(), msg, Color::Red, 120, 9);
        }

        if self.confirmation_dialog {
            draw_dialog(
                f,
                area,
                "Confirm Cancellation".to_string(),
                "Are you sure you want to cancel this pending transaction? \n(Y)es / (N)o".to_string(),
                Color::Red,
                120,
                9,
            );
        }
    }

    fn on_key(&mut self, app_state: &mut AppState, c: char) {
        if self.error_message.is_some() && '\n' == c {
            self.error_message = None;
            return;
        }

        if self.confirmation_dialog {
            if 'n' == c {
                self.confirmation_dialog = false;
                return;
            } else if 'y' == c {
                if self.selected_tx_list == SelectedTransactionList::PendingTxs {
                    if let Some(i) = self.pending_list_state.selected() {
                        if let Some(pending_tx) = app_state.get_pending_tx(i).cloned() {
                            if let Err(e) = Handle::current().block_on(app_state.cancel_transaction(pending_tx.tx_id)) {
                                self.error_message = Some(format!(
                                    "Could not cancel pending transaction.\n{}\nPress Enter to continue.",
                                    e
                                ));
                            }
                        }
                    }
                }
                self.confirmation_dialog = false;
                return;
            }
        }

        match c {
            'p' => {
                self.completed_list_state.select(None);
                self.selected_tx_list = SelectedTransactionList::PendingTxs;
                self.pending_list_state.set_num_items(app_state.get_pending_txs().len());
                let idx = match self.pending_list_state.selected() {
                    None => {
                        self.pending_list_state.select_first();
                        0
                    },
                    Some(i) => i,
                };
                self.detailed_transaction = app_state.get_pending_tx(idx).cloned()
            },
            't' => {
                self.pending_list_state.select(None);
                self.selected_tx_list = SelectedTransactionList::CompletedTxs;
                self.completed_list_state
                    .set_num_items(app_state.get_completed_txs().len());
                let idx = match self.completed_list_state.selected() {
                    None => {
                        self.completed_list_state.select_first();
                        0
                    },
                    Some(i) => i,
                };
                self.detailed_transaction = app_state.get_completed_tx(idx).cloned();
            },
            'c' => {
                if self.selected_tx_list == SelectedTransactionList::PendingTxs {
                    self.confirmation_dialog = true;
                    return;
                }
            },
            '\n' => match self.selected_tx_list {
                SelectedTransactionList::None => {},
                SelectedTransactionList::PendingTxs => {
                    self.detailed_transaction = match self.pending_list_state.selected() {
                        None => None,
                        Some(i) => app_state.get_pending_tx(i).cloned(),
                    };
                },
                SelectedTransactionList::CompletedTxs => {
                    self.detailed_transaction = match self.completed_list_state.selected() {
                        None => None,
                        Some(i) => app_state.get_completed_tx(i).cloned(),
                    };
                },
            },
            _ => {},
        }
    }

    fn on_up(&mut self, app_state: &mut AppState) {
        match self.selected_tx_list {
            SelectedTransactionList::None => {},
            SelectedTransactionList::PendingTxs => {
                self.pending_list_state.set_num_items(app_state.get_pending_txs().len());
                self.pending_list_state.previous();
                self.detailed_transaction = match self.pending_list_state.selected() {
                    None => None,
                    Some(i) => app_state.get_pending_tx(i).cloned(),
                };
            },
            SelectedTransactionList::CompletedTxs => {
                self.completed_list_state
                    .set_num_items(app_state.get_completed_txs().len());
                self.completed_list_state.previous();
                self.detailed_transaction = match self.completed_list_state.selected() {
                    None => None,
                    Some(i) => app_state.get_completed_tx(i).cloned(),
                };
            },
        }
    }

    fn on_down(&mut self, app_state: &mut AppState) {
        match self.selected_tx_list {
            SelectedTransactionList::None => {},
            SelectedTransactionList::PendingTxs => {
                self.pending_list_state.set_num_items(app_state.get_pending_txs().len());
                self.pending_list_state.next();
                self.detailed_transaction = match self.pending_list_state.selected() {
                    None => None,
                    Some(i) => app_state.get_pending_tx(i).cloned(),
                };
            },
            SelectedTransactionList::CompletedTxs => {
                self.completed_list_state
                    .set_num_items(app_state.get_completed_txs().len());
                self.completed_list_state.next();
                self.detailed_transaction = match self.completed_list_state.selected() {
                    None => None,
                    Some(i) => app_state.get_completed_tx(i).cloned(),
                };
            },
        }
    }

    fn on_esc(&mut self, _app_state: &mut AppState) {
        self.selected_tx_list = SelectedTransactionList::None;
        self.pending_list_state.select(None);
        self.completed_list_state.select(None);
        self.detailed_transaction = None;
    }
}

#[derive(PartialEq)]
pub enum SelectedTransactionList {
    None,
    PendingTxs,
    CompletedTxs,
}
