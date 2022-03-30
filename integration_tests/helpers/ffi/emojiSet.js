// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

const InterfaceFFI = require("./ffiInterface");

class EmojiSet {
  ptr;

  constructor() {
    this.ptr = InterfaceFFI.getEmojiSet();
  }

  getLength() {
    return InterfaceFFI.emojiSetGetLength(this.ptr);
  }

  getAt(position) {
    return InterfaceFFI.emojiSetGetAt(this.ptr, position);
  }

  list() {
    let set = [];
    for (let i = 0; i < this.getLength(); i++) {
      let item = this.getAt(i);
      set.push(Buffer.from(item.getBytes(), "utf-8").toString());
      item.destroy();
    }
    return set;
  }

  destroy() {
    if (this.ptr) {
      InterfaceFFI.byteVectorDestroy(this.ptr);
      this.ptr = undefined; //prevent double free segfault
    }
  }
}

module.exports = EmojiSet;
