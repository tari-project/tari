// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

const grpc = require("@grpc/grpc-js");
const protoLoader = require("@grpc/proto-loader");
const { promisifyAll } = require("grpc-promise");
const path = require("path");

const packageDefinition = protoLoader.loadSync(
  path.resolve(
    __dirname,
    "../../../../applications/minotari_app_grpc/proto/base_node.proto"
  ),
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
  const client = new tariGrpc.BaseNode(
    address,
    grpc.credentials.createInsecure()
  );
  promisifyAll(client, { metadata: new grpc.Metadata() });
  return client;
}

function Client(address = "127.0.0.1:18142") {
  this.inner = connect(address);

  const methods = [
    "getVersion",
    "listHeaders",
    "getBlocks",
    "getMempoolTransactions",
    "getTipInfo",
    "searchUtxos",
    "getTokens",
    "getNetworkDifficulty",
    "getActiveValidatorNodes",
  ];
  methods.forEach((method) => {
    this[method] = (arg) => this.inner[method]().sendMessage(arg);
  });
}

Client.connect = (address) => new Client(address);

module.exports = {
  Client,
  types: tariGrpc,
};
