// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

const InterfaceFFI = require("./ffiInterface");

class CompletedTransactionKernel {
  ptr;

  pointerAssign(ptr) {
    // Prevent pointer from being leaked in case of re-assignment
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

  getExcess() {
    let strPtr = InterfaceFFI.transactionKernelGetExcess(this.ptr);
    let result = strPtr.readCString();
    InterfaceFFI.stringDestroy(strPtr);
    return result;
  }

  getNonce() {
    let strPtr = InterfaceFFI.transactionKernelGetExcessPublicNonce(this.ptr);
    let result = strPtr.readCString();
    InterfaceFFI.stringDestroy(strPtr);
    return result;
  }

  getSignature() {
    let strPtr = InterfaceFFI.transactionKernelGetExcessSigntature(this.ptr);
    let result = strPtr.readCString();
    InterfaceFFI.stringDestroy(strPtr);
    return result;
  }

  asObject() {
    return {
      excess: this.getExcess(),
      nonce: this.getNonce(),
      sig: this.getSignature(),
    };
  }

  destroy() {
    if (this.ptr) {
      InterfaceFFI.transactionKernelDestroy(this.ptr);
      this.ptr = undefined; //prevent double free segfault
    }
  }
}

module.exports = CompletedTransactionKernel;
