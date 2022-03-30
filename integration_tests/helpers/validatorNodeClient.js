// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

const grpc = require("@grpc/grpc-js");
const protoLoader = require("@grpc/proto-loader");
const {
  tryConnect,
  convertHexStringToVec,
  convertStringToVec,
} = require("./util");
const grpcPromise = require("grpc-promise");

class ValidatorNodeClient {
  constructor() {
    this.client = null;
    this.blockTemplates = {};
  }

  async connect(port) {
    const PROTO_PATH =
      __dirname +
      "/../../applications/tari_validator_node/proto/validator_node.proto";
    const packageDefinition = protoLoader.loadSync(PROTO_PATH, {
      keepCase: true,
      longs: String,
      enums: String,
      defaults: true,
      oneofs: true,
    });
    const protoDescriptor = grpc.loadPackageDefinition(packageDefinition);
    const tari = protoDescriptor.tari.validator_node.rpc;
    this.client = await tryConnect(
      () =>
        new tari.ValidatorNode(
          "127.0.0.1:" + port,
          grpc.credentials.createInsecure()
        )
    );

    grpcPromise.promisifyAll(this.client, {
      metadata: new grpc.Metadata(),
    });
  }

  static async create(port) {
    const client = new ValidatorNodeClient();
    await client.connect(port);
    return client;
  }

  executeInstruction(asset_public_key, method, metadata, token, signature, id) {
    console.log(
      `Executing instruction for asset ${asset_public_key} / token ${token} via method ${method} with metadata ${metadata} `
    );
    return this.client.executeInstruction().sendMessage({
      asset_public_key: convertHexStringToVec(asset_public_key),
      method,
      args: [convertStringToVec(metadata)],
      token_id: convertHexStringToVec(token),
      signature,
      id,
    });
  }
}

module.exports = ValidatorNodeClient;
