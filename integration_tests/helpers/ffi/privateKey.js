// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

const InterfaceFFI = require("./ffiInterface");
const ByteVector = require("./byteVector");
const utf8 = require("utf8");

class PrivateKey {
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

  generate() {
    this.ptr = InterfaceFFI.privateKeyGenerate();
  }

  fromHexString(hex) {
    let sanitize = utf8.encode(hex); // Make sure it's not UTF-16 encoded (JS default)
    let result = new PrivateKey();
    result.pointerAssign(InterfaceFFI.privateKeyFromHex(sanitize));
    return result;
  }

  fromByteVector(byte_vector) {
    let result = new PrivateKey();
    result.pointerAssign(InterfaceFFI.privateKeyCreate(byte_vector.getPtr()));
    return result;
  }

  getPtr() {
    return this.ptr;
  }

  getBytes() {
    let result = new ByteVector();
    result.pointerAssign(InterfaceFFI.privateKeyGetBytes(this.ptr));
    return result;
  }

  getHex() {
    const bytes = this.getBytes();
    const length = bytes.getLength();
    let byte_array = new Uint8Array(length);
    for (let i = 0; i < length; i++) {
      byte_array[i] = bytes.getAt(i);
    }
    bytes.destroy();
    let buffer = Buffer.from(byte_array, 0);
    return buffer.toString("hex");
  }

  destroy() {
    if (this.ptr) {
      InterfaceFFI.privateKeyDestroy(this.ptr);
      this.ptr = undefined; //prevent double free segfault
    }
  }
}

module.exports = PrivateKey;
