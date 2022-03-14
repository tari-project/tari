const { Client } = require("wallet-grpc-client");
const {
  byteArrayToHex,
  tryConnect,
  convertStringToVec,
  multiAddrToSocket,
} = require("./util");

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
  constructor(name) {
    this.client = null;
    this.name = name;
  }

  async connect(multiAddrOrSocket) {
    this.client = await tryConnect(() =>
      Client.connect(multiAddrToSocket(multiAddrOrSocket))
    );
  }

  async getVersion() {
    return await this.client.getVersion();
  }

  async checkForUpdates() {
    return await this.client.checkForUpdates({});
  }

  async getBalance() {
    return await this.client.getBalance().then((balance) => ({
      available_balance: parseInt(balance.available_balance),
      pending_incoming_balance: parseInt(balance.pending_incoming_balance),
      pending_outgoing_balance: parseInt(balance.pending_outgoing_balance),
    }));
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
      if (transactionStatus().indexOf(data[i].status) === 6) {
        transactions.push(data[i]);
      }
    }
    return transactions;
  }

  async countAllCoinbaseTransactions() {
    const data = await this.getCompletedTransactions();
    let count = 0;
    for (let i = 0; i < data.length; i++) {
      if (
        data[i].message.includes("Coinbase Transaction for Block ") &&
        data[i].fee == 0
      ) {
        count += 1;
      }
    }
    return count;
  }

  async countAllSpendableCoinbaseTransactions() {
    const data = await this.getAllCoinbaseTransactions();
    let count = 0;
    for (let i = 0; i < data.length; i++) {
      if (transactionStatus().indexOf(data[i].status) == 6) {
        count += 1;
      }
    }
    return count;
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
    const data = await this.getCompletedTransactions();
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

  async sendHtlc(args) {
    return await this.client.SendShaAtomicSwapTransaction(args);
  }

  async claimHtlc(args) {
    return await this.client.claimShaAtomicSwapTransaction(args);
  }

  async claimHtlcRefund(args) {
    return await this.client.ClaimHtlcRefundTransaction(args);
  }

  async importUtxos(outputs) {
    return await this.client.importUtxos({
      outputs: outputs,
    });
  }

  async getTransactionInfo(args) {
    return await this.client.getTransactionInfo(args);
  }

  async identify() {
    const info = await this.client.identify();
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
      console.log(
        "Waiting for available balance > amount",
        balance.available_balance,
        amount
      );
      if (parseInt(balance.available_balance) >= parseInt(amount)) {
        return true;
      } else {
        return false;
      }
    } catch (e) {
      // Any error here must be treated as if the required status was not achieved
      return false;
    }
  }

  async isBalanceLessThan(amount) {
    try {
      let balance = await this.getBalance();
      if (parseInt(balance["available_balance"]) < parseInt(amount)) {
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

  async isTransactionPending(tx_id) {
    try {
      const txnDetails = await this.getTransactionInfo({
        transaction_ids: [tx_id.toString()],
      });
      if (transactionStatus().indexOf(txnDetails.transactions[0].status) == 2) {
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

  async listConnectedPeers() {
    const { connected_peers } = await this.client.listConnectedPeers();
    return connected_peers.map((peer) => ({
      ...peer,
      public_key: byteArrayToHex(peer.public_key),
      node_id: byteArrayToHex(peer.node_id),
      supported_protocols: peer.supported_protocols.map((p) =>
        p.toString("utf8")
      ),
      features: +peer.features,
    }));
  }

  async getNetworkStatus() {
    let resp = await this.client.getNetworkStatus();
    return {
      ...resp,
      num_node_connections: +resp.num_node_connections,
    };
  }

  async cancelTransaction(tx_id) {
    try {
      const result = await this.client.cancelTransaction({
        tx_id: tx_id,
      });
      return {
        success: result.is_success,
        failure_message: result.failure_message,
      };
    } catch (err) {
      return {
        success: false,
        failure_message: err,
      };
    }
  }

  async registerAsset(name) {
    let public_key = await this.client.registerAsset({ name });
    return public_key.public_key.toString("hex");
  }

  async getOwnedAssets() {
    let assets = await this.client.getOwnedAssets();
    return assets.assets;
  }

  async mintTokens(asset_public_key, names) {
    let owner_commitments = await this.client.mintTokens({
      asset_public_key,
      unique_ids: names.map((name) => convertStringToVec(name)),
    });
    return owner_commitments.owner_commitments;
  }

  async getOwnedTokens(asset_public_key) {
    let tokens = await this.client.getOwnedTokens({ asset_public_key });
    return tokens.tokens;
  }
}

module.exports = WalletClient;
