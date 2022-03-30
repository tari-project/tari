// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

const InterfaceFFI = require("./ffiInterface");
const ByteVector = require("./byteVector");
const utf8 = require("utf8");

class PublicKey {
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

  fromPrivateKey(key) {
    let result = new PublicKey();
    result.pointerAssign(InterfaceFFI.publicKeyFromPrivateKey(key.getPtr()));
    return result;
  }

  static fromHexString(hex) {
    let sanitize = utf8.encode(hex); // Make sure it's not UTF-16 encoded (JS default)
    let result = new PublicKey();
    result.pointerAssign(InterfaceFFI.publicKeyFromHex(sanitize));
    return result;
  }

  fromEmojiID(emoji) {
    let sanitize = utf8.encode(emoji); // Make sure it's not UTF-16 encoded (JS default)
    let result = new PublicKey();
    result.pointerAssign(InterfaceFFI.emojiIdToPublicKey(sanitize));
    return result;
  }

  fromByteVector(byte_vector) {
    let result = new PublicKey();
    result.pointerAssign(InterfaceFFI.publicKeyCreate(byte_vector.getPtr()));
    return result;
  }

  getPtr() {
    return this.ptr;
  }

  getBytes() {
    let result = new ByteVector();
    result.pointerAssign(InterfaceFFI.publicKeyGetBytes(this.ptr));
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

  getEmojiId() {
    let emoji_id = InterfaceFFI.publicKeyToEmojiId(this.ptr);
    let result = emoji_id.readCString();
    InterfaceFFI.stringDestroy(emoji_id);
    return result;
  }

  destroy() {
    if (this.ptr) {
      InterfaceFFI.publicKeyDestroy(this.ptr);
      this.ptr = undefined; //prevent double free segfault
    }
  }
}

module.exports = PublicKey;
