const { setWorldConstructor, After, BeforeAll } = require("cucumber");

const BaseNodeProcess = require("../../helpers/baseNodeProcess");
const MergeMiningProxyProcess = require("../../helpers/mergeMiningProxyProcess");
const WalletProcess = require("../../helpers/walletProcess");
const MiningNodeProcess = require("../../helpers/miningNodeProcess");
const glob = require("glob");
const fs = require("fs");
const archiver = require("archiver");
class CustomWorld {
  constructor({ attach, parameters }) {
    // this.variable = 0;
    this.attach = attach;

    this.seeds = {};
    this.nodes = {};
    this.proxies = {};
    this.miners = {};
    this.wallets = {};
    this.walletPubkeys = {};
    this.clients = {};
    this.headers = {};
    this.outputs = {};
    this.testrun = `run${Date.now()}`;
    this.lastResult = null;
    this.blocks = {};
    this.transactions = {};
    this.peers = {};
    this.transactionsMap = new Map();
    this.resultStack = [];
    this.tipHeight = 0;
    this.logFilePathBaseNode =
      parameters.logFilePathBaseNode || "./log4rs/base_node.yml";
    this.logFilePathProxy = parameters.logFilePathProxy || "./log4rs/proxy.yml";
    this.logFilePathMiningNocde =
      parameters.logFilePathMiningNocde || "./log4rs/mining_node.yml";
    this.logFilePathWallet =
      parameters.logFilePathWallet || "./log4rs/wallet.yml";
  }

  async createSeedNode(name) {
    const proc = new BaseNodeProcess(
      `seed-${name}`,
      false,
      null,
      this.logFilePathBaseNode
    );
    await proc.startNew();
    this.seeds[name] = proc;
    this.clients[name] = proc.createGrpcClient();
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
    this.addNode(name, node);
  }

  addNode(name, process) {
    this.nodes[name] = process;
    this.clients[name] = process.createGrpcClient();
  }

  addMiningNode(name, process) {
    this.miners[name] = process;
  }

  addProxy(name, process) {
    this.proxies[name] = process;
  }

  async createAndAddWallet(name, nodeAddresses) {
    const wallet = new WalletProcess(name, false, {}, this.logFilePathWallet);
    wallet.setPeerSeeds([nodeAddresses]);
    await wallet.startNew();

    this.addWallet(name, wallet);
    let walletClient = wallet.getClient();
    let walletInfo = await walletClient.identify();
    this.walletPubkeys[name] = walletInfo.public_key;
  }

  addWallet(name, process) {
    this.wallets[name] = process;
  }

  addOutput(name, output) {
    this.outputs[name] = output;
  }

  async mineBlock(name, weight, beforeSubmit, onError) {
    await this.clients[name].mineBlockWithoutWallet(
      beforeSubmit,
      weight,
      onError
    );
  }

  baseNodeMineBlocksUntilHeightIncreasedBy(baseNode, wallet, numBlocks) {
    const promise = this.getClient(baseNode).mineBlocksUntilHeightIncreasedBy(
      numBlocks,
      wallet ? this.getWallet(wallet).getClient() : null
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
    return this.clients[name];
  }

  getNode(name) {
    return this.nodes[name] || this.seeds[name];
  }

  getMiningNode(name) {
    return this.miners[name];
  }

  getWallet(name) {
    return this.wallets[name];
  }

  getWalletPubkey(name) {
    return this.walletPubkeys[name];
  }

  async getOrCreateWallet(name) {
    const wallet = this.getWallet(name);
    if (wallet) {
      return wallet;
    }
    await this.createAndAddWallet(name, this.seedAddresses());
    return this.getWallet(name);
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
        Promise.resolve(promise).then(
          () => {
            succeeded += 1;
            console.log(`${succeeded} of ${total} (need ${total - canFail})`);
            if (succeeded >= total - canFail) resolve();
          },
          () => {
            failed += 1;
            if (failed > canFail) reject("Too many failed.");
          }
        );
      }
    });
  }

  async stopNode(name) {
    const node = this.seeds[name] || this.nodes[name];
    await node.stop();
  }

  async startNode(name) {
    const node = this.seeds[name] || this.nodes[name];
    await node.start();
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
    "127.0.0.1:9998",
    this.logFilePathMiningNocde
  );
  console.log("Compiling mining node...");
  await miningNode.init(1, 1, 1, 1, true, 1);
  await miningNode.compile();

  console.log("Finished compilation.");
});

After(async function (testCase) {
  console.log("Stopping nodes");
  await stopHandleLogs(this.seeds, testCase, this);
  await stopHandleLogs(this.nodes, testCase, this);
  await stopHandleLogs(this.proxies, testCase, this);
  await stopHandleLogs(this.wallets, testCase, this);
  await stopHandleLogs(this.miners, testCase, this);
  if (testCase.result.status === "failed") {
    throw "Logs contain atleast one Error message";
  }
});

async function stopHandleLogs(objects, testCase, context) {
  for (const key in objects) {
    scanForError(`${objects[key].baseDir}`, testCase);
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
            fs.rmSync("./temp/logzip.zip");
            outerRes();
          }
        );
      });
    });
  });
}

function scanForError(path, testCase) {
  glob(path + "/**/*.log", {}, function (err, files) {
    for (let i = 0; i < files.length; i++) {
      let fs = require("fs");
      let data = fs.readFileSync(files[i]).toString("UTF8");
      // we onbly search for the word error in the log file
      var regExp = RegExp("] ERROR ");
      if (regExp.test(data)) {
        testCase.result.status = "failed";
        console.log("The file: " + files[i] + " contains an error");
      }
    }
  });
}
