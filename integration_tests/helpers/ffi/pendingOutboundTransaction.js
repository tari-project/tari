const WalletFFI = require("./walletFFI");

class PendingOutboundTransaction {
  #tari_pending_outbound_transaction_ptr;

  constructor(tari_pending_outbound_transaction_ptr) {
    this.#tari_pending_outbound_transaction_ptr =
      tari_pending_outbound_transaction_ptr;
  }

  getTransactionId() {
    return WalletFFI.pendingOutboundTransactionGetTransactionId(
      this.#tari_pending_outbound_transaction_ptr
    );
  }

  getStatus() {
    return WalletFFI.pendingOutboundTransactionGetStatus(
      this.#tari_pending_outbound_transaction_ptr
    );
  }

  destroy() {
    return WalletFFI.pendingOutboundTransactionDestroy(
      this.#tari_pending_outbound_transaction_ptr
    );
  }
}

module.exports = PendingOutboundTransaction;
