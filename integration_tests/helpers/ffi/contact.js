// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

const PublicKey = require("./publicKey");
const InterfaceFFI = require("./ffiInterface");

class Contact {
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

  getPtr() {
    return this.ptr;
  }

  getAlias() {
    let alias = InterfaceFFI.contactGetAlias(this.ptr);
    let result = alias.readCString();
    InterfaceFFI.stringDestroy(alias);
    return result;
  }

  getPubkey() {
    let result = new PublicKey();
    result.pointerAssign(InterfaceFFI.contactGetPublicKey(this.ptr));
    return result;
  }

  getPubkeyHex() {
    let result = "";
    let pk = new PublicKey();
    pk.pointerAssign(InterfaceFFI.contactGetPublicKey(this.ptr));
    result = pk.getHex();
    pk.destroy();
    return result;
  }

  destroy() {
    if (this.ptr) {
      InterfaceFFI.contactDestroy(this.ptr);
      this.ptr = undefined; //prevent double free segfault
    }
  }
}

module.exports = Contact;
