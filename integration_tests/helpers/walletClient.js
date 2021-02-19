const {Client} = require('wallet-grpc-client');
const {sleep} = require("./util");

function transactionStatus() {
    return [
        "TRANSACTION_STATUS_IMPORTED",
        "TRANSACTION_STATUS_COINBASE",
        "TRANSACTION_STATUS_PENDING",
        "TRANSACTION_STATUS_COMPLETED",
        "TRANSACTION_STATUS_BROADCAST",
        "TRANSACTION_STATUS_MINED_UNCONFIRMED",
        "TRANSACTION_STATUS_MINED_CONFIRMED"
    ];
}

class WalletClient {
     constructor(walletAddress, name) {
       this.client = Client.connect(walletAddress)
       this.name = name;
     }

    async getVersion() {
      return await this.client.getVersion();
    }

    async getBalance() {
      return await this.client.getBalance();
    }

    async getAllCompletedTransactions() {
        let data = await this.client.getAllCompletedTransactions();
        let transactions = [];
        let myDate = new Date();
        for (var i=0; i<data.transactions.length; i++) {
            transactions.push({
                "tx_id": data.transactions[i]["tx_id"],
                "source_pk": data.transactions[i]["source_pk"].toString('hex'),
                "dest_pk": data.transactions[i]["dest_pk"].toString('hex'),
                "status": data.transactions[i]["status"],
                "direction": data.transactions[i]["direction"],
                "amount": data.transactions[i]["amount"],
                "fee": data.transactions[i]["fee"],
                "is_cancelled": data.transactions[i]["is_cancelled"],
                "excess_sig": data.transactions[i]["excess_sig"].toString('hex'),
                "timestamp": new Date(Number(data.transactions[i]["timestamp"]["seconds"]) * 1000),
                "message": data.transactions[i]["message"],
                "valid": data.transactions[i]["valid"]
            });
        }
        return transactions;
    }

    async getAllCoinbaseTransactions() {
        let data = await this.getAllCompletedTransactions();
        let transactions = [];
        for (var i=0; i<data.length; i++) {
            if (
                data[i]["message"].includes('Coinbase Transaction for Block ') &&
                data[i]["fee"] == 0
            ) {
                transactions.push(data[i]);
            }
        }
        return transactions;
    }

    async getAllSpendableCoinbaseTransactions() {
        let data = await this.getAllCoinbaseTransactions();
        let transactions = [];
        for (var i=0; i<data.length; i++) {
            if (
                transactionStatus().indexOf(data[i]["status"]) == 6 &&
                data[i]["valid"] == true
            ) {
                transactions.push(data[i]);
            }
        }
        return transactions;
    }

    async areCoinbasesConfirmedAtLeast(number) {
        let data = await this.getAllSpendableCoinbaseTransactions();
        if (data.length >= number) {
            return true;
        } else {
            return false;
        }
    }

    async getAllNormalTransactions() {
        let data = this.getAllCompletedTransactions();
        let transactions = [];
        for (var i=0; i<data.length; i++) {
            if (!(data[i]["message"].includes('Coinbase Transaction for Block ') && data[i]["fee"] == 0)) {
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
       let info = await this.client.identify(args);
       return {
         "public_key": info["public_key"].toString('utf8'),
         "public_address": info["public_address"],
         "node_id": info["node_id"].toString('utf8'),
       };
    }

    async isTransactionRegistered(tx_id) {
        try {
            await this.getTransactionInfo({
                "transaction_ids": [ tx_id.toString() ]
            });
            return true;
        } catch (err) {
            // Any error here must be treated as if the required status was not achieved
            return false;
        }
    }

    async isBalanceAtLeast(amount) {
        try {
            let balance = await this.getBalance();
            if (balance["available_balance"] >= amount) {
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
            let txnDetails = await this.getTransactionInfo({
                "transaction_ids": [ tx_id.toString() ]
            });
            if (transactionStatus().indexOf(txnDetails.transactions[0]["status"]) >= 2) {
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
            let txnDetails = await this.getTransactionInfo({
                "transaction_ids": [ tx_id.toString() ]
            });
            if (transactionStatus().indexOf(txnDetails.transactions[0]["status"]) >= 3) {
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
            let txnDetails = await this.getTransactionInfo({
                "transaction_ids": [ tx_id.toString() ]
            });
            if (transactionStatus().indexOf(txnDetails.transactions[0]["status"]) >= 4) {
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
            let txnDetails = await this.getTransactionInfo({
                "transaction_ids": [ tx_id.toString() ]
            });
            if (transactionStatus().indexOf(txnDetails.transactions[0]["status"]) >= 5) {
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
            let txnDetails = await this.getTransactionInfo({
                "transaction_ids": [ tx_id.toString() ]
            });
            if (transactionStatus().indexOf(txnDetails.transactions[0]["status"]) == 5) {
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
            let txnDetails = await this.getTransactionInfo({
                "transaction_ids": [ tx_id.toString() ]
            });
            if (transactionStatus().indexOf(txnDetails.transactions[0]["status"]) == 6) {
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
            let txnDetails = await this.getTransactionInfo({
                "transaction_ids": [ tx_id.toString() ]
            });
            return [true, txnDetails];
        } catch (err) {
            return [false, err];
        }
    }
}

module.exports = WalletClient;