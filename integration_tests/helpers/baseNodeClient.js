// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

const expect = require("chai").expect;
const grpc = require("@grpc/grpc-js");
const protoLoader = require("@grpc/proto-loader");
const TransactionBuilder = require("./transactionBuilder");
const { SHA3 } = require("sha3");
const { toLittleEndian, byteArrayToHex, tryConnect } = require("./util");
const { PowAlgo } = require("./types");
const cloneDeep = require("clone-deep");
const grpcPromise = require("grpc-promise");

class BaseNodeClient {
  constructor() {
    this.client = null;
    this.blockTemplates = {};
  }

  async connect(port) {
    const PROTO_PATH =
      __dirname + "/../../applications/tari_app_grpc/proto/base_node.proto";
    const packageDefinition = protoLoader.loadSync(PROTO_PATH, {
      keepCase: true,
      longs: String,
      enums: String,
      defaults: true,
      oneofs: true,
    });
    const protoDescriptor = grpc.loadPackageDefinition(packageDefinition);
    const tari = protoDescriptor.tari.rpc;
    this.client = await tryConnect(
      () =>
        new tari.BaseNode(
          "127.0.0.1:" + port,
          grpc.credentials.createInsecure()
        )
    );

    grpcPromise.promisifyAll(this.client, {
      metadata: new grpc.Metadata(),
    });
  }

  getHeaderAt(height) {
    return this.getHeaders(height, 1).then((header) =>
      header && header.length ? header[0] : null
    );
  }

  getNetworkDifficulties(tip, start, end) {
    return this.client
      .getNetworkDifficulty()
      .sendMessage({ from_tip: tip, start_height: start, end_height: end });
  }

  getPeers() {
    return this.client
      .getPeers()
      .sendMessage({})
      .then((peers) => {
        console.log("Got ", peers.length, " peers:");
        return peers;
      });
  }

  getTipHeader() {
    return this.getHeaders(0, 1).then((headers) => {
      const header = headers[0];
      return Object.assign(header, {
        height: +header.height,
      });
    });
  }

  async getHeaders(from_height, num_headers, sorting = 0) {
    return await this.client
      .listHeaders()
      .sendMessage({ from_height, num_headers, sorting })
      .then((resp) => resp.map((r) => r.header));
  }

  getTipHeight() {
    return this.client
      .getTipInfo()
      .sendMessage({})
      .then((tip) => parseInt(tip.metadata.height_of_longest_chain));
  }

  getPrunedHeight() {
    return this.client
      .getTipInfo()
      .sendMessage({})
      .then((tip) => parseInt(tip.metadata.pruned_height));
  }

  getPreviousBlockTemplate(height) {
    return cloneDeep(this.blockTemplates["height" + height]);
  }

  getBlockTemplate(weight) {
    return this.client
      .getNewBlockTemplate()
      .sendMessage({ algo: { pow_algo: PowAlgo.SHA3 }, max_weight: weight })
      .then((template) => {
        const res = {
          minerData: template.miner_data,
          block: template.new_block_template,
        };
        this.blockTemplates[
          "height" + template.new_block_template.header.height
        ] = cloneDeep(res);
        return res;
      });
  }

  submitBlockWithCoinbase(template, coinbase) {
    const cb = coinbase;
    template.body.outputs = template.body.outputs.concat(cb.outputs);
    template.body.kernels = template.body.kernels.concat(cb.kernels);
    return this.client
      .getNewBlock()
      .sendMessage(template)
      .then((b) => {
        return this.client.submitBlock().sendMessage(b.block);
      });
  }

  submitTemplate(template, beforeSubmit) {
    return this.client
      .getNewBlock()
      .sendMessage(template.template)
      .then((b) => {
        // console.log("Sha3 diff", this.getSha3Difficulty(b.block.header));
        if (beforeSubmit) {
          b = beforeSubmit({ block: b, originalTemplate: template });
          if (!b) {
            return Promise.resolve();
          }
          b = b.block;
        }
        return this.client.submitBlock().sendMessage(b.block);
      });
  }

  submitBlock(b) {
    return this.client.submitBlock().sendMessage(b.block);
  }

  submitTransaction(txn) {
    return this.client
      .submitTransaction()
      .sendMessage({ transaction: txn })
      .then((res) => {
        return res;
      });
  }

