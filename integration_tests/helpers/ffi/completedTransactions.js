const CompletedTransaction = require("./completedTransaction");
const WalletFFI = require("./walletFFI");

class CompletedTransactions {
  #tari_completed_transactions_ptr;

  constructor(tari_completed_transactions_ptr) {
    this.#tari_completed_transactions_ptr = tari_completed_transactions_ptr;
  }

  static async fromWallet(wallet) {
    return new CompletedTransactions(
      await WalletFFI.walletGetCompletedTransactions(wallet)
    );
  }

  getLength() {
    return WalletFFI.completedTransactionsGetLength(
      this.#tari_completed_transactions_ptr
    );
  }

  async getAt(position) {
    return new CompletedTransaction(
      await WalletFFI.completedTransactionsGetAt(
        this.#tari_completed_transactions_ptr,
        position
      )
    );
  }

  destroy() {
    return WalletFFI.completedTransactionsDestroy(
      this.#tari_completed_transactions_ptr
    );
  }
}

module.exports = CompletedTransactions;
