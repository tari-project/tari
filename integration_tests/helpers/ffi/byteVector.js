// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

const InterfaceFFI = require("./ffiInterface");

class ByteVector {
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

  fromBytes(input) {
    let buf = Buffer.from(input, "utf-8"); // ensure encoding is utf=8, js default is utf-16
    let len = buf.length; // get the length
    let result = new ByteVector();
    result.pointerAssign(InterfaceFFI.byteVectorCreate(buf, len));
    return result;
  }

  getBytes() {
    let result = [];
    for (let i = 0; i < this.getLength(); i++) {
      result.push(this.getAt(i));
    }
    return result;
  }

  getLength() {
    return InterfaceFFI.byteVectorGetLength(this.ptr);
  }

  getAt(position) {
    return InterfaceFFI.byteVectorGetAt(this.ptr, position);
  }

  getPtr() {
    return this.ptr;
  }

  destroy() {
    if (this.ptr) {
      InterfaceFFI.byteVectorDestroy(this.ptr);
      this.ptr = undefined; //prevent double free segfault
    }
  }
}

module.exports = ByteVector;
