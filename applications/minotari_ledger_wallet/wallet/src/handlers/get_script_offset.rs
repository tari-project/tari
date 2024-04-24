// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use ledger_device_sdk::{io::Comm, ui::gadgets::SingleMessage};

use crate::{alloc::string::ToString, AppSW, RESPONSE_VERSION};

pub fn handler_get_script_offset(comm: &mut Comm) -> Result<(), AppSW> {
    let data = comm.get_data().map_err(|_| AppSW::WrongApduLength)?;

    SingleMessage::new(&"Finished".to_string()).show_and_wait();

    comm.append(&[RESPONSE_VERSION]); // version
    comm.reply_ok();

    Ok(())
}
