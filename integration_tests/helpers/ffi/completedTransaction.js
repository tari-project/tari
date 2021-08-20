const WalletFFI = require("./walletFFI");

class CompletedTransaction {
  #tari_completed_transaction_ptr;

  constructor(tari_completed_transaction_ptr) {
    this.#tari_completed_transaction_ptr = tari_completed_transaction_ptr;
  }

  isOutbound() {
    return WalletFFI.completedTransactionIsOutbound(
      this.#tari_completed_transaction_ptr
    );
  }

  destroy() {
    return WalletFFI.completedTransactionDestroy(
      this.#tari_completed_transaction_ptr
    );
  }
}

module.exports = CompletedTransaction;
