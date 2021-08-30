const PendingOutboundTransaction = require("./pendingOutboundTransaction");
const InterfaceFFI = require("./ffiInterface");

class PendingOutboundTransactions {
  #tari_pending_outbound_transactions_ptr;

  constructor(ptr) {
    this.#tari_pending_outbound_transactions_ptr = ptr;
  }

  getLength() {
    return InterfaceFFI.pendingOutboundTransactionsGetLength(
      this.#tari_pending_outbound_transactions_ptr
    );
  }

  getAt(position) {
    let result = new PendingOutboundTransaction();
    result.pointerAssign(
      InterfaceFFI.pendingOutboundTransactionsGetAt(
        this.#tari_pending_outbound_transactions_ptr,
        position
      )
    );
    return result;
  }

  destroy() {
    if (this.#tari_pending_outbound_transactions_ptr) {
      InterfaceFFI.pendingOutboundTransactionsDestroy(
        this.#tari_pending_outbound_transactions_ptr
      );
      this.#tari_pending_outbound_transactions_ptr = undefined; //prevent double free segfault
    }
  }
}

module.exports = PendingOutboundTransactions;
