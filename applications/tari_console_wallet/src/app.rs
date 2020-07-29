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
    dummy_data::{dummy_completed_txs, dummy_inbound_txs, dummy_outbound_txs, get_dummy_contacts, get_dummy_identity},
    utils::widget_states::{StatefulList, TabsState},
};
use tari_wallet::{
    contacts_service::storage::database::Contact,
    transaction_service::storage::database::CompletedTransaction,
    util::emoji::EmojiId,
};

pub struct App<'a> {
    pub title: &'a str,
    pub should_quit: bool,
    // Cached state this will need to be cleaned up into a threadsafe container
    pub pending_txs: StatefulList<CompletedTransaction>,
    pub completed_txs: StatefulList<CompletedTransaction>,
    pub detailed_transaction: Option<CompletedTransaction>,
    pub my_identity: MyIdentity<'a>,
    pub contacts: StatefulList<UiContact>,
    // Ui working state
    pub tabs: TabsState<'a>,
    pub selected_tx_list: SelectedTransactionList,
    pub to_field: String,
    pub amount_field: String,
    pub send_input_mode: SendInputMode,
    pub show_contacts: bool,
}

impl<'a> App<'a> {
    pub fn new(title: &'a str) -> App<'a> {
        let mut pending_transactions: Vec<CompletedTransaction> = Vec::new();
        pending_transactions.extend(
            dummy_inbound_txs()
                .iter()
                .map(|i| CompletedTransaction::from(i.clone()))
                .collect::<Vec<CompletedTransaction>>(),
        );
        pending_transactions.extend(
            dummy_outbound_txs()
                .iter()
                .map(|i| CompletedTransaction::from(i.clone()))
                .collect::<Vec<CompletedTransaction>>(),
        );
        pending_transactions.sort_by(|a: &CompletedTransaction, b: &CompletedTransaction| {
            b.timestamp.partial_cmp(&a.timestamp).unwrap()
        });

        Self {
            title,
            should_quit: false,
            tabs: TabsState::new(vec!["Transactions", "Send/Receive", "Network"]),
            pending_txs: StatefulList::with_items(pending_transactions),
            completed_txs: StatefulList::with_items(dummy_completed_txs()),
            detailed_transaction: None,
            selected_tx_list: SelectedTransactionList::None,
            to_field: "".to_string(),
            amount_field: "".to_string(),
            send_input_mode: SendInputMode::None,
            my_identity: get_dummy_identity(),
            contacts: StatefulList::with_items(
                get_dummy_contacts()
                    .iter()
                    .map(|c| UiContact::from(c.clone()))
                    .collect(),
            ),
            show_contacts: false,
        }
    }

    pub fn on_control_key(&mut self, c: char) {
        match c {
            'c' => {
                self.should_quit = true;
            },
            _ => {},
        }
    }

    pub fn on_key(&mut self, c: char) {
        match c {
            'q' => {
                self.should_quit = true;
            },
            '\t' => {
                self.tabs.next();
                return;
            },
            _ => {},
        }
        if self.tabs.index == 0 {
            match c {
                'p' => {
                    self.selected_tx_list = SelectedTransactionList::PendingTxs;
                    self.pending_txs.select_first();
                    self.completed_txs.unselect();
                },
                'c' => {
                    self.selected_tx_list = SelectedTransactionList::CompletedTxs;
                    self.pending_txs.unselect();
                    self.completed_txs.select_first();
                },
                '\n' => match self.selected_tx_list {
                    SelectedTransactionList::None => {},
                    SelectedTransactionList::PendingTxs => {
                        self.detailed_transaction = self.pending_txs.selected_item();
                    },
                    SelectedTransactionList::CompletedTxs => {
                        self.detailed_transaction = self.completed_txs.selected_item();
                    },
                },
                _ => {},
            }
        }
        if self.tabs.index == 1 {
            match self.send_input_mode {
                SendInputMode::None => match c {
                    'c' => self.show_contacts = !self.show_contacts,
                    't' => self.send_input_mode = SendInputMode::To,
                    'a' => self.send_input_mode = SendInputMode::Amount,
                    '\n' => {
                        if self.show_contacts {
                            if let Some(c) = self.contacts.selected_item().as_ref() {
                                self.to_field = c.public_key.clone();
                                self.show_contacts = false;
                            }
                        }
                    },
                    _ => {},
                },
                SendInputMode::To => match c {
                    '\n' | '\t' => {
                        self.send_input_mode = SendInputMode::None;
                        self.send_input_mode = SendInputMode::Amount;
                    },
                    c => {
                        self.to_field.push(c);
                    },
                },
                SendInputMode::Amount => match c {
                    '\n' | '\t' => self.send_input_mode = SendInputMode::None,
                    c => {
                        if c.is_numeric() {
                            self.amount_field.push(c);
                        }
                    },
                },
            }
        }
    }

    pub fn on_up(&mut self) {
        if self.tabs.index == 0 {
            match self.selected_tx_list {
                SelectedTransactionList::None => {},
                SelectedTransactionList::PendingTxs => {
                    self.pending_txs.previous();
                    self.detailed_transaction = self.pending_txs.selected_item();
                },
                SelectedTransactionList::CompletedTxs => {
                    self.completed_txs.previous();
                    self.detailed_transaction = self.completed_txs.selected_item();
                },
            }
        }
        if self.tabs.index == 1 {
            self.contacts.previous();
        }
    }

    pub fn on_down(&mut self) {
        if self.tabs.index == 0 {
            match self.selected_tx_list {
                SelectedTransactionList::None => {},
                SelectedTransactionList::PendingTxs => {
                    self.pending_txs.next();
                    self.detailed_transaction = self.pending_txs.selected_item();
                },
                SelectedTransactionList::CompletedTxs => {
                    self.completed_txs.next();
                    self.detailed_transaction = self.completed_txs.selected_item();
                },
            }
        }
        if self.tabs.index == 1 {
            self.contacts.next();
        }
    }

    pub fn on_right(&mut self) {
        self.tabs.next();
    }

    pub fn on_left(&mut self) {
        self.tabs.previous();
    }

    pub fn on_esc(&mut self) {
        if self.tabs.index == 1 {
            self.send_input_mode = SendInputMode::None;
            self.show_contacts = false;
        }
    }

    pub fn on_backspace(&mut self) {
        if self.tabs.index == 1 {
            match self.send_input_mode {
                SendInputMode::To => {
                    let _ = self.to_field.pop();
                },
                SendInputMode::Amount => {
                    let _ = self.amount_field.pop();
                },
                SendInputMode::None => {},
            }
        }
    }

    pub fn on_tick(&mut self) {}
}

#[derive(PartialEq)]
pub enum SelectedTransactionList {
    None,
    PendingTxs,
    CompletedTxs,
}

pub enum SendInputMode {
    None,
    To,
    Amount,
}

pub struct MyIdentity<'a> {
    pub public_key: &'a str,
    pub public_address: &'a str,
    pub emoji_id: &'a str,
    pub qr_code: &'a str,
}

#[derive(Clone)]
pub struct UiContact {
    pub alias: String,
    pub public_key: String,
    pub emoji_id: String,
}

impl From<Contact> for UiContact {
    fn from(c: Contact) -> Self {
        Self {
            alias: c.alias,
            public_key: format!("{}", c.public_key),
            emoji_id: EmojiId::from_pubkey(&c.public_key).as_str().to_string(),
        }
    }
}
