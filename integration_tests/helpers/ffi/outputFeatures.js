// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

const InterfaceFFI = require("./ffiInterface");
const ByteVector = require("./byteVector");

class OutputFeatures {
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

  static createFromBytes() {
    let result = new OutputFeatures();

    let version = 0;
    let flags = 0;
    let maturity = 0;
    let recovery_byte = 0;

    let metadata = ByteVector.fromBytes("0000").getPtr();

    let unique_id = null; 
    let parent_public_key = null;

    result.pointerAssign(InterfaceFFI.outputFeaturesCreateFromBytes(
      version,
      flags,
      maturity,
      recovery_byte,
      metadata,
      unique_id,
      parent_public_key
    ));
    return result;
  }

  destroy() {
    if (this.ptr) {
      InterfaceFFI.destroyOutputFeatures(this.ptr);
      this.ptr = undefined; //prevent double free segfault
    }
  }
}

module.exports = OutputFeatures;
