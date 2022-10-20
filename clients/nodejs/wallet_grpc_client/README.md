# Tari Wallet GRPC client

Async GRPC client library for the Tari Console Wallet.

## Usage

```javascript
    const {Client} = require('@tari/wallet-grpc-client');

    const walletAddress = 'localhost:18143';
    const client = Client.connect(walletAddress);
    const {version} = await client.getVersion();
    console.log(version);
    const {transaction} = await client.getCoinbase({fee: 1, amount: 10000, reward: 123, height: 1000});
    console.log(transaction);
```
