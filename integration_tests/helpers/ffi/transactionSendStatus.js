// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

const InterfaceFFI = require("./ffiInterface");

class TransactionSendStatus {
  ptr;

  pointerAssign(ptr) {
    if (this.ptr) {
      this.destroy();
      this.ptr = ptr;
    } else {
      this.ptr = ptr;
    }
  }

  getPtr() {
    return this.ptr;
  }

  getSendStatus() {
    return InterfaceFFI.transactionSendStatusDecode(this.ptr);
  }

  destroy() {
    if (this.ptr) {
      InterfaceFFI.transactionSendStatusDestroy(this.ptr);
      this.ptr = undefined; //prevent double free segfault
    }
  }
}

module.exports = TransactionSendStatus;
