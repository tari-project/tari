const expect = require("chai").expect;
const grpc = require("grpc");
const protoLoader = require("@grpc/proto-loader");
const grpc_promise = require("grpc-promise");
const TransactionBuilder = require("./transactionBuilder");
const { SHA3 } = require("sha3");
const { toLittleEndian } = require("./util");
const cloneDeep = require("clone-deep");

class BaseNodeClient {
  constructor(clientOrPort) {
    if (typeof clientOrPort === "number") {
      this.client = this.createGrpcClient(clientOrPort);
    } else {
      this.client = clientOrPort;
    }
    this.blockTemplates = {};
  }

  createGrpcClient(port) {
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
    const client = new tari.BaseNode(
      "127.0.0.1:" + port,
      grpc.credentials.createInsecure()
    );
    grpc_promise.promisifyAll(client);
    return client;
  }

  getHeaderAt(height) {
    return this.client
      .listHeaders()
      .sendMessage({ from_height: height, num_headers: 1 })
      .then((header) => {
        console.log("Header:", header);
        return header;
      });
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
    return this.client
      .listHeaders()
      .sendMessage({ from_height: 0, num_headers: 1 })
      .then((headers) => {
        const header = headers[0];
        return Object.assign(header, {
          height: +header.height,
        });
      });
  }

  getTipHeight() {
    return this.client
      .getTipInfo()
      .sendMessage({})
      .then((tip) => parseInt(tip.metadata.height_of_longest_chain));
  }

  getPreviousBlockTemplate(height) {
    return cloneDeep(this.blockTemplates["height" + height]);
  }

  getBlockTemplate(weight) {
    return this.client
      .getNewBlockTemplate()
      .sendMessage({ algo: { pow_algo: 2 }, max_weight: weight })
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

  async mineBlockBeforeSubmit(walletClient, weight) {
    // Empty template from base node
    const emptyTemplate = await this.client
      .getNewBlockTemplate()
      .sendMessage({ algo: { pow_algo: 2 }, max_weight: weight });
    // Coinbase from wallet
    const coinbase = await walletClient.client.inner.getCoinbase().sendMessage({
      reward: emptyTemplate.miner_data.reward,
      fee: emptyTemplate.miner_data.total_fees,
      height: emptyTemplate.new_block_template.header.height,
    });
    // New block from base node including coinbase
    const block = emptyTemplate.new_block_template;
    block.body.outputs = block.body.outputs.concat(
      coinbase.transaction.body.outputs
    );
    block.body.kernels = block.body.kernels.concat(
      coinbase.transaction.body.kernels
    );
    const newBlock = await this.client.getNewBlock().sendMessage(block);
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
        return this.client
          .getNewBlockTemplate()
          .sendMessage({ algo: { pow_algo: 2 }, max_weight: weight });
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

  async getMinedCandidateBlock(weight, existingBlockTemplate) {
    const builder = new TransactionBuilder();
    const blockTemplate =
      existingBlockTemplate || (await this.getBlockTemplate(weight));
    const privateKey = Buffer.from(
      toLittleEndian(blockTemplate.block.header.height, 256)
    ).toString("hex");
    const cb = builder.generateCoinbase(
      parseInt(blockTemplate.minerData.reward),
      privateKey,
      parseInt(blockTemplate.minerData.total_fees),
      parseInt(blockTemplate.block.header.height) + 2
    );
    const template = blockTemplate.block;
    template.body.outputs = template.body.outputs.concat(cb.outputs);
    template.body.kernels = template.body.kernels.concat(cb.kernels);
    return {
      template: template,
      coinbase: {
        output: cb.outputs[0],
        privateKey: privateKey,
        amount:
          parseInt(blockTemplate.minerData.reward) +
          parseInt(blockTemplate.minerData.total_fees),
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

  getSha3Difficulty(header) {
    const hash = new SHA3(256);
    hash.update(toLittleEndian(header.version, 16));
    hash.update(toLittleEndian(parseInt(header.height), 64));
    hash.update(header.prev_hash);
    const timestamp = parseInt(header.timestamp.seconds);
    hash.update(toLittleEndian(timestamp, 64));
    hash.update(header.output_mr);
    hash.update(header.range_proof_mr);
    hash.update(header.kernel_mr);
    hash.update(header.total_kernel_offset);
    hash.update(toLittleEndian(parseInt(header.nonce), 64));
    hash.update(toLittleEndian(header.pow.pow_algo));
    hash.update(
      toLittleEndian(parseInt(header.pow.accumulated_monero_difficulty), 64)
    );
    hash.update(
      toLittleEndian(parseInt(header.pow.accumulated_blake_difficulty), 64)
    );
    hash.update(header.pow.pow_data);
    const first_round = hash.digest();
    const hash2 = new SHA3(256);
    hash2.update(first_round);
    const res = hash2.digest("hex");
    return res;
  }
}

module.exports = BaseNodeClient;
