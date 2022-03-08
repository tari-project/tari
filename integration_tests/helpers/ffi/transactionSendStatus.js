const InterfaceFFI = require("./ffiInterface");

class TransactionSendStatus {
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

  getSendDirect() {
    return InterfaceFFI.transactionSendStatusGetDirect(this.ptr);
  }

  getSendSaf() {
    return InterfaceFFI.transactionSendStatusGetSaf(this.ptr);
  }

  getQueued() {
    return InterfaceFFI.transactionSendStatusGetQueued(this.ptr);
  }

  destroy() {
    if (this.ptr) {
      InterfaceFFI.transactionSendStatusDestroy(this.ptr);
      this.ptr = undefined; //prevent double free segfault
    }
  }
}

module.exports = TransactionSendStatus;
