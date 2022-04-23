// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

const {
  setWorldConstructor,
  After,
  BeforeAll,
  Before,
} = require("@cucumber/cucumber");

const BaseNodeProcess = require("../../helpers/baseNodeProcess");
const ValidatorNodeProcess = require("../../helpers/validatorNodeProcess");
const MergeMiningProxyProcess = require("../../helpers/mergeMiningProxyProcess");
const WalletProcess = require("../../helpers/walletProcess");
const WalletFFIClient = require("../../helpers/walletFFIClient");
const MiningNodeProcess = require("../../helpers/miningNodeProcess");
const TransactionBuilder = require("../../helpers/transactionBuilder");
const glob = require("glob");
const fs = require("fs");
const archiver = require("archiver");
const { waitFor, sleep, consoleLogBalance } = require("../../helpers/util");
const { PaymentType } = require("../../helpers/types");
const { expect } = require("chai");
const InterfaceFFI = require("../../helpers/ffi/ffiInterface");

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
      parameters.logFilePathMiningNode || "./log4rs/miner.yml";
    this.logFilePathWallet =
      parameters.logFilePathWallet || "./log4rs/wallet.yml";
    this.lastResult = {};
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

  getRandomSeedName() {
    let keys = Object.keys(this.seeds);
    let r = Math.random() * keys.length;
    return keys[r];
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
    this.wallets[name.toString()] = process;
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

  async mineBlocks(name, num) {
    for (let i = 0; i < num; i++) {
      await this.mineBlock(name, 0);
    }
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

  async send_tari(
    sourceWallet,
    destWalletName,
    destWalletPubkey,
    tariAmount,
    feePerGram,
    oneSided = false,
    message = "",
    printMessage = true
  ) {
    const sourceWalletClient = await sourceWallet.connectClient();
    console.log(
      sourceWallet.name +
        " sending " +
        tariAmount +
        "uT one-sided(" +
        oneSided +
        ") to " +
        destWalletName +
        " `" +
        destWalletPubkey +
        "`"
    );
    if (printMessage) {
      console.log(message);
    }
    let success = false;
    let retries = 1;
    const retries_limit = 25;
    let lastResult;
    while (!success && retries <= retries_limit) {
      await waitFor(
        async () => {
          try {
            if (!oneSided) {
              lastResult = await sourceWalletClient.transfer({
                recipients: [
                  {
                    address: destWalletPubkey,
                    amount: tariAmount,
                    fee_per_gram: feePerGram,
                    message: message,
                  },
                ],
              });
            } else {
              lastResult = await sourceWalletClient.transfer({
                recipients: [
                  {
                    address: destWalletPubkey,
                    amount: tariAmount,
                    fee_per_gram: feePerGram,
                    message: message,
                    payment_type: PaymentType.ONE_SIDED,
                  },
                ],
              });
            }
          } catch (error) {
            console.log(error);
            return false;
          }
          return true;
        },
        true,
        20 * 1000,
        5 * 1000,
        5
      );
      success = lastResult.results[0].is_success;
      if (!success) {
        const wait_seconds = 5;
        console.log(
          "  " +
            lastResult.results[0].failure_message +
            ", trying again after " +
            wait_seconds +
            "s (" +
            retries +
            " of " +
            retries_limit +
            ")"
        );
        await sleep(wait_seconds * 1000);
        retries++;
      }
    }
    return lastResult;
  }

  async transfer(tariAmount, source, dest, feePerGram) {
    const sourceWallet = this.getWallet(source);
    const sourceClient = await sourceWallet.connectClient();
    const sourceInfo = await sourceClient.identify();

    const destPublicKey = this.getWalletPubkey(dest);

    this.lastResult = await this.send_tari(
      sourceWallet,
      dest,
      destPublicKey,
      tariAmount,
      feePerGram
    );
    expect(this.lastResult.results[0]["is_success"]).to.equal(true);
    this.addTransaction(
      sourceInfo.public_key,
      this.lastResult.results[0]["transaction_id"]
    );
    this.addTransaction(
      destPublicKey,
      this.lastResult.results[0]["transaction_id"]
    );
    console.log(
      "  Transaction '" +
        this.lastResult.results[0]["transaction_id"] +
        "' is_success(" +
        this.lastResult.results[0]["is_success"] +
        ")"
    );
    //lets now wait for this transaction to be at least broadcast before we continue.
    await waitFor(
      async () =>
        sourceClient.isTransactionAtLeastBroadcast(
          this.lastResult.results[0]["transaction_id"]
        ),
      true,
      60 * 1000,
      5 * 1000,
      5
    );
  }

  async waitForWalletToHaveBalance(wallet, amount) {
    const walletClient = await this.getWallet(wallet).connectClient();
    console.log("\n");
    console.log(
      "Waiting for " + wallet + " balance to be at least " + amount + " uT"
    );

    await waitFor(
      async () => await walletClient.isBalanceAtLeast(amount),
      true,
      115 * 1000,
      5 * 1000,
      5
    );
    consoleLogBalance(await walletClient.getBalance());
    if (!(await walletClient.isBalanceAtLeast(amount))) {
      console.log("Balance not adequate!");
    }
    expect(await walletClient.isBalanceAtLeast(amount)).to.equal(true);
  }

  async all_nodes_are_at_height(height) {
    await waitFor(
      async () => {
        let result = true;
        await this.forEachClientAsync(async (client, name) => {
          await waitFor(
            async () => await client.getTipHeight(),
            height,
            5 * height * 1000 /* 5 seconds per block */
          );
          const currTip = await client.getTipHeight();
          console.log(
            `Node ${name} is at tip: ${currTip} (should be ${height})`
          );
          result = result && currTip == height;
        });
        return result;
      },
      true,
      600 * 1000,
      5 * 1000,
      5
    );
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
    "/ip4/127.0.0.1/tcp/9999",
    null,
    "/ip4/127.0.0.1/tcp/9998"
  );

  console.log("Compiling mmproxy...");
  await mmProxy.init();
  await mmProxy.compile();

  const miningNode = new MiningNodeProcess(
    "compile",
    "/ip4/127.0.0.1/tcp/9999",
    null,
    "/ip4/127.0.0.1/tcp/9998"
    // this.logFilePathMiningNode
  );

  console.log("Compiling miner...");
  await miningNode.init(1, 1, 1, 1, true, 1);
  await miningNode.compile();

  console.log("Compiling wallet FFI...");
  await InterfaceFFI.compile();
  console.log("Finished compilation.");
  console.log("Loading FFI interface..");
  await InterfaceFFI.init();
  console.log("FFI interface loaded.");

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
    let zipFile = fs.createWriteStream(path + "/logzip.zip");
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
          fs.createReadStream(path + "/logzip.zip"),
          "application/zip",
          function () {
            fs.rmSync && fs.rmSync(path + "/logzip.zip");
            outerRes();
          }
        );
      });
    });
  });
}
