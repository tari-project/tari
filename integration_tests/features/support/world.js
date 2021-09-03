const { setWorldConstructor, After, BeforeAll } = require("cucumber");

const BaseNodeProcess = require("../../helpers/baseNodeProcess");
const MergeMiningProxyProcess = require("../../helpers/mergeMiningProxyProcess");
const WalletProcess = require("../../helpers/walletProcess");
const WalletFFIClient = require("../../helpers/walletFFIClient");
const MiningNodeProcess = require("../../helpers/miningNodeProcess");
const TransactionBuilder = require("../../helpers/transactionBuilder");
const glob = require("glob");
const fs = require("fs");
const archiver = require("archiver");
const InterfaceFFI = require("../../helpers/ffi/ffiInterface");

class CustomWorld {
  constructor({ attach, parameters }) {
    // this.variable = 0;
    this.attach = attach;
    this.checkAutoTransactions = true;
    this.seeds = {};
    this.nodes = {};
    this.proxies = {};
    this.miners = {};
    this.wallets = {};
    this.walletsFFI = {};
    this.walletPubkeys = {};
    this.clients = {};
    this.headers = {};
    this.outputs = {};
    this.transactionOutputs = {};
    this.testrun = `run${Date.now()}`;
    this.lastResult = null;
    this.blocks = {};
    this.transactions = {};
    this.peers = {};
    this.transactionsMap = new Map();
    this.resultStack = [];
    this.logFilePathBaseNode =
      parameters.logFilePathBaseNode || "./log4rs/base_node.yml";
    this.logFilePathProxy = parameters.logFilePathProxy || "./log4rs/proxy.yml";
    this.logFilePathMiningNode =
      parameters.logFilePathMiningNode || "./log4rs/mining_node.yml";
    this.logFilePathWallet =
      parameters.logFilePathWallet || "./log4rs/wallet.yml";
  }

  async createSeedNode(name) {
    console.log(`seed:`, name);
    const proc = new BaseNodeProcess(
      `seed-${name}`,
      false,
      null,
      this.logFilePathBaseNode
    );
    await proc.startNew();
    this.seeds[name] = proc;
    this.clients[name] = await proc.createGrpcClient();
  }

  seedAddresses() {
    const res = [];
    for (const property in this.seeds) {
      res.push(this.seeds[property].peerAddress());
    }
    return res;
  }

  /// Create but don't add the node
  createNode(name, options) {
    return new BaseNodeProcess(name, false, options, this.logFilePathBaseNode);
  }

  async createAndAddNode(name, addresses) {
    const node = this.createNode(name);
    if (Array.isArray(addresses)) {
      node.setPeerSeeds(addresses);
    } else {
      node.setPeerSeeds([addresses]);
    }
    await node.startNew();
    await this.addNode(name, node);
  }

  async addNode(name, process) {
    this.nodes[name] = process;
    this.clients[name] = await process.createGrpcClient();
  }

  addMiningNode(name, process) {
    this.miners[name] = process;
  }

  addProxy(name, process) {
    this.proxies[name] = process;
  }

  async createAndAddWallet(name, nodeAddresses, options = {}) {
    const wallet = new WalletProcess(
      name,
      false,
      options,
      this.logFilePathWallet
    );
    wallet.setPeerSeeds([nodeAddresses]);
    await wallet.startNew();

    this.addWallet(name, wallet);
    let walletClient = await wallet.connectClient();
    let walletInfo = await walletClient.identify();
    this.walletPubkeys[name] = walletInfo.public_key;
  }

  async createAndAddFFIWallet(name, seed_words = null, passphrase = null) {
    const wallet = new WalletFFIClient(name);
    await wallet.startNew(seed_words, passphrase);
    this.walletsFFI[name] = wallet;
    this.walletPubkeys[name] = wallet.identify();
    return wallet;
  }

  addWallet(name, process) {
    this.wallets[name] = process;
  }

  addWalletPubkey(name, pubkey) {
    this.walletPubkeys[name] = pubkey;
  }

  addOutput(name, output) {
    this.outputs[name] = output;
  }

