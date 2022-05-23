// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

const FeePerGramStat = require("./feePerGramStat");
const InterfaceFFI = require("./ffiInterface");

class FeePerGramStats {
  ptr;

  constructor(ptr) {
    this.ptr = ptr;
  }

  getLength() {
    return InterfaceFFI.feePerGramStatsGetLength(this.ptr);
  }

  getAt(position) {
    let result = new FeePerGramStat();
    result.pointerAssign(InterfaceFFI.feePerGramStatsGetAt(this.ptr, position));
    return result;
  }

  destroy() {
    if (this.ptr) {
      InterfaceFFI.feePerGramStatsDestroy(this.ptr);
      this.ptr = undefined; //prevent double free segfault
    }
  }
}

module.exports = FeePerGramStats;
