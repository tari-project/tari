const InterfaceFFI = require("./ffiInterface");
const PublicKey = require("./publicKey");

class PendingInboundTransaction {
  #tari_pending_inbound_transaction_ptr;

  pointerAssign(ptr) {
    if (this.#tari_pending_inbound_transaction_ptr) {
      this.destroy();
      this.#tari_pending_inbound_transaction_ptr = ptr;
    } else {
      this.#tari_pending_inbound_transaction_ptr = ptr;
    }
  }

  getPtr() {
    return this.#tari_pending_inbound_transaction_ptr;
  }

  getSourcePublicKey() {
    let result = new PublicKey();
    result.pointerAssign(
      InterfaceFFI.pendingInboundTransactionGetSourcePublicKey(
        this.#tari_pending_inbound_transaction_ptr
      )
    );
    return result;
  }

  getAmount() {
    return InterfaceFFI.pendingInboundTransactionGetAmount(
      this.#tari_pending_inbound_transaction_ptr
    );
  }

  getMessage() {
    return InterfaceFFI.pendingInboundTransactionGetMessage(
      this.#tari_pending_inbound_transaction_ptr
    );
  }

  getStatus() {
    return InterfaceFFI.pendingInboundTransactionGetStatus(
      this.#tari_pending_inbound_transaction_ptr
    );
  }

  getTransactionID() {
    return InterfaceFFI.pendingInboundTransactionGetTransactionId(
      this.#tari_pending_inbound_transaction_ptr
    );
  }

  getTimestamp() {
    return InterfaceFFI.pendingInboundTransactionGetTimestamp(
      this.#tari_pending_inbound_transaction_ptr
    );
  }

  destroy() {
    if (this.#tari_pending_inbound_transaction_ptr) {
      InterfaceFFI.pendingInboundTransactionDestroy(
        this.#tari_pending_inbound_transaction_ptr
      );
      this.#tari_pending_inbound_transaction_ptr = undefined; //prevent double free segfault
    }
  }
}

module.exports = PendingInboundTransaction;
