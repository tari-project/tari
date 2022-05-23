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

  static createFromObject(features_object) {
    // For convenience, default any value that is not specified in parameters
    const default_features = {
      version: 0,
      flags: 0,
      maturity: 0,
      recovery_byte: 0,
      metadata: "0",
      unique_id: null, 
      parent_public_key: null
    };
    let f = { ...default_features, ...features_object };
    let metadata = ByteVector.fromBytes(f.metadata).getPtr();

    let result = new OutputFeatures();
    result.pointerAssign(InterfaceFFI.outputFeaturesCreateFromBytes(
      f.version,
      f.flags,
      f.maturity,
      f.recovery_byte,
      metadata,
      f.unique_id,
      f.parent_public_key
    ));
    return result;
  }

  destroy() {
    if (this.ptr) {
      InterfaceFFI.outputFeaturesDestroy(this.ptr);
      this.ptr = undefined; //prevent double free segfault
    }
  }
}

module.exports = OutputFeatures;
