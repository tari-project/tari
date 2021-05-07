const { Client } = require("wallet-grpc-client");

function transactionStatus() {
  return [
    "TRANSACTION_STATUS_IMPORTED",
    "TRANSACTION_STATUS_COINBASE",
    "TRANSACTION_STATUS_PENDING",
    "TRANSACTION_STATUS_COMPLETED",
    "TRANSACTION_STATUS_BROADCAST",
    "TRANSACTION_STATUS_MINED_UNCONFIRMED",
    "TRANSACTION_STATUS_MINED_CONFIRMED",
  ];
}

class WalletClient {
  constructor(walletAddress, name) {
    this.client = Client.connect(walletAddress);
    this.name = name;
  }

  async getVersion() {
    return await this.client.getVersion();
  }

  async getBalance() {
    return await this.client.getBalance();
  }

  async getCompletedTransactions() {
    const data = await this.client.getCompletedTransactions();
    const transactions = [];
    for (let i = 0; i < data.length; i++) {
      transactions.push({
        tx_id: data[i].transaction.tx_id,
        source_pk: data[i].transaction.source_pk.toString("hex"),
        dest_pk: data[i].transaction.dest_pk.toString("hex"),
        status: data[i].transaction.status,
        direction: data[i].transaction.direction,
        amount: data[i].transaction.amount,
        fee: data[i].transaction.fee,
        is_cancelled: data[i].transaction.is_cancelled,
        excess_sig: data[i].transaction.excess_sig.toString("hex"),
        timestamp: new Date(
          Number(data[i].transaction.timestamp.seconds) * 1000
        ),
        message: data[i].transaction.message,
        valid: data[i].transaction.valid,
      });
    }
    return transactions;
  }

  async getAllCoinbaseTransactions() {
    const data = await this.getCompletedTransactions();
    const transactions = [];
    for (let i = 0; i < data.length; i++) {
      if (
        data[i].message.includes("Coinbase Transaction for Block ") &&
        data[i].fee == 0
      ) {
        transactions.push(data[i]);
      }
    }
    return transactions;
  }

  async getAllSpendableCoinbaseTransactions() {
    const data = await this.getAllCoinbaseTransactions();
    const transactions = [];
    for (let i = 0; i < data.length; i++) {
      if (
        transactionStatus().indexOf(data[i].status) == 6 &&
        data[i].valid == true
      ) {
        transactions.push(data[i]);
      }
    }
    return transactions;
  }

  async areCoinbasesConfirmedAtLeast(number) {
    const data = await this.getAllSpendableCoinbaseTransactions();
    if (data.length >= number) {
      return true;
    } else {
      return false;
    }
  }

  async getAllNormalTransactions() {
    const data = this.getCompletedTransactions();
    const transactions = [];
    for (let i = 0; i < data.length; i++) {
      if (
        !(
          data[i].message.includes("Coinbase Transaction for Block ") &&
          data[i].fee == 0
        )
      ) {
        transactions.push(data[i]);
      }
    }
    return transactions;
  }

  async transfer(args) {
    return await this.client.transfer(args);
  }

  async getTransactionInfo(args) {
    return await this.client.getTransactionInfo(args);
  }

  async identify(args) {
    const info = await this.client.identify(args);
    return {
      public_key: info.public_key.toString("utf8"),
      public_address: info.public_address,
      node_id: info.node_id.toString("utf8"),
    };
  }

  async isTransactionRegistered(tx_id) {
    try {
      await this.getTransactionInfo({
        transaction_ids: [tx_id.toString()],
      });
      return true;
    } catch (err) {
      // Any error here must be treated as if the required status was not achieved
      return false;
    }
  }

  async isBalanceAtLeast(amount) {
    try {
      const balance = await this.getBalance();
      if (balance.available_balance >= amount) {
        return true;
      } else {
        return false;
      }
    } catch (err) {
      // Any error here must be treated as if the required status was not achieved
      return false;
    }
  }

  async isTransactionAtLeastPending(tx_id) {
    try {
      const txnDetails = await this.getTransactionInfo({
        transaction_ids: [tx_id.toString()],
      });
      if (transactionStatus().indexOf(txnDetails.transactions[0].status) >= 2) {
        return true;
      } else {
        return false;
      }
    } catch (err) {
      // Any error here must be treated as if the required status was not achieved
      return false;
    }
  }

  async isTransactionAtLeastCompleted(tx_id) {
    try {
      const txnDetails = await this.getTransactionInfo({
        transaction_ids: [tx_id.toString()],
      });
      if (transactionStatus().indexOf(txnDetails.transactions[0].status) >= 3) {
        return true;
      } else {
        return false;
      }
    } catch (err) {
      // Any error here must be treated as if the required status was not achieved
      return false;
    }
  }

  async isTransactionAtLeastBroadcast(tx_id) {
    try {
      const txnDetails = await this.getTransactionInfo({
        transaction_ids: [tx_id.toString()],
      });
      if (transactionStatus().indexOf(txnDetails.transactions[0].status) >= 4) {
        return true;
      } else {
        return false;
      }
    } catch (err) {
      // Any error here must be treated as if the required status was not achieved
      return false;
    }
  }

  async isTransactionAtLeastMinedUnconfirmed(tx_id) {
    try {
      const txnDetails = await this.getTransactionInfo({
        transaction_ids: [tx_id.toString()],
      });
      if (transactionStatus().indexOf(txnDetails.transactions[0].status) >= 5) {
        return true;
      } else {
        return false;
      }
    } catch (err) {
      // Any error here must be treated as if the required status was not achieved
      return false;
    }
  }

  async isTransactionMinedUnconfirmed(tx_id) {
    try {
      const txnDetails = await this.getTransactionInfo({
        transaction_ids: [tx_id.toString()],
      });
      if (transactionStatus().indexOf(txnDetails.transactions[0].status) == 5) {
        return true;
      } else {
        return false;
      }
    } catch (err) {
      // Any error here must be treated as if the required status was not achieved
      return false;
    }
  }

  async isTransactionMinedConfirmed(tx_id) {
    try {
      const txnDetails = await this.getTransactionInfo({
        transaction_ids: [tx_id.toString()],
      });
      if (transactionStatus().indexOf(txnDetails.transactions[0].status) == 6) {
        return true;
      } else {
        return false;
      }
    } catch (err) {
      // Any error here must be treated as if the required status was not achieved
      return false;
    }
  }

  async getTransactionDetails(tx_id) {
    try {
      const txnDetails = await this.getTransactionInfo({
        transaction_ids: [tx_id.toString()],
      });
      return [true, txnDetails];
    } catch (err) {
      return [false, err];
    }
  }

  async coin_split(args) {
    return await this.client.coinSplit(args);
  }
}

module.exports = WalletClient;
