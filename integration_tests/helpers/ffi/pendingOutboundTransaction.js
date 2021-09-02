const InterfaceFFI = require("./ffiInterface");
const PublicKey = require("./publicKey");

class PendingOutboundTransaction {
  #tari_pending_outbound_transaction_ptr;

  pointerAssign(ptr) {
    if (this.#tari_pending_outbound_transaction_ptr) {
      this.#tari_pending_outbound_transaction_ptr = ptr;
      this.destroy();
    } else {
      this.#tari_pending_outbound_transaction_ptr = ptr;
    }
  }

  getPtr() {
    return this.#tari_pending_outbound_transaction_ptr;
  }

  getDestinationPublicKey() {
    let result = new PublicKey();
    result.pointerAssign(
      InterfaceFFI.pendingOutboundTransactionGetDestinationPublicKey(
        this.#tari_pending_outbound_transaction_ptr
      )
    );
    return result;
  }

  getAmount() {
    return InterfaceFFI.pendingOutboundTransactionGetAmount(
      this.#tari_pending_outbound_transaction_ptr
    );
  }

  getFee() {
    return InterfaceFFI.pendingOutboundTransactionGetFee(
      this.#tari_pending_outbound_transaction_ptr
    );
  }

  getMessage() {
    return InterfaceFFI.pendingOutboundTransactionGetMessage(
      this.#tari_pending_outbound_transaction_ptr
    );
  }

  getStatus() {
    return InterfaceFFI.pendingOutboundTransactionGetStatus(
      this.#tari_pending_outbound_transaction_ptr
    );
  }

  getTransactionID() {
    return InterfaceFFI.pendingOutboundTransactionGetTransactionId(
      this.#tari_pending_outbound_transaction_ptr
    );
  }

  getTimestamp() {
    return InterfaceFFI.pendingOutboundTransactionGetTimestamp(
      this.#tari_pending_outbound_transaction_ptr
    );
  }

  destroy() {
    if (this.#tari_pending_outbound_transaction_ptr) {
      InterfaceFFI.pendingOutboundTransactionDestroy(
        this.#tari_pending_outbound_transaction_ptr
      );
      this.#tari_pending_outbound_transaction_ptr = undefined; //prevent double free segfault
    }
  }
}

module.exports = PendingOutboundTransaction;
