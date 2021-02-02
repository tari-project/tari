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

    async identify(args) {
       let info = await this.client.identify(args);
       return {
         "public_key": info["public_key"].toString('utf8'),
         "public_address": info["public_address"],
         "node_id": info["node_id"].toString('utf8'),
       };
    }
}

module.exports = WalletClient;