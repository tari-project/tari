const {
  setWorldConstructor,
  After,
  BeforeAll,
  Before,
} = require("@cucumber/cucumber");

const BaseNodeProcess = require("../../helpers/baseNodeProcess");
const StratumTranscoderProcess = require("../../helpers/stratumTranscoderProcess");
const ValidatorNodeProcess = require("../../helpers/validatorNodeProcess");
const MergeMiningProxyProcess = require("../../helpers/mergeMiningProxyProcess");
const WalletProcess = require("../../helpers/walletProcess");
const WalletFFIClient = require("../../helpers/walletFFIClient");
const MiningNodeProcess = require("../../helpers/miningNodeProcess");
const TransactionBuilder = require("../../helpers/transactionBuilder");
const glob = require("glob");
const fs = require("fs");
const archiver = require("archiver");
// const InterfaceFFI = require("../../helpers/ffi/ffiInterface");

class CustomWorld {
  constructor({ attach, parameters }) {
    // this.variable = 0;
    this.attach = attach;
    this.seeds = {};
    this.nodes = {};
    this.dan_nodes = {};
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

  currentBaseNodeName() {
    return Object.keys(this.nodes)[0];
  }

  currentWalletName() {
    return Object.keys(this.wallets)[0];
  }

  currentWallet() {
    return Object.values(this.wallets)[0];
  }

  /// Create but don't add the node
  createNode(name, options) {
    return new BaseNodeProcess(name, false, options, this.logFilePathBaseNode);
  }

  createDanNode(name, options) {
    return new ValidatorNodeProcess(
      name,
      false,
      options,
      this.logFilePathBaseNode
    );
  }

  async createAndAddDanNode(name) {
    const node = this.createDanNode(name);
    await node.init();
    await this.addDanNode(name, node);
  }

  async createAndAddNode(name, addresses) {
    console.log(`Creating node ${name} connected to ${addresses}`);
    const node = this.createNode(name);
    if (addresses) {
      if (Array.isArray(addresses)) {
        node.setPeerSeeds(addresses);
      } else {
        node.setPeerSeeds([addresses]);
      }
    }
    await node.startNew();
    await this.addNode(name, node);
  }

  async addDanNode(name, process) {
    this.dan_nodes[name] = process;
    // this.clients[name] = await process.createGrpcClient();
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
    console.log(`Creating wallet ${name} connected to ${nodeAddresses}`);
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
    this.lastTransactionsSucceeded = true;
    let result = true;
    const txInputs = this.transactionOutputs[height];
    if (txInputs == null) {
      return result;
    }
    let i = 0;
    const client = this.getClient(name);
    for (const input of txInputs) {
      // console.log(input);
      // console.log(await client.fetchMatchingUtxos(input.hash));

      const txn = new TransactionBuilder();
      txn.addInput(input);
      txn.changeFee(1);
      const txOutput = txn.addOutput(txn.getSpendableAmount());
      const completedTx = txn.build();

      const submitResult = await client.submitTransaction(completedTx);
      if (submitResult.result != "ACCEPTED") {
        this.lastTransactionsSucceeded = false;
        // result = false;
      } else {
        // Add the output to be spent... assumes it has been mined.
        this.addTransactionOutput(height + 1, txOutput);
      }
      i++;
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

  sha3MineBlocksUntilHeightIncreasedBy(
    miner,
    numBlocks,
    minDifficulty,
    mineOnTipOnly
  ) {
    const promise = this.getMiningNode(miner).mineBlocksUntilHeightIncreasedBy(
      numBlocks,
      minDifficulty,
      mineOnTipOnly
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
    const node = this.nodes[name] || this.seeds[name] || this.dan_nodes[name];
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

  async createMiningNode(name, node, wallet) {
    const baseNode = this.getNode(node);
    const walletNode = await this.getOrCreateWallet(wallet);
    const miningNode = new MiningNodeProcess(
      name,
      baseNode.getGrpcAddress(),
      this.getClient(node),
      walletNode.getGrpcAddress(),
      this.logFilePathMiningNode,
      true
    );
    this.addMiningNode(name, miningNode);
    return miningNode;
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
    console.log("\n", name, "stopped\n");
  }

  async startNode(name, args) {
    const node = this.seeds[name] || this.nodes[name];
    await node.start(args);
    console.log("\n", name, "started\n");
  }

  addTransaction(pubKey, txId) {
    if (!this.transactionsMap.has(pubKey)) {
      this.transactionsMap.set(pubKey, []);
    }
    this.transactionsMap.get(pubKey).push(txId);
  }
}

setWorldConstructor(CustomWorld);

BeforeAll({ timeout: 2400000 }, async function () {
  console.log(
    "NOTE: Some tests may be excluded based on the profile used in <root>/integration_tests/cucumber.js. If none was specified, `default` profile is used."
  );
  const baseNode = new BaseNodeProcess("compile");
  console.log("Compiling base node...");
  await baseNode.init();
  await baseNode.compile();

  // const danNode = new ValidatorNodeProcess("compile");
  // console.log("Compiling validator node...");
  // await danNode.init();
  // await danNode.compile();

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

  const stratumtranscoder = new StratumTranscoderProcess(
    "compile",
    "127.0.0.1:9999",
    "127.0.0.1:9998",
    null
  );

  console.log("Compiling stratum transcoder...");
  await stratumtranscoder.init();
  await stratumtranscoder.compile();

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

  // console.log("Compiling wallet FFI...");
  // await InterfaceFFI.compile();
  // console.log("Finished compilation.");
  // console.log("Loading FFI interface..");
  // await InterfaceFFI.init();
  // console.log("FFI interface loaded.");

  console.log("World ready, now lets run some tests! :)");
});

Before(async function (testCase) {
  console.log(`\nTesting scenario: "${testCase.pickle.name}"\n`);
});

After(async function (testCase) {
  console.log("Stopping nodes");
  await stopAndHandleLogs(this.walletsFFI, testCase, this);
  await stopAndHandleLogs(this.seeds, testCase, this);
  await stopAndHandleLogs(this.nodes, testCase, this);
  await stopAndHandleLogs(this.proxies, testCase, this);
  await stopAndHandleLogs(this.miners, testCase, this);
  await stopAndHandleLogs(this.dan_nodes, testCase, this);
  await stopAndHandleLogs(this.wallets, testCase, this);
});

async function stopAndHandleLogs(objects, testCase, context) {
  for (const key in objects) {
    try {
      if (testCase.result.status !== "passed") {
        await attachLogs(`${objects[key].baseDir}`, context);
      }
      await objects[key].stop();
    } catch (e) {
      console.log(e);
      // Continue with others
    }
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
      for (let i = 0; i < files.length; i++) {
        // Append the file name at the bottom
        fs.appendFileSync(files[i], `>>>> End of ${files[i]}`);
        archive.append(fs.createReadStream(files[i]), {
          name: files[i].replace("./temp", ""),
        });
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
