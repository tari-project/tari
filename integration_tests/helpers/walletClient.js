const {Client} = require('wallet-grpc-client');
const {sleep} = require("./util");

function transactionStatus() {
    return [
        "TRANSACTION_STATUS_PENDING",
        "TRANSACTION_STATUS_COMPLETED",
        "TRANSACTION_STATUS_BROADCAST",
        "TRANSACTION_STATUS_MINED"
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
            return false;
        }
    }

    async isTransactionAtLeastPending(tx_id) {
        try {
            let txnDetails = await this.getTransactionInfo({
                "transaction_ids": [ tx_id.toString() ]
            });
            if (transactionStatus().indexOf(txnDetails.transactions[0]["status"]) >= 0) {
                return true;
            } else {
                return false;
            }
        } catch (err) {
            return false;
        }
    }

    async isTransactionAtLeastCompleted(tx_id) {
        try {
            let txnDetails = await this.getTransactionInfo({
                "transaction_ids": [ tx_id.toString() ]
            });
            if (transactionStatus().indexOf(txnDetails.transactions[0]["status"]) >= 1) {
                return true;
            } else {
                return false;
            }
        } catch (err) {
            return false;
        }
    }

    async isTransactionAtLeastBroadcast(tx_id) {
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
            return false;
        }
    }

    async isTransactionMined(tx_id) {
        try {
            let txnDetails = await this.getTransactionInfo({
                "transaction_ids": [ tx_id.toString() ]
            });
            if (transactionStatus().indexOf(xnDetails.transactions[0]["status"]) == 3) {
                return true;
            } else {
                return false;
            }
        } catch (err) {
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