  addTransactionOutput(spendHeight, output) {
    if (this.transactionOutputs[spendHeight] == null) {
      this.transactionOutputs[spendHeight] = [output];
    } else {
      this.transactionOutputs[spendHeight].push(output);
    }
  }

  async createTransactions(name, height) {
    let result = true;
    const txInputs = this.transactionOutputs[height];
    if (txInputs == null) {
      return result;
    }
    let i = 0;
    for (const input of txInputs) {
      const txn = new TransactionBuilder();
      txn.addInput(input);
      const txOutput = txn.addOutput(txn.getSpendableAmount());
      this.addTransactionOutput(height + 1, txOutput);
      const completedTx = txn.build();
      const submitResult = await this.getClient(name).submitTransaction(
        completedTx
      );
      if (this.checkAutoTransactions && submitResult.result != "ACCEPTED") {
        result = false;
      }
      if (submitResult.result == "ACCEPTED") {
        i++;
      }
      if (i > 9) {
        //this is to make sure the blocks stay relatively empty so that the tests don't take too long
        break;
      }
    }
    console.log(
      `Created ${i} transactions for node: ${name} at height: ${height}`
    );
    return result;
  }

  async mineBlock(name, weight, beforeSubmit, onError) {
    await this.clients[name].mineBlockWithoutWallet(
      beforeSubmit,
      weight,
      onError
    );
  }

  async baseNodeMineBlocksUntilHeightIncreasedBy(baseNode, wallet, numBlocks) {
    let w = null;
    if (wallet) {
      let tmp = this.getWallet(wallet);
      w = await tmp.connectClient();
    }
    const promise = this.getClient(baseNode).mineBlocksUntilHeightIncreasedBy(
      numBlocks,
      w
    );
    return promise;
  }

  sha3MineBlocksUntilHeightIncreasedBy(miner, numBlocks, minDifficulty) {
    const promise = this.getMiningNode(miner).mineBlocksUntilHeightIncreasedBy(
      numBlocks,
      minDifficulty
    );
    return promise;
  }

  async mergeMineBlock(name) {
    const client = this.proxies[name].createClient();
    await client.mineBlock();
  }

  mergeMineBlocksUntilHeightIncreasedBy(mmProxy, numBlocks) {
    const promise = this.getProxy(mmProxy)
      .createClient()
      .mineBlocksUntilHeightIncreasedBy(numBlocks);
    return promise;
  }

  saveBlock(name, block) {
    this.blocks[name] = block;
  }

  async submitBlock(blockName, nodeName) {
    await this.clients[nodeName]
      .submitBlock(this.blocks[blockName].block)
      .catch((err) => {
        console.log("submit block error", err);
      });
    // console.log(result);
  }

  getClient(name) {
    const client = this.clients[name];
    if (!client) {
      throw new Error(`Node client not found with name '${name}'`);
    }
    return client;
  }

  getNode(name) {
    const node = this.nodes[name] || this.seeds[name];
    if (!node) {
      throw new Error(`Node not found with name '${name}'`);
    }
    return node;
  }

  getMiningNode(name) {
    const miner = this.miners[name];
    if (!miner) {
      throw new Error(`Miner not found with name '${name}'`);
    }
    return miner;
  }

  getWallet(name) {
    const wallet = this.wallets[name] || this.walletsFFI[name];
    if (!wallet) {
      throw new Error(`Wallet not found with name '${name}'`);
    }
    return wallet;
  }

  getWalletPubkey(name) {
    return this.walletPubkeys[name];
  }

  async getNodeOrWalletClient(name) {
    let client = this.clients[name.trim()];
    if (client) {
      client.isNode = true;
      client.isWallet = false;
      return client;
    }
    let wallet = this.wallets[name.trim()];
    if (wallet) {
      client = await wallet.connectClient();
      client.isNode = false;
      client.isWallet = true;
      return client;
    }
    let ffi_wallet = this.walletsFFI[name.trim()];
    if (ffi_wallet) {
      return ffi_wallet;
    }
    return null;
  }

  async getOrCreateWallet(name) {
    const wallet = this.wallets[name];
    if (wallet) {
      return wallet;
    }
    await this.createAndAddWallet(name, this.seedAddresses());
    return this.wallets[name];
  }

