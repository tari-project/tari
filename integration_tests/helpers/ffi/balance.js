// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

const InterfaceFFI = require("./ffiInterface");

class Balance {
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

  getAvailable() {
    return InterfaceFFI.balanceGetAvailable(this.ptr);
  }

  getTimeLocked() {
    return InterfaceFFI.balanceGetTimeLocked(this.ptr);
  }

  getPendingIncoming() {
    return InterfaceFFI.balanceGetPendingIncoming(this.ptr);
  }

  getPendingOutgoing() {
    return InterfaceFFI.balanceGetPendingOutgoing(this.ptr);
  }

  destroy() {
    if (this.ptr) {
      InterfaceFFI.balanceDestroy(this.ptr);
      this.ptr = undefined; //prevent double free segfault
    }
  }
}

module.exports = Balance;
