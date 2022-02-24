const InterfaceFFI = require("./ffiInterface");
const PublicKey = require("./publicKey");
const TransactionKernel = require("./transactionKernel");

class CompletedTransaction {
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

  isOutbound() {
    return InterfaceFFI.completedTransactionIsOutbound(this.ptr);
  }

  getDestinationPublicKey() {
    let result = new PublicKey();
    result.pointerAssign(
      InterfaceFFI.completedTransactionGetDestinationPublicKey(this.ptr)
    );
    return result;
  }

  getSourcePublicKey() {
    let result = new PublicKey();
    result.pointerAssign(
      InterfaceFFI.completedTransactionGetSourcePublicKey(this.ptr)
    );
    return result;
  }

  getAmount() {
    return InterfaceFFI.completedTransactionGetAmount(this.ptr);
  }

  getFee() {
    return InterfaceFFI.completedTransactionGetFee(this.ptr);
  }

  getMessage() {
    return InterfaceFFI.completedTransactionGetMessage(this.ptr);
  }

  getStatus() {
    return InterfaceFFI.completedTransactionGetStatus(this.ptr);
  }

  getTransactionID() {
    return InterfaceFFI.completedTransactionGetTransactionId(this.ptr);
  }

  getTimestamp() {
    return InterfaceFFI.completedTransactionGetTimestamp(this.ptr);
  }

  getConfirmations() {
    return InterfaceFFI.completedTransactionGetConfirmations(this.ptr);
  }

  getKernel() {
    let result = new TransactionKernel();
    result.pointerAssign(InterfaceFFI.completedTransactionGetKernel(this.ptr));
    return result;
  }

  destroy() {
    if (this.ptr) {
      InterfaceFFI.completedTransactionDestroy(this.ptr);
      this.ptr = undefined; //prevent double free segfault
    }
  }
}

module.exports = CompletedTransaction;