  getProxy(name) {
    return this.proxies[name];
  }

  async forEachClientAsync(f, canFailPercent = 0) {
    const promises = [];
    let total = 0;
    let succeeded = 0;
    let failed = 0;

    for (const property in this.seeds) {
      promises.push(f(this.getClient(property), property));
      ++total;
    }
    for (const property in this.nodes) {
      promises.push(f(this.getClient(property), property));
      ++total;
    }

    // Round up the number of nodes that can fail.
    let canFail = Math.ceil((total * canFailPercent) / 100);

    return new Promise((resolve, reject) => {
      for (let promise of promises) {
        Promise.resolve(promise)
          .then(() => {
            succeeded += 1;
            console.log(`${succeeded} of ${total} (need ${total - canFail})`);
            if (succeeded >= total - canFail) resolve();
          })
          .catch((err) => {
            console.error(err);
            failed += 1;
            if (failed > canFail)
              reject(`Too many failed. Expected at most ${canFail} failures`);
          });
      }
    });
  }

  async stopNode(name) {
    const node = this.seeds[name] || this.nodes[name];
    await node.stop();
  }

  async startNode(name, args) {
    const node = this.seeds[name] || this.nodes[name];
    await node.start(args);
  }

  addTransaction(pubKey, txId) {
    if (!this.transactionsMap.has(pubKey)) {
      this.transactionsMap.set(pubKey, []);
    }
    this.transactionsMap.get(pubKey).push(txId);
  }
}

setWorldConstructor(CustomWorld);

BeforeAll({ timeout: 1200000 }, async function () {
  const baseNode = new BaseNodeProcess("compile");
  console.log("Compiling base node...");
  await baseNode.init();
  await baseNode.compile();

  const wallet = new WalletProcess("compile");
  console.log("Compiling wallet...");
  await wallet.init();
  await wallet.compile();

  const mmProxy = new MergeMiningProxyProcess(
    "compile",
    "127.0.0.1:9999",
    null,
    "127.0.0.1:9998"
  );
  console.log("Compiling mmproxy...");
  await mmProxy.init();
  await mmProxy.compile();

  const miningNode = new MiningNodeProcess(
    "compile",
    "127.0.0.1:9999",
    null,
    "127.0.0.1:9998"
    // this.logFilePathMiningNode
  );
  console.log("Compiling mining node...");
  await miningNode.init(1, 1, 1, 1, true, 1);
  await miningNode.compile();

  console.log("Compiling wallet FFI...");
  await InterfaceFFI.compile();
  await InterfaceFFI.init();
  console.log("Finished compilation.");
});

After(async function (testCase) {
  console.log("Stopping nodes");
  await stopAndHandleLogs(this.seeds, testCase, this);
  await stopAndHandleLogs(this.nodes, testCase, this);
  await stopAndHandleLogs(this.proxies, testCase, this);
  await stopAndHandleLogs(this.wallets, testCase, this);
  await stopAndHandleLogs(this.walletsFFI, testCase, this);
  await stopAndHandleLogs(this.miners, testCase, this);
});

async function stopAndHandleLogs(objects, testCase, context) {
  for (const key in objects) {
    if (testCase.result.status === "failed") {
      await attachLogs(`${objects[key].baseDir}`, context);
    }
    await objects[key].stop();
  }
}

function attachLogs(path, context) {
  return new Promise((outerRes) => {
    let zipFile = fs.createWriteStream("./temp/logzip.zip");
    const archive = archiver("zip", {
      zlib: { level: 9 },
    });
    archive.pipe(zipFile);

    glob(path + "/**/*.log", {}, function (err, files) {
      console.log(files);
      for (let i = 0; i < files.length; i++) {
        // Append the file name at the bottom
        fs.appendFileSync(files[i], `>>>> End of ${files[i]}`);
        archive.append(fs.createReadStream(files[i]), { name: files[i] });
      }
      archive.finalize().then(function () {
        context.attach(
          fs.createReadStream("./temp/logzip.zip"),
          "application/zip",
          function () {
            fs.rmSync && fs.rmSync("./temp/logzip.zip");
            outerRes();
          }
        );
      });
    });
  });
}
