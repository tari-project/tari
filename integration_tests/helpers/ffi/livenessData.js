// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

const InterfaceFFI = require("./ffiInterface");
const PublicKey = require("./publicKey");
const { expect } = require("chai");

class LivenessData {
  ptr;

  pointerAssign(ptr) {
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

  getPublicKey() {
    let ptr = InterfaceFFI.livenessDataGetPublicKey(this.ptr);
    let pk = new PublicKey();
    pk.pointerAssign(ptr);
    let result = pk.getHex();
    pk.destroy();
    return result;
  }

  getLatency() {
    return InterfaceFFI.livenessDataGetLatency(this.ptr);
  }

  getLastSeen() {
    let lastSeen = InterfaceFFI.livenessDataGetLastSeen(this.ptr);
    let result = lastSeen.readCString();
    InterfaceFFI.stringDestroy(lastSeen);
    return result;
  }

  getMessageType() {
    let messageType = InterfaceFFI.livenessDataGetMessageType(this.ptr);
    switch (messageType) {
      case 0:
        return "Ping";
      case 1:
        return "Pong";
      case 2:
        return "NoMessage";
      default:
        expect(messageType).to.equal("please add this<< MessageType");
    }
  }

  getOnlineStatus() {
    let status = InterfaceFFI.livenessDataGetOnlineStatus(this.ptr);
    const result = status.readCString();
    InterfaceFFI.stringDestroy(status);
    return result;
  }

  destroy() {
    if (this.ptr) {
      InterfaceFFI.livenessDataDestroy(this.ptr);
      this.ptr = undefined; //prevent double free segfault
    }
  }
}

module.exports = LivenessData;
