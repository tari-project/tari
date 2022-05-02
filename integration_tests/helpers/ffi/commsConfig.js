// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

const InterfaceFFI = require("./ffiInterface");
const utf8 = require("utf8");

class CommsConfig {
  ptr;

  constructor(
    public_address,
    transport_ptr,
    database_name,
    datastore_path,
    discovery_timeout_in_secs,
    saf_message_duration_in_secs
  ) {
    let sanitize_address = utf8.encode(public_address);
    let sanitize_db_name = utf8.encode(database_name);
    let sanitize_db_path = utf8.encode(datastore_path);
    this.ptr = InterfaceFFI.commsConfigCreate(
      sanitize_address,
      transport_ptr,
      sanitize_db_name,
      sanitize_db_path,
      discovery_timeout_in_secs,
      saf_message_duration_in_secs
    );
  }

  getPtr() {
    return this.ptr;
  }

  destroy() {
    if (this.ptr) {
      InterfaceFFI.commsConfigDestroy(this.ptr);
      this.ptr = undefined; //prevent double free segfault
    }
  }
}

module.exports = CommsConfig;
