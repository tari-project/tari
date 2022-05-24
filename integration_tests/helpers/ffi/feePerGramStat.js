// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

const InterfaceFFI = require("./ffiInterface");

class FeePerGramStat {
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

  getOrder() {
    return InterfaceFFI.feePerGramStatGetOrder(this.ptr);
  }

  getMinFeePerGram() {
    return InterfaceFFI.feePerGramStatGetMinFeePerGram(this.ptr);
  }

  getAvgFeePerGram() {
    return InterfaceFFI.feePerGramStatGetAvgFeePerGram(this.ptr);
  }

  getMaxFeePerGram() {
    return InterfaceFFI.feePerGramStatGetMaxFeePerGram(this.ptr);
  }

  destroy() {
    if (this.ptr) {
      InterfaceFFI.feePerGramStatDestroy(this.ptr);
      this.ptr = undefined; //prevent double free segfault
    }
  }
}

module.exports = FeePerGramStat;
