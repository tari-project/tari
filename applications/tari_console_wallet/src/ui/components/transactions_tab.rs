use std::collections::HashMap;

use chrono::{DateTime, Local};
use log::*;
use tari_common_types::transaction::{TransactionDirection, TransactionStatus};
use tari_wallet::transaction_service::storage::models::TxCancellationReason;
use tokio::runtime::Handle;
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, ListItem, Paragraph, Wrap},
    Frame,
};

use crate::ui::{
    components::{balance::Balance, styles, Component},
    state::{AppState, CompletedTransactionInfo},
    widgets::{draw_dialog, MultiColumnList, WindowedListState},
    MAX_WIDTH,
};

const LOG_TARGET: &str = "wallet::console_wallet::transaction_tab";

pub struct TransactionsTab {
    balance: Balance,
    selected_tx_list: SelectedTransactionList,
    pending_list_state: WindowedListState,
    completed_list_state: WindowedListState,
    detailed_transaction: Option<CompletedTransactionInfo>,
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
        let (pending_constraint, completed_constraint) = if app_state.get_pending_txs().is_empty() {
            self.selected_tx_list = SelectedTransactionList::CompletedTxs;
            (Constraint::Max(3), Constraint::Min(4))
        } else {
            (
                Constraint::Length((3 + app_state.get_pending_txs().len()).min(7) as u16),
                Constraint::Min(4),
            )
        };
        let list_areas = Layout::default()
            .constraints([pending_constraint, completed_constraint].as_ref())
            .split(area);

