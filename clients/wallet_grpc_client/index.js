const grpc = require("@grpc/grpc-js");
const protoLoader = require("@grpc/proto-loader");
const { promisifyAll } = require("grpc-promise");

const packageDefinition = protoLoader.loadSync(
  `${__dirname}/../../applications/tari_app_grpc/proto/wallet.proto`,
  {
    keepCase: true,
    longs: String,
    enums: String,
    defaults: true,
    oneofs: true,
  }
);
const protoDescriptor = grpc.loadPackageDefinition(packageDefinition);
const tariGrpc = protoDescriptor.tari.rpc;

function connect(address) {
  const client = new tariGrpc.Wallet(
    address,
    grpc.credentials.createInsecure()
  );
  promisifyAll(client, { metadata: new grpc.Metadata() });
  return client;
}

function Client(address) {
  this.inner = connect(address);
  const functions = [
    "identify",
    "coinSplit",
    "getBalance",
    "getCoinbase",
    "getCompletedTransactions",
    "getTransactionInfo",
    "getVersion",
    "identify",
    "transfer",
    "importUtxos",
    "listConnectedPeers",
    "getNetworkStatus",
  ];

  this.waitForReady = (...args) => {
    this.inner.waitForReady(...args);
  };

  functions.forEach((method) => {
    this[method] = (arg) => this.inner[method]().sendMessage(arg);
  });
}

Client.connect = (address) => new Client(address);

module.exports = {
  Client,
  types: tariGrpc,
};

// (async () => {
//     const a = Client.connect('localhost:18143');
//     const {version} = await a.getVersion();
//     console.log(version);
//     const resp = await a.getCoinbase({fee: 1, amount: 10000, reward: 124, height: 1001});
//     console.log(resp);
// })()
