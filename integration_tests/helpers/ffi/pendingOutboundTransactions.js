// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

const PendingOutboundTransaction = require("./pendingOutboundTransaction");
const InterfaceFFI = require("./ffiInterface");

class PendingOutboundTransactions {
  ptr = undefined;

  constructor(ptr) {
    this.ptr = ptr;
  }

  getLength() {
    return InterfaceFFI.pendingOutboundTransactionsGetLength(this.ptr);
  }

  getAt(position) {
    let result = new PendingOutboundTransaction();
    result.pointerAssign(
      InterfaceFFI.pendingOutboundTransactionsGetAt(this.ptr, position)
    );
    return result;
  }

  destroy() {
    if (this.ptr) {
      InterfaceFFI.pendingOutboundTransactionsDestroy(this.ptr);
      this.ptr = undefined; //prevent double free segfault
    }
  }
}

module.exports = PendingOutboundTransactions;
