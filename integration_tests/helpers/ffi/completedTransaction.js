const InterfaceFFI = require("./ffiInterface");
const PublicKey = require("./publicKey");

class CompletedTransaction {
  #tari_completed_transaction_ptr;

  pointerAssign(ptr) {
    if (this.#tari_completed_transaction_ptr) {
      this.destroy();
      this.#tari_completed_transaction_ptr = ptr;
    } else {
      this.#tari_completed_transaction_ptr = ptr;
    }
  }

  getPtr() {
    return this.#tari_completed_transaction_ptr;
  }

  isOutbound() {
    return InterfaceFFI.completedTransactionIsOutbound(
      this.#tari_completed_transaction_ptr
    );
  }

  getDestinationPublicKey() {
    let result = new PublicKey();
    result.pointerAssign(
      InterfaceFFI.completedTransactionGetDestinationPublicKey(
        this.#tari_completed_transaction_ptr
      )
    );
    return result;
  }

  getSourcePublicKey() {
    let result = new PublicKey();
    result.pointerAssign(
      InterfaceFFI.completedTransactionGetSourcePublicKey(
        this.#tari_completed_transaction_ptr
      )
    );
    return result;
  }

  getAmount() {
    return InterfaceFFI.completedTransactionGetAmount(
      this.#tari_completed_transaction_ptr
    );
  }

  getFee() {
    return InterfaceFFI.completedTransactionGetFee(
      this.#tari_completed_transaction_ptr
    );
  }

  getMessage() {
    return InterfaceFFI.completedTransactionGetMessage(
      this.#tari_completed_transaction_ptr
    );
  }

  getStatus() {
    return InterfaceFFI.completedTransactionGetStatus(
      this.#tari_completed_transaction_ptr
    );
  }

  getTransactionID() {
    return InterfaceFFI.completedTransactionGetTransactionId(
      this.#tari_completed_transaction_ptr
    );
  }

  getTimestamp() {
    return InterfaceFFI.completedTransactionGetTimestamp(
      this.#tari_completed_transaction_ptr
    );
  }

  isValid() {
    return InterfaceFFI.completedTransactionIsValid(
      this.#tari_completed_transaction_ptr
    );
  }

  getConfirmations() {
    return InterfaceFFI.completedTransactionGetConfirmations(
      this.#tari_completed_transaction_ptr
    );
  }

  destroy() {
    if (this.#tari_completed_transaction_ptr) {
      InterfaceFFI.completedTransactionDestroy(
        this.#tari_completed_transaction_ptr
      );
      this.#tari_completed_transaction_ptr = undefined; //prevent double free segfault
    }
  }
}

module.exports = CompletedTransaction;
