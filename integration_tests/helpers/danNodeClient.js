const grpc = require("@grpc/grpc-js");
const protoLoader = require("@grpc/proto-loader");
const { tryConnect } = require("./util");
const grpcPromise = require("grpc-promise");

class DanNodeClient {
  constructor() {
    this.client = null;
    this.blockTemplates = {};
  }

  async connect(port) {
    const PROTO_PATH =
      __dirname + "/../../applications/tari_dan_node/proto/dan_node.proto";
    const packageDefinition = protoLoader.loadSync(PROTO_PATH, {
      keepCase: true,
      longs: String,
      enums: String,
      defaults: true,
      oneofs: true,
    });
    const protoDescriptor = grpc.loadPackageDefinition(packageDefinition);
    const tari = protoDescriptor.tari.dan.rpc;
    this.client = await tryConnect(
      () =>
        new tari.DanNode("127.0.0.1:" + port, grpc.credentials.createInsecure())
    );

    grpcPromise.promisifyAll(this.client, {
      metadata: new grpc.Metadata(),
    });
  }

  static async create(port) {
    const client = new DanNodeClient();
    await client.connect(port);
    return client;
  }

  executeInstruction(asset_public_key, method, metadata, token, signature, id) {
    let convertHexStringToVec = (string) =>
      string.match(/.{2}/g).map((x) => parseInt(x, 16));
    let convertStringToVec = (string) =>
      Array(string.length)
        .fill()
        .map((_, i) => string.charCodeAt(i));

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

module.exports = DanNodeClient;
