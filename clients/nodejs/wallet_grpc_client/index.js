// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

const grpc = require("@grpc/grpc-js");
const protoLoader = require("@grpc/proto-loader");
const { promisifyAll } = require("grpc-promise");

const packageDefinition = protoLoader.loadSync(
  `${__dirname}/../../../applications/minotari_app_grpc/proto/wallet.proto`,
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

function connect(address, options = {}) {
  const client = new tariGrpc.Wallet(
    address,
    createAuth(options.authentication || {}),
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
    "getAddress",
    "transfer",
    "importUtxos",
    "listConnectedPeers",
    "getNetworkStatus",
    "cancelTransaction",
    "checkForUpdates",
    "revalidateAllTransactions",
    "SendShaAtomicSwapTransaction",
    "CreateBurnTransaction",
    "claimShaAtomicSwapTransaction",
    "ClaimHtlcRefundTransaction",
    "registerAsset",
    "getOwnedAssets",
    "mintTokens",
    "getOwnedTokens",
  ];

  this.waitForReady = (...args) => {
    this.inner.waitForReady(...args);
  };

  functions.forEach((method) => {
    this[method] = (arg) => this.inner[method]().sendMessage(arg);
  });
}

Client.connect = (address) => new Client(address);

function createAuth(auth = {}) {
  if (auth.type === "basic") {
    const {
      username,
      password
    } = auth;
    return grpc.credentials.createFromMetadataGenerator((params, callback) => {
        const md = new grpc.Metadata();
        let token = new Buffer(`${username}:${password}`).toString("base64");
        md.set('authorization', 'Basic ' + token);
        return callback(null, md);
    });
  } else{
    return grpc.credentials.createInsecure();
  }

}

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
