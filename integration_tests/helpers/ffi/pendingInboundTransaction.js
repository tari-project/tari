// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

const InterfaceFFI = require("./ffiInterface");
const PublicKey = require("./publicKey");

class PendingInboundTransaction {
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

  getSourcePublicKey() {
    let result = new PublicKey();
    result.pointerAssign(
      InterfaceFFI.pendingInboundTransactionGetSourcePublicKey(this.ptr)
    );
    return result;
  }

  getAmount() {
    return InterfaceFFI.pendingInboundTransactionGetAmount(this.ptr);
  }

  getMessage() {
    return InterfaceFFI.pendingInboundTransactionGetMessage(this.ptr);
  }

  getStatus() {
    return InterfaceFFI.pendingInboundTransactionGetStatus(this.ptr);
  }

  getTransactionID() {
    return InterfaceFFI.pendingInboundTransactionGetTransactionId(this.ptr);
  }

  getTimestamp() {
    return InterfaceFFI.pendingInboundTransactionGetTimestamp(this.ptr);
  }

  destroy() {
    if (this.ptr) {
      InterfaceFFI.pendingInboundTransactionDestroy(this.ptr);
      this.ptr = undefined; //prevent double free segfault
    }
  }
}

module.exports = PendingInboundTransaction;
