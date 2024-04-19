// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use include_gif::include_gif;
use ledger_device_sdk::{
    io::{Comm, Event},
    ui::{
        bitmaps::{Glyph, BACK, CERTIFICATE, DASHBOARD_X},
        gadgets::{EventOrPageIndex, MultiPageMenu, Page},
    },
};

use crate::Instruction;

fn ui_about_menu(comm: &mut Comm) -> Event<Instruction> {
    let pages = [
        &Page::from((["MinoTari Wallet", "(c) 2024 The Tari Project"], true)),
        &Page::from(("Back", &BACK)),
    ];
    loop {
        match MultiPageMenu::new(comm, &pages).show() {
            EventOrPageIndex::Event(e) => return e,
            EventOrPageIndex::Index(1) => return ui_menu_main(comm),
            EventOrPageIndex::Index(_) => (),
        }
    }
}

pub fn ui_menu_main(comm: &mut Comm) -> Event<Instruction> {
    const APP_ICON: Glyph = Glyph::from_include(include_gif!("key.gif"));
    let pages = [
        // The from trait allows to create different styles of pages
        // without having to use the new() function.
        &Page::from((["MinoTari", "Wallet"], &APP_ICON)),
        &Page::from((["Version", env!("CARGO_PKG_VERSION")], true)),
        &Page::from(("About", &CERTIFICATE)),
        &Page::from(("Quit", &DASHBOARD_X)),
    ];
    loop {
        match MultiPageMenu::new(comm, &pages).show() {
            EventOrPageIndex::Event(e) => return e,
            EventOrPageIndex::Index(2) => return ui_about_menu(comm),
            EventOrPageIndex::Index(3) => ledger_device_sdk::exit_app(0),
            EventOrPageIndex::Index(_) => (),
        }
    }
}