        self.draw_pending_transactions(f, list_areas[0], app_state);
        self.draw_completed_transactions(f, list_areas[1], app_state);
    }

    fn draw_pending_transactions<B>(&mut self, f: &mut Frame<B>, area: Rect, app_state: &AppState)
    where B: Backend {
        let style = if self.selected_tx_list == SelectedTransactionList::PendingTxs {
            Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
        };

        let title = Block::default().borders(Borders::ALL).title(Span::styled(
            format!("(P)ending Transactions ({}) ", app_state.get_pending_txs().len()),
            style,
        ));
        f.render_widget(title, area);

        // Pending Transactions
        self.pending_list_state.set_num_items(app_state.get_pending_txs().len());
        let mut pending_list_state = self
            .pending_list_state
            .get_list_state((area.height as usize).saturating_sub(3));
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
            let text_color = text_colors
                .get(&t.cancelled.is_some())
                .unwrap_or(&Color::Reset)
                .to_owned();
            if t.direction == TransactionDirection::Outbound {
                column0_items.push(ListItem::new(Span::styled(
                    app_state.get_alias(&t.destination_public_key),
                    Style::default().fg(text_color),
                )));
                let amount_style = if t.cancelled.is_some() {
                    Style::default().fg(Color::Red).add_modifier(Modifier::DIM)
                } else {
                    Style::default().fg(Color::Red)
                };
                let amount = if t.unique_id.is_empty() {
                    format!("{}", t.amount)
                } else {
                    format!("Token: {}", t.unique_id)
                };
                column1_items.push(ListItem::new(Span::styled(amount, amount_style)));
            } else {
                column0_items.push(ListItem::new(Span::styled(
                    app_state.get_alias(&t.source_public_key),
                    Style::default().fg(text_color),
                )));
                let amount_style = if t.cancelled.is_some() {
                    Style::default().fg(Color::Green).add_modifier(Modifier::DIM)
                } else {
                    Style::default().fg(Color::Green)
                };
                let amount = if t.unique_id.is_empty() {
                    format!("{}", t.amount)
                } else {
                    format!("Token: {}", t.unique_id)
                };
                column1_items.push(ListItem::new(Span::styled(amount, amount_style)));
            }
            let local_time = DateTime::<Local>::from_utc(t.timestamp, Local::now().offset().to_owned());
            column2_items.push(ListItem::new(Span::styled(
                format!("{}", local_time.format("%Y-%m-%d %H:%M:%S")),
                Style::default().fg(text_color),
            )));
            column3_items.push(ListItem::new(Span::styled(
                t.message.as_str(),
                Style::default().fg(text_color),
            )));
        }

        let column_list = MultiColumnList::new()
            .highlight_style(styles::highlight())
            .heading_style(styles::header_row())
            .max_width(MAX_WIDTH)
            .add_column(Some("Source/Destination Public Key"), Some(67), column0_items)
            .add_column(Some("Amount/Token"), Some(18), column1_items)
            .add_column(Some("Local Date/Time"), Some(20), column2_items)
            .add_column(Some("Message"), None, column3_items);
        column_list.render(f, area, &mut pending_list_state);
    }

    fn draw_completed_transactions<B>(&mut self, f: &mut Frame<B>, area: Rect, app_state: &AppState)
    where B: Backend {
        //  Completed Transactions
        let style = if self.selected_tx_list == SelectedTransactionList::CompletedTxs {
            Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
        };
        let block = Block::default().borders(Borders::ALL).title(Span::styled(
            format!("Completed (T)ransactions ({}) ", app_state.get_completed_txs().len()),
            style,
        ));
        f.render_widget(block, area);

        let completed_txs = app_state.get_completed_txs();
        self.completed_list_state.set_num_items(completed_txs.len());
        let mut completed_list_state = self
            .completed_list_state
            .get_list_state((area.height as usize).saturating_sub(3));
        let (start, end) = self.completed_list_state.get_start_end();
        let windowed_view = &completed_txs[start..end];

        let text_colors: HashMap<bool, Color> = [(true, Color::DarkGray), (false, Color::Reset)]
            .iter()
            .cloned()
            .collect();

        let base_node_state = app_state.get_base_node_state();
        let chain_height = base_node_state
            .chain_metadata
            .as_ref()
            .map(|cm| cm.height_of_longest_chain());

        let mut column0_items = Vec::new();
        let mut column1_items = Vec::new();
        let mut column2_items = Vec::new();
        let mut column3_items = Vec::new();

        for t in windowed_view.iter() {
            let cancelled = t.cancelled.is_some();
            let text_color = text_colors.get(&cancelled).unwrap_or(&Color::Reset).to_owned();
            if t.direction == TransactionDirection::Outbound {
                column0_items.push(ListItem::new(Span::styled(
                    app_state.get_alias(&t.destination_public_key),
                    Style::default().fg(text_color),
                )));
                let amount_style = if t.cancelled.is_some() {
                    Style::default().fg(Color::Red).add_modifier(Modifier::DIM)
                } else {
                    Style::default().fg(Color::Red)
                };
                let amount = if t.unique_id.is_empty() {
                    format!("{}", t.amount)
                } else {
                    format!("Token: {}", t.unique_id)
                };
                column1_items.push(ListItem::new(Span::styled(amount, amount_style)));
            } else {
                column0_items.push(ListItem::new(Span::styled(
                    app_state.get_alias(&t.source_public_key),
                    Style::default().fg(text_color),
                )));
                let color = match (t.cancelled.is_some(), chain_height) {
                    // cancelled
                    (true, _) => Color::DarkGray,
                    // not mature yet
                    (_, Some(height)) if t.maturity > height => Color::Yellow,
                    // default
                    _ => Color::Green,
                };
                let amount_style = Style::default().fg(color);
                let amount = if t.unique_id.is_empty() {
                    format!("{}", t.amount)
                } else {
                    format!("Token: {}", t.unique_id)
                };
                column1_items.push(ListItem::new(Span::styled(amount, amount_style)));
            }
            let local_time = DateTime::<Local>::from_utc(t.timestamp, Local::now().offset().to_owned());
            column2_items.push(ListItem::new(Span::styled(
                format!("{}", local_time.format("%Y-%m-%d %H:%M:%S")),
                Style::default().fg(text_color),
            )));
            let status = if matches!(t.cancelled, Some(TxCancellationReason::AbandonedCoinbase)) {
                "Abandoned".to_string()
            } else if matches!(t.cancelled, Some(TxCancellationReason::UserCancelled)) {
                "Cancelled".to_string()
            } else if t.cancelled.is_some() {
                "Rejected".to_string()
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
            .add_column(Some("Amount/Token"), Some(18), column1_items)
            .add_column(Some("Local Date/Time"), Some(20), column2_items)
            .add_column(Some("Status"), None, column3_items);

        column_list.render(f, area, &mut completed_list_state);
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

        // Labels
        let constraints = [Constraint::Length(1); 13];
        let label_layout = Layout::default().constraints(constraints).split(columns[0]);

        let tx_id = Span::styled("TxID:", Style::default().fg(Color::Magenta));
        let source_public_key = Span::styled("Source Public Key:", Style::default().fg(Color::Magenta));
        let destination_public_key = Span::styled("Destination Public Key:", Style::default().fg(Color::Magenta));
        let direction = Span::styled("Direction:", Style::default().fg(Color::Magenta));
        let amount = Span::styled(
            match self.detailed_transaction.as_ref() {
                Some(tx) => {
                    if tx.unique_id.is_empty() {
                        "Amount:"
                    } else {
                        "Token:"
                    }
                },
                None => "Amount/Token:",
            },
            Style::default().fg(Color::Magenta),
        );
        let fee = Span::styled("Fee:", Style::default().fg(Color::Magenta));
        let status = Span::styled("Status:", Style::default().fg(Color::Magenta));
        let message = Span::styled("Message:", Style::default().fg(Color::Magenta));
        let timestamp = Span::styled("Local Date/Time:", Style::default().fg(Color::Magenta));
        let excess = Span::styled("Excess:", Style::default().fg(Color::Magenta));
        let confirmations = Span::styled("Confirmations:", Style::default().fg(Color::Magenta));
        let mined_height = Span::styled("Mined Height:", Style::default().fg(Color::Magenta));
        let maturity = Span::styled("Maturity:", Style::default().fg(Color::Magenta));

        let trim = Wrap { trim: true };
        let paragraph = Paragraph::new(tx_id).wrap(trim);
        f.render_widget(paragraph, label_layout[0]);
        let paragraph = Paragraph::new(source_public_key).wrap(trim);
        f.render_widget(paragraph, label_layout[1]);
        let paragraph = Paragraph::new(destination_public_key).wrap(trim);
        f.render_widget(paragraph, label_layout[2]);
        let paragraph = Paragraph::new(direction).wrap(trim);
        f.render_widget(paragraph, label_layout[3]);
        let paragraph = Paragraph::new(amount).wrap(trim);
        f.render_widget(paragraph, label_layout[4]);
        let paragraph = Paragraph::new(fee).wrap(trim);
        f.render_widget(paragraph, label_layout[5]);
        let paragraph = Paragraph::new(status).wrap(trim);
        f.render_widget(paragraph, label_layout[6]);
        let paragraph = Paragraph::new(message).wrap(trim);
        f.render_widget(paragraph, label_layout[7]);
        let paragraph = Paragraph::new(timestamp).wrap(trim);
        f.render_widget(paragraph, label_layout[8]);
        let paragraph = Paragraph::new(excess).wrap(trim);
        f.render_widget(paragraph, label_layout[9]);
        let paragraph = Paragraph::new(confirmations).wrap(trim);
        f.render_widget(paragraph, label_layout[10]);
        let paragraph = Paragraph::new(mined_height).wrap(trim);
        f.render_widget(paragraph, label_layout[11]);
        let paragraph = Paragraph::new(maturity).wrap(trim);
        f.render_widget(paragraph, label_layout[12]);

        // Content
        let required_confirmations = app_state.get_required_confirmations();
        if let Some(tx) = self.detailed_transaction.as_ref() {
            let constraints = [Constraint::Length(1); 13];
            let content_layout = Layout::default().constraints(constraints).split(columns[1]);
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
            let amount = tx.amount.to_string();
            let content = if tx.unique_id.is_empty() {
                &amount
            } else {
                &tx.unique_id
            };
            let amount = Span::styled(content, Style::default().fg(Color::White));
            let fee_details = if tx.is_coinbase {
                Span::raw("")
            } else {
                Span::styled(
                    format!(
                        " (weight: {}g, #inputs: {}, #outputs: {})",
                        tx.weight, tx.inputs_count, tx.outputs_count
                    ),
                    Style::default().fg(Color::Gray),
                )
            };
            let fee = Spans::from(vec![
                Span::styled(format!("{}", tx.fee), Style::default().fg(Color::White)),
                fee_details,
            ]);
            let status_msg = if let Some(reason) = tx.cancelled {
                format!("Cancelled: {}", reason)
            } else {
                tx.status.to_string()
            };

            let status = Span::styled(status_msg, Style::default().fg(Color::White));
            let message = Span::styled(tx.message.as_str(), Style::default().fg(Color::White));
            let local_time = DateTime::<Local>::from_utc(tx.timestamp, Local::now().offset().to_owned());
            let timestamp = Span::styled(
                format!("{}", local_time.format("%Y-%m-%d %H:%M:%S")),
                Style::default().fg(Color::White),
            );
            let excess = Span::styled(tx.excess_signature.as_str(), Style::default().fg(Color::White));
            let confirmation_count = app_state.get_confirmations(&tx.tx_id);
            let confirmations_msg = if tx.status == TransactionStatus::MinedConfirmed && tx.cancelled.is_none() {
                format!("{} required confirmations met", required_confirmations)
            } else if tx.status == TransactionStatus::MinedUnconfirmed && tx.cancelled.is_none() {
                if let Some(count) = confirmation_count {
                    format!("{} of {} required confirmations met", count, required_confirmations)
                } else {
                    "N/A".to_string()
                }
            } else {
                "N/A".to_string()
            };
            let confirmations = Span::styled(confirmations_msg.as_str(), Style::default().fg(Color::White));
            let mined_height = Span::styled(
                tx.mined_height
                    .map(|m| m.to_string())
                    .unwrap_or_else(|| "N/A".to_string()),
                Style::default().fg(Color::White),
            );
            let maturity = if tx.maturity > 0 {
                format!("Spendable at Block #{}", tx.maturity)
            } else {
                "N/A".to_string()
            };
            let maturity = Span::styled(maturity, Style::default().fg(Color::White));

            let paragraph = Paragraph::new(tx_id).wrap(trim);
            f.render_widget(paragraph, content_layout[0]);
            let paragraph = Paragraph::new(source_public_key).wrap(trim);
            f.render_widget(paragraph, content_layout[1]);
            let paragraph = Paragraph::new(destination_public_key).wrap(trim);
            f.render_widget(paragraph, content_layout[2]);
            let paragraph = Paragraph::new(direction).wrap(trim);
            f.render_widget(paragraph, content_layout[3]);
            let paragraph = Paragraph::new(amount).wrap(trim);
            f.render_widget(paragraph, content_layout[4]);
            let paragraph = Paragraph::new(fee).wrap(trim);
            f.render_widget(paragraph, content_layout[5]);
            let paragraph = Paragraph::new(status).wrap(trim);
            f.render_widget(paragraph, content_layout[6]);
            let paragraph = Paragraph::new(message).wrap(trim);
            f.render_widget(paragraph, content_layout[7]);
            let paragraph = Paragraph::new(timestamp).wrap(trim);
            f.render_widget(paragraph, content_layout[8]);
            let paragraph = Paragraph::new(excess).wrap(trim);
            f.render_widget(paragraph, content_layout[9]);
            let paragraph = Paragraph::new(confirmations).wrap(trim);
            f.render_widget(paragraph, content_layout[10]);
            let paragraph = Paragraph::new(mined_height).wrap(trim);
            f.render_widget(paragraph, content_layout[11]);
            let paragraph = Paragraph::new(maturity).wrap(trim);
            f.render_widget(paragraph, content_layout[12]);
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
                    Constraint::Length(15),
                ]
                .as_ref(),
            )
            .split(area);

        self.balance.draw(f, areas[0], app_state);

        let mut span_vec = if app_state.get_pending_txs().is_empty() {
            vec![]
        } else {
            vec![
                Span::styled("P/T", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" moves between transaction lists, "),
            ]
        };

        span_vec.push(Span::styled(
            " Up↑/Down↓",
            Style::default().add_modifier(Modifier::BOLD),
        ));
        span_vec.push(Span::raw(" select Tx "));
        span_vec.push(Span::styled("(C)", Style::default().add_modifier(Modifier::BOLD)));
        span_vec.push(Span::raw(" cancel selected pending Txs "));
        span_vec.push(Span::styled("(A)", Style::default().add_modifier(Modifier::BOLD)));
        span_vec.push(Span::raw(" show/hide abandoned coinbases "));
        span_vec.push(Span::styled("(R)", Style::default().add_modifier(Modifier::BOLD)));
        span_vec.push(Span::raw(" rebroadcast Txs "));
        span_vec.push(Span::styled("(Esc)", Style::default().add_modifier(Modifier::BOLD)));
        span_vec.push(Span::raw(" exit list"));

        let instructions = Paragraph::new(Spans::from(span_vec)).wrap(Wrap { trim: false });
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
                }
            },
            // Rebroadcast
            'r' => {
                if let Err(e) = Handle::current().block_on(app_state.rebroadcast_all()) {
                    error!(target: LOG_TARGET, "Error rebroadcasting transactions: {}", e);
                }
            },
            'a' => app_state.toggle_abandoned_coinbase_filter(),
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
        if self.confirmation_dialog {
            return;
        }
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
        if self.confirmation_dialog {
            return;
        }
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
        self.confirmation_dialog = false;
    }
}

#[derive(PartialEq)]
pub enum SelectedTransactionList {
    None,
    PendingTxs,
    CompletedTxs,
}
