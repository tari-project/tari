const WalletFFI = require("./walletFFI");

class PendingInboundTransaction {
  #tari_pending_inbound_transaction_ptr;

  constructor(tari_pending_inbound_transaction_ptr) {
    this.#tari_pending_inbound_transaction_ptr =
      tari_pending_inbound_transaction_ptr;
  }

  getStatus() {
    return WalletFFI.pendingInboundTransactionGetStatus(
      this.#tari_pending_inbound_transaction_ptr
    );
  }

  destroy() {
    return WalletFFI.pendingInboundTransactionDestroy(
      this.#tari_pending_inbound_transaction_ptr
    );
  }
}

module.exports = PendingInboundTransaction;