  transactionState(txn) {
    return this.client
      .transactionState()
      .sendMessage({ excess_sig: txn })
      .then((res) => {
        return res;
      });
  }

  checkForUpdates() {
    return this.client.checkForUpdates().sendMessage({});
  }

  transactionStateResult(txn) {
    return this.client
      .transactionState()
      .sendMessage({ excess_sig: txn })
      .then((res) => {
        return res.result;
      });
  }

  fetchMatchingUtxos(hashes) {
    return this.client
      .fetchMatchingUtxos()
      .sendMessage({ hashes: hashes })
      .then((result) => {
        return result;
      });
  }

  async mineBlockBeforeSubmit(weight) {
    // New block from base node including coinbase
    const block = await this.getMinedCandidateBlock(weight);
    const newBlock = await this.client
      .getNewBlock()
      .sendMessage(block.template);
    return newBlock;
  }

  async submitMinedBlock(newBlock) {
    const response = await this.client
      .submitBlock()
      .sendMessage(newBlock.block);
    return response;
  }

  mineBlock(walletClient, weight) {
    if (!walletClient) {
      return this.mineBlockWithoutWallet(weight);
    }
    let currHeight;
    let block;
    return this.client
      .getTipInfo()
      .sendMessage({})
      .then((tip) => {
        currHeight = parseInt(tip.metadata.height_of_longest_chain);
        return this.client.getNewBlockTemplate().sendMessage({
          algo: { pow_algo: PowAlgo.SHA3 },
          max_weight: weight,
        });
      })
      .then((template) => {
        block = template.new_block_template;
        const height = block.header.height;
        return walletClient.client.inner.getCoinbase().sendMessage({
          reward: template.miner_data.reward,
          fee: template.miner_data.total_fees,
          height: height,
        });
      })
      .then((coinbase) => {
        const cb = coinbase.transaction;
        block.body.outputs = block.body.outputs.concat(cb.body.outputs);
        block.body.kernels = block.body.kernels.concat(cb.body.kernels);
        return this.client.getNewBlock().sendMessage(block);
      })
      .then((b) => {
        return this.client.submitBlock().sendMessage(b.block);
      })
      .then(() => {
        return this.client.getTipInfo().sendMessage({});
      })
      .then((tipInfo) => {
        expect(tipInfo.metadata.height_of_longest_chain).to.equal(
          currHeight + 1 + ""
        );
      });
  }

  async getMinedCandidateBlock(weight, existingBlockTemplate, walletClient) {
    const builder = new TransactionBuilder();
    const blockTemplate =
      existingBlockTemplate || (await this.getBlockTemplate(weight));
    const privateKey = toLittleEndian(
      blockTemplate.block.header.height,
      256
    ).toString("hex");
    const height = parseInt(blockTemplate.block.header.height) + 2;

    let cb_outputs;
    let cb_kernels;
    if (!walletClient) {
      const cb_builder = builder.generateCoinbase(
        parseInt(blockTemplate.minerData.reward),
        privateKey,
        parseInt(blockTemplate.minerData.total_fees),
        height
      );
      cb_outputs = cb_builder.outputs;
      cb_kernels = cb_builder.kernels;
    } else {
      const cb_wallet = await walletClient.client.inner
        .getCoinbase()
        .sendMessage({
          reward: parseInt(blockTemplate.minerData.reward),
          fee: parseInt(blockTemplate.minerData.total_fees),
          height: height,
        });
      cb_outputs = cb_wallet.transaction.body.outputs;
      cb_kernels = cb_wallet.transaction.body.kernels;
    }

    const template = blockTemplate.block;
    template.body.outputs = template.body.outputs.concat(cb_outputs);
    template.body.kernels = template.body.kernels.concat(cb_kernels);
    return {
      template: template,
      coinbase: {
        output: cb_outputs[0],
        privateKey: privateKey,
        amount:
          parseInt(blockTemplate.minerData.reward) +
          parseInt(blockTemplate.minerData.total_fees),
        scriptPrivateKey: privateKey,
        scriptOffsetPrivateKey: Buffer.from(
          "0000000000000000000000000000000000000000000000000000000000000000",
          "hex"
        ),
        height: blockTemplate.block.header.height,
      },
    };
  }

