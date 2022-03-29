// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

const Contact = require("./contact");
const InterfaceFFI = require("./ffiInterface");

class Contacts {
  ptr;

  constructor(ptr) {
    this.ptr = ptr;
  }

  getLength() {
    return InterfaceFFI.contactsGetLength(this.ptr);
  }

  getAt(position) {
    let result = new Contact();
    result.pointerAssign(InterfaceFFI.contactsGetAt(this.ptr, position));
    return result;
  }

  destroy() {
    if (this.ptr) {
      InterfaceFFI.contactsDestroy(this.ptr);
      this.ptr = undefined; //prevent double free segfault
    }
  }
}

module.exports = Contacts;
