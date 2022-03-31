// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

const PendingInboundTransaction = require("./pendingInboundTransaction");
const InterfaceFFI = require("./ffiInterface");

class PendingInboundTransactions {
  ptr;

  constructor(ptr) {
    this.ptr = ptr;
  }

  getLength() {
    return InterfaceFFI.pendingInboundTransactionsGetLength(this.ptr);
  }

  getAt(position) {
    let result = new PendingInboundTransaction();
    result.pointerAssign(
      InterfaceFFI.pendingInboundTransactionsGetAt(this.ptr, position)
    );
    return result;
  }

  destroy() {
    if (this.ptr) {
      InterfaceFFI.pendingInboundTransactionsDestroy(this.ptr);
      this.ptr = undefined; //prevent double free segfault
    }
  }
}

module.exports = PendingInboundTransactions;
