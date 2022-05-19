// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

const InterfaceFFI = require("./ffiInterface");
const ByteVector = require("./byteVector");

class Covenant {
  ptr;
  pointerAssign(ptr) {
    // Prevent pointer from being leaked in case of re-assignment
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

  static createFromBytes(covenant_bytes) {
    let result = new Covenant();
    let byte_vector = ByteVector.fromBytes(covenant_bytes).getPtr();
    result.pointerAssign(InterfaceFFI.covenantCreateFromBytes(byte_vector));
    return result;
  }

  destroy() {
    if (this.ptr) {
      InterfaceFFI.covenantDestroy(this.ptr);
      this.ptr = undefined; //prevent double free segfault
    }
  }
}

module.exports = Covenant;
