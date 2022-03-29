// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

const InterfaceFFI = require("./ffiInterface");
const PublicKey = require("./publicKey");

class PendingOutboundTransaction {
  ptr;

  pointerAssign(ptr) {
    if (this.ptr) {
      this.ptr = ptr;
      this.destroy();
    } else {
      this.ptr = ptr;
    }
  }

  getPtr() {
    return this.ptr;
  }

  getDestinationPublicKey() {
    let result = new PublicKey();
    result.pointerAssign(
      InterfaceFFI.pendingOutboundTransactionGetDestinationPublicKey(this.ptr)
    );
    return result;
  }

  getAmount() {
    return InterfaceFFI.pendingOutboundTransactionGetAmount(this.ptr);
  }

  getFee() {
    return InterfaceFFI.pendingOutboundTransactionGetFee(this.ptr);
  }

  getMessage() {
    return InterfaceFFI.pendingOutboundTransactionGetMessage(this.ptr);
  }

  getStatus() {
    return InterfaceFFI.pendingOutboundTransactionGetStatus(this.ptr);
  }

  getTransactionID() {
    return InterfaceFFI.pendingOutboundTransactionGetTransactionId(this.ptr);
  }

  getTimestamp() {
    return InterfaceFFI.pendingOutboundTransactionGetTimestamp(this.ptr);
  }

  destroy() {
    if (this.ptr) {
      InterfaceFFI.pendingOutboundTransactionDestroy(this.ptr);
      this.ptr = undefined; //prevent double free segfault
    }
  }
}

module.exports = PendingOutboundTransaction;
