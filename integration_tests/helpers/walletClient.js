const {Client} = require('wallet-grpc-client');

class WalletClient {
     constructor(walletAddress) {
       this.client = Client.connect(walletAddress)
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
}

module.exports = WalletClient;