  async mineBlockWithoutWallet(beforeSubmit, weight, onError) {
    const template = await this.getMinedCandidateBlock(weight);
    return this.submitTemplate(template, beforeSubmit).then(
      async () => {
        // let tip = await this.getTipHeight();
        // console.log("Node is at tip:", tip);
      },
      (err) => {
        console.log("err submitting block:", err);
        if (onError) {
          if (!onError(err)) {
            throw err;
          }
          // handled
        } else {
          throw err;
        }
      }
    );
  }

  async mineBlockWithWallet(weight, walletClient, onError) {
    const template = await this.getMinedCandidateBlock(
      weight,
      null,
      walletClient
    );
    return this.submitTemplate(template).then(
      async () => {
        // let tip = await this.getTipHeight();
        // console.log("Node is at tip:", tip);
      },
      (err) => {
        console.log("err submitting block:", err);
        if (onError) {
          if (!onError(err)) {
            throw err;
          }
          // handled
        } else {
          throw err;
        }
      }
    );
  }

  async mineBlocksUntilHeightIncreasedBy(numBlocks, walletClient) {
    let tipHeight = parseInt(await this.getTipHeight());
    const height = (await this.getTipHeight()) + numBlocks;
    const weight = 0;
    let i = 0;
    do {
      if (i % 25 === 0) {
        console.log(
          "[base node client] Tip at",
          tipHeight,
          "...(stopping at " + height + ")"
        );
      }
      i += 1;
      if (!walletClient) {
        await this.mineBlockWithoutWallet(null, weight, null);
      } else {
        await this.mineBlockWithWallet(weight, walletClient);
      }
      tipHeight = await this.getTipHeight();
    } while (tipHeight < height);
    return await this.getTipHeight();
  }

  getSha3Difficulty(header) {
    const hash = new SHA3(256);
    hash.update(toLittleEndian(header.version, 16));
    hash.update(toLittleEndian(parseInt(header.height), 64));
    hash.update(header.prev_hash);
    const timestamp = parseInt(header.timestamp.seconds);
    hash.update(toLittleEndian(timestamp, 64));
    hash.update(header.input_mr);
    hash.update(header.output_mr);
    hash.update(header.witness_mr);
    hash.update(header.kernel_mr);
    hash.update(header.total_kernel_offset);
    hash.update(toLittleEndian(parseInt(header.nonce), 64));
    hash.update(toLittleEndian(header.pow.pow_algo));
    hash.update(
      toLittleEndian(parseInt(header.pow.accumulated_monero_difficulty), 64)
    );
    hash.update(
      toLittleEndian(parseInt(header.pow.accumulated_sha_difficulty), 64)
    );
    hash.update(header.pow.pow_data);
    const first_round = hash.digest();
    const hash2 = new SHA3(256);
    hash2.update(first_round);
    const res = hash2.digest("hex");
    return res;
  }

  async identify() {
    const info = await this.client.identify().sendMessage({});
    return {
      public_key: byteArrayToHex(info.public_key),
      public_address: info.public_address,
      node_id: byteArrayToHex(info.node_id),
    };
  }

  async listConnectedPeers() {
    const { connected_peers } = await this.client
      .listConnectedPeers()
      .sendMessage({});
    return connected_peers.map((peer) => ({
      ...peer,
      public_key: byteArrayToHex(peer.public_key),
      node_id: byteArrayToHex(peer.node_id),
      supported_protocols: peer.supported_protocols.map((p) =>
        p.toString("utf8")
      ),
      features: +peer.features,
    }));
  }

  async getNetworkStatus() {
    let resp = await this.client.getNetworkStatus().sendMessage({});
    return {
      ...resp,
      num_node_connections: +resp.num_node_connections,
    };
  }

  async initial_sync_achieved() {
    let result = await this.client.GetTipInfo().sendMessage({});
    return result.initial_sync_achieved;
  }

  async get_node_state() {
    let result = await this.client.GetTipInfo().sendMessage({});
    return result.base_node_state;
  }

  static async create(port) {
    const client = new BaseNodeClient();
    await client.connect(port);
    return client;
  }

  async getMempoolStats() {
    const mempoolStats = await this.client.getMempoolStats().sendMessage({});
    return mempoolStats;
  }

  async getBlocks(heights) {
    return await this.client.getBlocks().sendMessage({ heights });
  }
}

module.exports = BaseNodeClient;
