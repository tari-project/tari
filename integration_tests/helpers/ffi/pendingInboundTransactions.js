const PendingInboundTransaction = require("./pendingInboundTransaction");
const InterfaceFFI = require("./ffiInterface");

class PendingInboundTransactions {
  #tari_pending_inbound_transactions_ptr;

  constructor(ptr) {
    this.#tari_pending_inbound_transactions_ptr = ptr;
  }

  getLength() {
    return InterfaceFFI.pendingInboundTransactionsGetLength(
      this.#tari_pending_inbound_transactions_ptr
    );
  }

  getAt(position) {
    let result = new PendingInboundTransaction();
    result.pointerAssign(
      InterfaceFFI.pendingInboundTransactionsGetAt(
        this.#tari_pending_inbound_transactions_ptr,
        position
      )
    );
    return result;
  }

  destroy() {
    if (this.#tari_pending_inbound_transactions_ptr) {
      InterfaceFFI.pendingInboundTransactionsDestroy(
        this.#tari_pending_inbound_transactions_ptr
      );
      this.#tari_pending_inbound_transactions_ptr = undefined; //prevent double free segfault
    }
  }
}

module.exports = PendingInboundTransactions;
