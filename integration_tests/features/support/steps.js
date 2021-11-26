// features/support/steps.js
const assert = require("assert");
const { Given, When, Then } = require("@cucumber/cucumber");
const StratumTranscoderProcess = require("../../helpers/stratumTranscoderProcess");
const MergeMiningProxyProcess = require("../../helpers/mergeMiningProxyProcess");
const MiningNodeProcess = require("../../helpers/miningNodeProcess");
const WalletProcess = require("../../helpers/walletProcess");
const expect = require("chai").expect;

const {
  waitFor,
  waitForPredicate,
  getTransactionOutputHash,
  sleep,
  consoleLogBalance,
  consoleLogTransactionDetails,
  withTimeout,
  waitForIterate,
} = require("../../helpers/util");
const { ConnectivityStatus, PaymentType } = require("../../helpers/types");
const TransactionBuilder = require("../../helpers/transactionBuilder");
let lastResult;

const AUTOUPDATE_HASHES_TXT_URL =
  "https://raw.githubusercontent.com/tari-project/tari/development/meta/hashes.txt";
const AUTOUPDATE_HASHES_TXT_SIG_URL =
  "https://raw.githubusercontent.com/tari-project/tari/development/meta/hashes.txt.sig";
const AUTOUPDATE_HASHES_TXT_BAD_SIG_URL =
  "https://raw.githubusercontent.com/tari-project/tari/development/meta/hashes.txt.bad.sig";

Given(/I have a seed node (.*)/, { timeout: 20 * 1000 }, async function (name) {
  return await this.createSeedNode(name);
});

Given("I have {int} seed nodes", { timeout: 20 * 1000 }, async function (n) {
  const promises = [];
  for (let i = 0; i < n; i++) {
    promises.push(this.createSeedNode(`SeedNode${i}`));
  }
  await Promise.all(promises);
});

Then(/all transactions must have succeeded/, function () {
  expect(this.lastTransactionsSucceeded).to.be(true);
});

Given(
  /I have a base node (.*) connected to all seed nodes/,
  { timeout: 20 * 1000 },
  async function (name) {
    await this.createAndAddNode(name, this.seedAddresses());
  }
);

Given(
  /I have a base node (.*) connected to seed (.*)/,
  { timeout: 20 * 1000 },
  async function (name, seedNode) {
    await this.createAndAddNode(name, this.seeds[seedNode].peerAddress());
  }
);

Given(
  /I have a base node (.*) connected to nodes (.*)/,
  { timeout: 20 * 1000 },
  async function (name, nodes) {
    const addresses = [];
    nodes = nodes.split(",");
    for (let i = 0; i < nodes.length; i++) {
      addresses.push(this.nodes[nodes[i]].peerAddress());
    }
    await this.createAndAddNode(name, addresses);
  }
);

Given(
  /I have a node (.*) with auto update enabled/,
  { timeout: 20 * 1000 },
  async function (name) {
    const node = await this.createNode(name, {
      common: {
        auto_update: {
          check_interval: 10,
          enabled: true,
          dns_hosts: ["_test_autoupdate.tari.io"],
          hashes_url: AUTOUPDATE_HASHES_TXT_URL,
          hashes_sig_url: AUTOUPDATE_HASHES_TXT_SIG_URL,
        },
      },
    });
    await node.startNew();
    await this.addNode(name, node);
  }
);

Given(
  /I have a node (.*) with auto update configured with a bad signature/,
  { timeout: 20 * 1000 },
  async function (name) {
    const node = await this.createNode(name, {
      common: {
        auto_update: {
          check_interval: 10,
          enabled: true,
          dns_hosts: ["_test_autoupdate.tari.io"],
          hashes_url: AUTOUPDATE_HASHES_TXT_URL,
          hashes_sig_url: AUTOUPDATE_HASHES_TXT_BAD_SIG_URL,
        },
      },
    });
    await node.startNew();
    await this.addNode(name, node);
  }
);

Given(
  /I have a wallet (.*) with auto update enabled/,
  { timeout: 20 * 1000 },
  async function (name) {
    await this.createAndAddWallet(name, "", {
      common: {
        auto_update: {
          check_interval: 10,
          enabled: true,
          dns_hosts: ["_test_autoupdate.tari.io"],
          hashes_url: AUTOUPDATE_HASHES_TXT_URL,
          hashes_sig_url: AUTOUPDATE_HASHES_TXT_SIG_URL,
        },
      },
    });
  }
);

Given(
  /I have a wallet (.*) with auto update configured with a bad signature/,
  { timeout: 20 * 1000 },
  async function (name) {
    await this.createAndAddWallet(name, "", {
      common: {
        auto_update: {
          check_interval: 10,
          enabled: true,
          dns_hosts: ["_test_autoupdate.tari.io"],
          hashes_url: AUTOUPDATE_HASHES_TXT_URL,
          hashes_sig_url: AUTOUPDATE_HASHES_TXT_BAD_SIG_URL,
        },
      },
    });
  }
);

Given(
  /I have a base node (.*) connected to node (.*)/,
  { timeout: 20 * 1000 },
  async function (name, node) {
    await this.createAndAddNode(name, this.nodes[node].peerAddress());
  }
);

Given(
  /I have a base node (\S+)$/,
  { timeout: 20 * 1000 },
  async function (name) {
    await this.createAndAddNode(name);
  }
);

Given(
  /I have a SHA3 miner (.*) connected to seed node (.*)/,
  { timeout: 20 * 1000 },
  async function (name, seed) {
    // add the base_node
    await this.createAndAddNode(name, this.seeds[seed].peerAddress(), this);
    const node = this.getNode(name);

    // Add the wallet connected to the above base node
    await this.createAndAddWallet(name, node.peerAddress(), this);

    // Now lets add a standalone miner to both
    const wallet = this.getWallet(name);
    const miningNode = new MiningNodeProcess(
      name,
      node.getGrpcAddress(),
      this.getClient(name),
      wallet.getGrpcAddress(),
      this.logFilePathMiningNode
    );
    this.addMiningNode(name, miningNode);
  }
);

Given(
  /I have a SHA3 miner (.*) connected to node (.*)/,
  { timeout: 20 * 1000 },
  async function (name, basenode) {
    // add the base_node
    await this.createAndAddNode(name, this.nodes[basenode].peerAddress(), this);
    const node = this.getNode(name);

    // Add the wallet connected to the above base node
    await this.createAndAddWallet(name, node.peerAddress(), this);

    // Now lets add a standalone miner to both
    const wallet = this.getWallet(name);
    const miningNode = new MiningNodeProcess(
      name,
      node.getGrpcAddress(),
      this.getClient(name),
      wallet.getGrpcAddress(),
      this.logFilePathMiningNode
    );
    this.addMiningNode(name, miningNode);
  }
);

Given(
  /I have a SHA3 miner (.*) connected to all seed nodes/,
  { timeout: 20 * 1000 },
  async function (name) {
    // add the base_node
    await this.createAndAddNode(name, this.seedAddresses(), this);
    const node = this.getNode(name);
    // Add the wallet connected to the above base node
    await this.createAndAddWallet(name, node.peerAddress(), this);

    // Now lets add a standalone miner to both

    const wallet = this.getWallet(name);
    const miningNode = new MiningNodeProcess(
      name,
      node.getGrpcAddress(),
      this.getClient(name),
      wallet.getGrpcAddress(),
      this.logFilePathMiningNode
    );
    this.addMiningNode(name, miningNode);
  }
);

Given(
  /I connect node (.*) to node (.*)/,
  { timeout: 20 * 1000 },
  async function (nodeNameA, nodeNameB) {
    console.log(
      "Connecting (add new peer seed, shut down, then start up)",
      nodeNameA,
      "to",
      nodeNameB
    );
    const nodeA = this.getNode(nodeNameA);
    const nodeB = this.getNode(nodeNameB);
    nodeA.setPeerSeeds([nodeB.peerAddress()]);
    console.log("Stopping node");
    await this.stopNode(nodeNameA);
    console.log("Starting node");
    await this.startNode(nodeNameA);
  }
);

Given(
  /I have a pruned node (.*) connected to node (.*) with pruning horizon set to (.*)/,
  { timeout: 20 * 1000 },
  async function (name, connected_to, horizon) {
    const node = this.createNode(name, { pruningHorizon: horizon });
    node.setPeerSeeds([this.nodes[connected_to].peerAddress()]);
    await node.startNew();
    await this.addNode(name, node);
  }
);

Given(
  /I have a lagging delayed node (.*) connected to node (.*) with blocks_behind_before_considered_lagging (\d+)/,
  { timeout: 20 * 1000 },
  async function (name, node, delay) {
    const miner = this.createNode(name, {
      blocks_behind_before_considered_lagging: delay,
    });
    miner.setPeerSeeds([this.nodes[node].peerAddress()]);
    await miner.startNew();
    await this.addNode(name, miner);
  }
);

Given(
  /I have a base node (.*) unconnected/,
  { timeout: 20 * 1000 },
  async function (name) {
    const node = this.createNode(name);
    await node.startNew();
    await this.addNode(name, node);
  }
);

Given(
  "I have {int} base nodes connected to all seed nodes",
  { timeout: 20 * 1000 },
  async function (n) {
    const promises = [];
    for (let i = 0; i < n; i++) {
      const miner = this.createNode(`BaseNode${i}`);
      miner.setPeerSeeds([this.seedAddresses()]);
      promises.push(
        miner.startNew().then(() => this.addNode(`BaseNode${i}`, miner))
      );
    }
    await Promise.all(promises);
  }
);

Given(
  /I have stress-test wallet (.*) connected to the seed node (.*) with broadcast monitoring timeout (.*)/,
  { timeout: 20 * 1000 },
  async function (walletName, seedName, timeout) {
    const wallet = new WalletProcess(
      walletName,
      false,
      { broadcastMonitoringTimeout: timeout },
      this.logFilePathWallet
    );
    wallet.setPeerSeeds([this.seeds[seedName].peerAddress()]);
    await wallet.startNew();
    this.addWallet(walletName, wallet);
    let walletClient = await this.getWallet(walletName).connectClient();
    let walletInfo = await walletClient.identify();
    this.addWalletPubkey(walletName, walletInfo.public_key);
  }
);

Given(
  /I have stress-test wallet (.*) connected to all the seed nodes with broadcast monitoring timeout (.*)/,
  { timeout: 20 * 1000 },
  async function (name, timeout) {
    const wallet = new WalletProcess(
      name,
      false,
      { broadcastMonitoringTimeout: timeout },
      this.logFilePathWallet
    );
    wallet.setPeerSeeds([this.seedAddresses()]);
    await wallet.startNew();
    this.addWallet(name, wallet);
    let walletClient = await this.getWallet(name).connectClient();
    let walletInfo = await walletClient.identify();
    this.addWalletPubkey(name, walletInfo.public_key);
  }
);

Given(
  /I have wallet (.*) connected to seed node (.*)/,
  { timeout: 20 * 1000 },
  async function (walletName, seedName) {
    await this.createAndAddWallet(
      walletName,
      this.seeds[seedName].peerAddress()
    );
  }
);

Given(
  /I have wallet (.*) connected to base node (.*)/,
  { timeout: 20 * 1000 },
  async function (walletName, nodeName) {
    await this.createAndAddWallet(
      walletName,
      this.nodes[nodeName].peerAddress()
    );
  }
);

Given(
  /I have wallet (.*) connected to all seed nodes/,
  { timeout: 20 * 1000 },
  async function (name) {
    await this.createAndAddWallet(name, this.seedAddresses());
  }
);

Given(
  /I have non-default wallet (.*) connected to all seed nodes using (.*)/,
  { timeout: 20 * 1000 },
  async function (name, mechanism) {
    // mechanism: DirectOnly, StoreAndForwardOnly, DirectAndStoreAndForward
    const wallet = new WalletProcess(
      name,
      false,
      { routingMechanism: mechanism },
      this.logFilePathWallet
    );
    console.log(wallet.name, wallet.options);
    wallet.setPeerSeeds([this.seedAddresses()]);
    await wallet.startNew();
    this.addWallet(name, wallet);
    let walletClient = await this.getWallet(name).connectClient();
    let walletInfo = await walletClient.identify();
    this.addWalletPubkey(name, walletInfo.public_key);
  }
);

Given(
  /I have (.*) non-default wallets connected to all seed nodes using (.*)/,
  { timeout: 190 * 1000 },
  async function (n, mechanism) {
    // mechanism: DirectOnly, StoreAndForwardOnly, DirectAndStoreAndForward
    const promises = [];
    for (let i = 0; i < n; i++) {
      let name = "Wallet_" + String(n).padStart(2, "0");
      promises.push(
        this.createAndAddWallet(name, [this.seedAddresses()], {
          routingMechanism: mechanism,
        })
      );
    }
    await Promise.all(promises);
  }
);

Given(
  /I recover wallet (.*) into wallet (.*) connected to all seed nodes/,
  { timeout: 30 * 1000 },
  async function (walletNameA, walletNameB) {
    const seedWords = this.getWallet(walletNameA).getSeedWords();
    console.log(
      "Recover " +
        walletNameA +
        " into " +
        walletNameB +
        ", seed words:\n  " +
        seedWords
    );
    const walletB = new WalletProcess(
      walletNameB,
      false,
      {},
      this.logFilePathWallet,
      seedWords
    );
    walletB.setPeerSeeds([this.seedAddresses()]);
    await walletB.startNew();
    this.addWallet(walletNameB, walletB);
    let walletClient = await this.getWallet(walletNameB).connectClient();
    let walletInfo = await walletClient.identify();
    this.addWalletPubkey(walletNameB, walletInfo.public_key);
  }
);

Given(
  /I recover wallet (.*) into (\d+) wallets connected to all seed nodes/,
  { timeout: 30 * 1000 },
  async function (walletNameA, numwallets) {
    const seedWords = this.getWallet(walletNameA).getSeedWords();
    for (let i = 1; i <= numwallets; i++) {
      console.log(
        "Recover " +
          walletNameA +
          " into wallet " +
          i +
          ", seed words:\n  " +
          seedWords
      );
      const wallet = new WalletProcess(
        i,
        false,
        {},
        this.logFilePathWallet,
        seedWords
      );
      wallet.setPeerSeeds([this.seedAddresses()]);
      await wallet.startNew();
      this.addWallet(i, wallet);
      let walletClient = await this.getWallet(i.toString()).connectClient();
      let walletInfo = await walletClient.identify();
      this.addWalletPubkey(wallet, walletInfo.public_key);
    }
  }
);

Then(
  /I wait for (\d+) wallets to have at least (\d+) uT/,
  { timeout: 60 * 1000 },
  async function (numwallets, amount) {
    for (let i = 1; i <= numwallets; i++) {
      const walletClient = await this.getWallet(i.toString()).connectClient();
      console.log("\n");
      console.log(
        "Waiting for wallet " + i + " balance to be at least " + amount + " uT"
      );

      await waitFor(
        async () => walletClient.isBalanceAtLeast(amount),
        true,
        20 * 1000,
        5 * 1000,
        5
      );
      consoleLogBalance(await walletClient.getBalance());
      if (!(await walletClient.isBalanceAtLeast(amount))) {
        console.log("Balance not adequate!");
      }
      expect(await walletClient.isBalanceAtLeast(amount)).to.equal(true);
    }
  }
);

Then(
  /Wallet (.*) and (\d+) wallets have the same balance/,
  { timeout: 120 * 1000 },
  async function (wallet, numwallets) {
    const walletClient = await this.getWallet(wallet).connectClient();
    let balance = await walletClient.getBalance();
    for (let i = 1; i <= numwallets; i++) {
      const walletClient2 = await this.getWallet(i.toString()).connectClient();
      let balance2 = await walletClient2.getBalance();
      expect(balance === balance2);
    }
  }
);

When(/I stop wallet (.*)/, async function (walletName) {
  let wallet = this.getWallet(walletName);
  await wallet.stop();
});

When(/I start wallet (.*)/, async function (walletName) {
  let wallet = this.getWallet(walletName);
  await wallet.start();
});

When(/I restart wallet (.*)/, async function (walletName) {
  let wallet = this.getWallet(walletName);
  await wallet.stop();
  await wallet.start();
});

When(
  /I import (.*) spent outputs to (.*)/,
  async function (walletNameA, walletNameB) {
    let walletA = this.getWallet(walletNameA);
    let walletB = this.getWallet(walletNameB);
    let clientB = await walletB.connectClient();

    await walletA.exportSpentOutputs();
    let spent_outputs = await walletA.readExportedOutputs();
    let result = await clientB.importUtxos(spent_outputs);
    lastResult = result.tx_ids;
  }
);

When(
  /I import (.*) unspent outputs to (.*)/,
  async function (walletNameA, walletNameB) {
    let walletA = this.getWallet(walletNameA);
    let walletB = this.getWallet(walletNameB);
    let clientB = await walletB.connectClient();

    await walletA.exportUnspentOutputs();
    let outputs = await walletA.readExportedOutputs();
    let result = await clientB.importUtxos(outputs);
    lastResult = result.tx_ids;
  }
);

When(
  /I import (.*) unspent outputs as faucet outputs to (.*)/,
  async function (walletNameA, walletNameB) {
    let walletA = this.getWallet(walletNameA);
    let walletB = this.getWallet(walletNameB);
    let clientB = await walletB.connectClient();

    await walletA.exportUnspentOutputs();
    let outputs = await walletA.readExportedOutputsAsFaucetOutputs();
    let result = await clientB.importUtxos(outputs);
    lastResult = result.tx_ids;
  }
);

When(
  /I check if last imported transactions are invalid in wallet (.*)/,
  async function (walletName) {
    let wallet = this.getWallet(walletName);
    let client = await wallet.connectClient();
    let found_txs = await client.getCompletedTransactions();
    //console.log("Found: ", found_txs);
    let found_count = 0;
    for (let imported_tx = 0; imported_tx < lastResult.length; imported_tx++) {
      for (let found_tx = 0; found_tx < found_txs.length; found_tx++) {
        if (found_txs[found_tx].tx_id === lastResult[imported_tx]) {
          found_count++;
          expect(found_txs[found_tx].status).to.equal(
            "TRANSACTION_STATUS_IMPORTED"
          );
          expect(found_txs[found_tx].valid).to.equal(false);
        }
      }
    }
    expect(found_count).to.equal(lastResult.length);
  }
);

When(
  /I check if wallet (.*) has (.*) transactions/,
  async function (walletName, count) {
    let wallet = this.getWallet(walletName);
    let client = await wallet.connectClient();
    let txs = await client.getCompletedTransactions();
    expect(count).to.equal(txs.length.toString());
  }
);

When(
  /I check if last imported transactions are valid in wallet (.*)/,
  async function (walletName) {
    let wallet = this.getWallet(walletName);
    let client = await wallet.connectClient();
    let found_txs = await client.getCompletedTransactions();

    let found_count = 0;
    for (let imported_tx = 0; imported_tx < lastResult.length; imported_tx++) {
      for (let found_tx = 0; found_tx < found_txs.length; found_tx++) {
        if (found_txs[found_tx].tx_id === lastResult[imported_tx]) {
          found_count++;
          expect(found_txs[found_tx].status).to.equal(
            "TRANSACTION_STATUS_IMPORTED"
          );
          expect(found_txs[found_tx].valid).to.equal(true);
        }
      }
    }
    expect(found_count).to.equal(lastResult.length);
  }
);

Given(
  /I have a merge mining proxy (.*) connected to (.*) and (.*) with default config/,
  { timeout: 120 * 1000 }, // The timeout must make provision for testing the monerod URL /get_height response
  async function (mmProxy, node, wallet) {
    const baseNode = this.getNode(node);
    const walletNode = this.getWallet(wallet);
    const proxy = new MergeMiningProxyProcess(
      mmProxy,
      baseNode.getGrpcAddress(),
      this.getClient(node),
      walletNode.getGrpcAddress(),
      this.logFilePathProxy,
      true
    );
    await proxy.startNew();
    this.addProxy(mmProxy, proxy);
  }
);

Given(
  /I have a merge mining proxy (.*) connected to (.*) and (.*) with origin submission disabled/,
  { timeout: 120 * 1000 }, // The timeout must make provision for testing the monerod URL /get_height response
  async function (mmProxy, node, wallet) {
    const baseNode = this.getNode(node);
    const walletNode = this.getWallet(wallet);
    const proxy = new MergeMiningProxyProcess(
      mmProxy,
      baseNode.getGrpcAddress(),
      this.getClient(node),
      walletNode.getGrpcAddress(),
      this.logFilePathProxy,
      false
    );
    await proxy.startNew();
    this.addProxy(mmProxy, proxy);
  }
);

Given(
  /I have a merge mining proxy (.*) connected to (.*) and (.*) with origin submission enabled/,
  { timeout: 120 * 1000 }, // The timeout must make provision for testing the monerod URL /get_height response
  async function (mmProxy, node, wallet) {
    const baseNode = this.getNode(node);
    const walletNode = this.getWallet(wallet);
    const proxy = new MergeMiningProxyProcess(
      mmProxy,
      baseNode.getGrpcAddress(),
      this.getClient(node),
      walletNode.getGrpcAddress(),
      this.logFilePathProxy,
      true
    );
    await proxy.startNew();
    this.addProxy(mmProxy, proxy);
  }
);

Given(
  /I have mining node (.*) connected to base node (.*) and wallet (.*)/,
  async function (miner, node, wallet) {
    await this.createMiningNode(miner, node, wallet);
  }
);

Given(
  /I have mine-before-tip mining node (.*) connected to base node (.*) and wallet (.*)/,
  function (miner, node, wallet) {
    const baseNode = this.getNode(node);
    const walletNode = this.getWallet(wallet);
    const miningNode = new MiningNodeProcess(
      miner,
      baseNode.getGrpcAddress(),
      this.getClient(node),
      walletNode.getGrpcAddress(),
      this.logFilePathMiningNode,
      false
    );
    this.addMiningNode(miner, miningNode);
  }
);

When(/I ask for a block height from proxy (.*)/, async function (mmProxy) {
  lastResult = "NaN";
  const proxy = this.getProxy(mmProxy);
  const proxyClient = proxy.createClient();
  const height = await proxyClient.getHeight();
  lastResult = height;
});

Then("Proxy response height is valid", function () {
  assert(Number.isInteger(lastResult), true);
});

When(/I ask for a block template from proxy (.*)/, async function (mmProxy) {
  lastResult = {};
  const proxy = this.getProxy(mmProxy);
  const proxyClient = proxy.createClient();
  const template = await proxyClient.getBlockTemplate();
  lastResult = template;
});

Then("Proxy response block template is valid", function () {
  assert(typeof lastResult === "object" && lastResult !== null, true);
  assert(typeof lastResult._aux !== "undefined", true);
  assert(lastResult.status, "OK");
});

When(/I submit a block through proxy (.*)/, async function (mmProxy) {
  const blockTemplateBlob = lastResult.blocktemplate_blob;
  const proxy = this.getProxy(mmProxy);
  const proxyClient = proxy.createClient();
  const result = await proxyClient.submitBlock(blockTemplateBlob);
  lastResult = result;
});

Then(
  "Proxy response block submission is valid with submitting to origin",
  function () {
    assert(
      typeof lastResult.result === "object" && lastResult.result !== null,
      true
    );
    assert(typeof lastResult.result._aux !== "undefined", true);
    assert(lastResult.result.status, "OK");
  }
);

Then(
  "Proxy response block submission is valid without submitting to origin",
  function () {
    assert(lastResult.result !== null, true);
    assert(lastResult.status, "OK");
  }
);

When(
  /I ask for the last block header from proxy (.*)/,
  async function (mmProxy) {
    const proxy = this.getProxy(mmProxy);
    const proxyClient = proxy.createClient();
    const result = await proxyClient.getLastBlockHeader();
    lastResult = result;
  }
);

Then("Proxy response for last block header is valid", function () {
  assert(typeof lastResult === "object" && lastResult !== null, true);
  assert(typeof lastResult.result._aux !== "undefined", true);
  assert(lastResult.result.status, "OK");
  lastResult = lastResult.result.block_header.hash;
});

When(
  /I ask for a block header by hash using last block header from proxy (.*)/,
  async function (mmProxy) {
    const proxy = this.getProxy(mmProxy);
    const proxyClient = proxy.createClient();
    const result = await proxyClient.getBlockHeaderByHash(lastResult);
    lastResult = result;
  }
);

Then("Proxy response for block header by hash is valid", function () {
  assert(typeof lastResult === "object" && lastResult !== null, true);
  assert(lastResult.result.status, "OK");
});

When(/I start base node (.*)/, { timeout: 20 * 1000 }, async function (name) {
  await this.startNode(name);
});

When(
  /I run blockchain recovery on node (\S*)/,
  { timeout: 120 * 1000 },
  async function (name) {
    await this.startNode(name, ["--rebuild-db"]);
  }
);

When(/I stop node (.*)/, async function (name) {
  await this.stopNode(name);
});

Then(
  /node (.*) is at height (\d+)/,
  { timeout: 600 * 1000 },
  async function (name, height) {
    const client = this.getClient(name);
    const currentHeight = await waitForIterate(
      () => client.getTipHeight(),
      height,
      1000,
      5 * height // 5 seconds per block
    );
    console.log(
      `Node ${name} is at tip: ${currentHeight} (should be`,
      height,
      `)`
    );
    expect(currentHeight).to.equal(height);
  }
);

Then(
  /node (.*) has a pruned height of (\d+)/,
  { timeout: 600 * 1000 },
  async function (name, height) {
    const client = this.getClient(name);
    await waitFor(
      async () => await client.getPrunedHeight(),
      height,
      1000,
      height * 5 * 1000 // 5 seconds per block
    );
    const currentHeight = await client.getPrunedHeight();
    console.log(
      `Node ${name} has a pruned height: ${currentHeight} (should be`,
      height,
      `)`
    );
    expect(currentHeight).to.equal(height);
  }
);

Then(
  /node (.*) is at the same height as node (.*)/,
  { timeout: 120 * 1000 },
  async function (nodeA, nodeB) {
    var expectedHeight, currentHeight;
    expectedHeight = parseInt(await this.getClient(nodeB).getTipHeight());
    for (let i = 1; i <= 12; i++) {
      await waitFor(
        async () => this.getClient(nodeA).getTipHeight(),
        expectedHeight,
        10 * 1000
      );
      expectedHeight = parseInt(await this.getClient(nodeB).getTipHeight());
      currentHeight = await this.getClient(nodeA).getTipHeight();
      if (currentHeight === expectedHeight) {
        break;
      }
    }
    console.log(
      `Node ${nodeA} is at tip: ${currentHeight} (should be`,
      expectedHeight,
      ")"
    );
    expect(currentHeight).to.equal(expectedHeight);
  }
);

Then(
  "all nodes are on the same chain at height {int}",
  { timeout: 600 * 1000 },
  async function (height) {
    let tipHash = null;
    await this.forEachClientAsync(async (client, name) => {
      await waitForIterate(
        () => client.getTipHeight(),
        height,
        1000,
        5 * height /* 5 seconds per block */
      );
      const currTip = await client.getTipHeader();
      console.log(
        `${client.name} is at tip ${currTip.height} (${currTip.hash.toString(
          "hex"
        )})`
      );
      expect(currTip.height).to.equal(height);
      if (!tipHash) {
        tipHash = currTip.hash.toString("hex");
        console.log(`Node ${name} is at tip: ${tipHash}`);
      } else {
        const currTipHash = currTip.hash.toString("hex");
        console.log(
          `Node ${name} is at tip: ${currTipHash} (should be ${tipHash})`
        );
        expect(currTipHash).to.equal(tipHash);
      }
    });
  }
);

Then(
  "all nodes are on the same chain tip",
  { timeout: 800 * 1000 },
  async function () {
    await waitFor(
      async () => {
        let tipHash = null;
        let height = null;
        let result = true;
        await this.forEachClientAsync(async (client, name) => {
          const currTip = await client.getTipHeader();
          if (!tipHash) {
            tipHash = currTip.hash.toString("hex");
            height = currTip.height;
            console.log(`Node ${name} is at tip: #${height}, ${tipHash}`);
          } else {
            const currTipHash = currTip.hash.toString("hex");
            console.log(
              `Node ${name} is at tip: #${currTip.height},${currTipHash} (should be #${height},${tipHash})`
            );
            result =
              result && currTipHash == tipHash && currTip.height == height;
          }
        });
        return result;
      },
      true,
      600 * 1000,
      5 * 1000,
      5
    );
  }
);

Then(
  "all nodes are at height {int}",
  { timeout: 800 * 1000 },
  async function (height) {
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
);

Then(
  /node (.*) has reached initial sync/,
  { timeout: 21 * 60 * 1000 },
  async function (node) {
    const client = this.getClient(node);
    await waitForPredicate(
      async () => await client.initial_sync_achieved(),
      20 * 60 * 1000,
      1000
    );
    let result = await this.getClient(node).initial_sync_achieved();
    console.log(`Node ${node} response is: ${result}`);
    expect(result).to.equal(true);
  }
);

Then(/node (.*) is in state (.*)/, async function (node, state) {
  const client = this.getClient(node);
  await waitForPredicate(
    async () => (await client.get_node_state()) == state,
    20 * 60 * 1000,
    1000
  );
  let result = await this.getClient(node).get_node_state();
  console.log(`Node ${node} is in the current state: ${result}`);
  expect(result).to.equal(state);
});

Then(
  /(.*) does not have a new software update/,
  { timeout: 65 * 1000 },
  async function (name) {
    let client = await this.getNodeOrWalletClient(name);
    await sleep(5000);
    await waitFor(
      async () => (await client.checkForUpdates()).has_update,
      false,
      60 * 1000
    );
    expect(
      (await client.checkForUpdates()).has_update,
      "There should be no update"
    ).to.be.false;
  }
);

Then(
  /(.+) has a new software update/,
  { timeout: 65 * 1000 },
  async function (name) {
    let client = await this.getNodeOrWalletClient(name);
    await waitFor(
      async () => (await client.checkForUpdates()).has_update,
      true,
      60 * 1000
    );
    expect(
      (await client.checkForUpdates()).has_update,
      "There should be update"
    ).to.be.true;
  }
);

Then(
  /all nodes are at the same height as node (.*)/,
  { timeout: 120 * 1000 },
  async function (nodeB) {
    let expectedHeight = parseInt(await this.getClient(nodeB).getTipHeight());
    console.log("Wait for all nodes to reach height of", expectedHeight);
    await this.forEachClientAsync(async (client, name) => {
      const newExpectedHeight = parseInt(
        await this.getClient(nodeB).getTipHeight()
      );
      if (newExpectedHeight !== expectedHeight) {
        expectedHeight = newExpectedHeight;
        console.log("Wait for all nodes to reach height of", expectedHeight);
      }
      let currentHeight;
      for (let i = 1; i <= 12; i++) {
        await waitFor(
          async () => await client.getTipHeight(),
          expectedHeight,
          10 * 1000
        );
        expectedHeight = parseInt(await this.getClient(nodeB).getTipHeight());
        currentHeight = parseInt(await client.getTipHeight());
        if (currentHeight === expectedHeight) {
          break;
        }
      }
      console.log(
        `Node ${name} is at tip: ${currentHeight} (should be`,
        expectedHeight,
        ")"
      );
      expect(currentHeight).to.equal(expectedHeight);
    });
  }
);

Then(
  /meddling with block template data from node (.*) is not allowed/,
  async function (baseNodeName) {
    const baseNodeClient = this.getClient(baseNodeName);

    // No meddling with data
    // - Current tip
    const currHeight = await baseNodeClient.getTipHeight();
    // - New block
    let newBlock = await baseNodeClient.mineBlockBeforeSubmit(0);
    // - Submit block to base node
    await baseNodeClient.submitMinedBlock(newBlock);
    // - Verify new height
    expect(await baseNodeClient.getTipHeight()).to.equal(currHeight + 1);

    // Meddle with data - kernel_mmr_size
    // - New block
    newBlock = await baseNodeClient.mineBlockBeforeSubmit(0);
    // - Change kernel_mmr_size
    newBlock.block.header.kernel_mmr_size =
      parseInt(newBlock.block.header.kernel_mmr_size) + 1;
    // - Try to submit illegal block to base node
    try {
      await baseNodeClient.submitMinedBlock(newBlock);
      expect("Meddling with MMR size for Kernel not detected!").to.equal("");
    } catch (err) {
      console.log(
        "\nMeddle with kernel_mmr_size - error details (as expected):\n",
        err.details
      );
      expect(
        err.details.includes(
          "Block validation error: MMR size for Kernel does not match."
        )
      ).to.equal(true);
    }

    // Meddle with data - output_mmr_size
    // - New block
    newBlock = await baseNodeClient.mineBlockBeforeSubmit(0);
    // - Change output_mmr_size
    newBlock.block.header.output_mmr_size =
      parseInt(newBlock.block.header.output_mmr_size) + 1;
    // - Try to submit illegal block to base node
    try {
      await baseNodeClient.submitMinedBlock(newBlock);
      expect("Meddling with MMR size for UTXO not detected!").to.equal("");
    } catch (err) {
      console.log(
        "Meddle with output_mmr_size - error details (as expected):\n",
        err.details
      );
      expect(
        err.details.includes(
          "Block validation error: MMR size for UTXO does not match."
        )
      ).to.equal(true);
    }
  }
);

When(
  /I create a transaction (.*) spending (.*) to (.*)/,
  function (txnName, inputs, output) {
    const txInputs = inputs.split(",").map((input) => this.outputs[input]);
    const txn = new TransactionBuilder();
    txInputs.forEach((txIn) => txn.addInput(txIn));
    const txOutput = txn.addOutput(txn.getSpendableAmount());
    this.addOutput(output, txOutput);
    this.transactions[txnName] = txn.build();
  }
);

When(
  /I create a custom fee transaction (.*) spending (.*) to (.*) with fee (\d+)/,
  function (txnName, inputs, output, fee) {
    const txInputs = inputs.split(",").map((input) => this.outputs[input]);
    const txn = new TransactionBuilder();
    txn.changeFee(fee);
    txInputs.forEach((txIn) => txn.addInput(txIn));
    const txOutput = txn.addOutput(txn.getSpendableAmount());
    this.addOutput(output, txOutput);
    this.transactions[txnName] = txn.build();
  }
);

When(/I submit transaction (.*) to (.*)/, async function (txn, node) {
  this.lastResult = await this.getClient(node).submitTransaction(
    this.transactions[txn]
  );
  expect(this.lastResult.result).to.equal("ACCEPTED");
});

When(/I submit locked transaction (.*) to (.*)/, async function (txn, node) {
  this.lastResult = await this.getClient(node).submitTransaction(
    this.transactions[txn]
  );
  expect(this.lastResult.result).to.equal("REJECTED");
});

When(/I spend outputs (.*) via (.*)/, async function (inputs, node) {
  const txInputs = inputs.split(",").map((input) => this.outputs[input]);
  console.log(txInputs);
  const txn = new TransactionBuilder();
  txInputs.forEach((txIn) => txn.addInput(txIn));
  console.log(txn.getSpendableAmount());
  const output = txn.addOutput(txn.getSpendableAmount());
  console.log(output);
  this.lastResult = await this.getClient(node).submitTransaction(txn.build());
  expect(this.lastResult.result).to.equal("ACCEPTED");
});

Then(
  /(.*) has (.*) in (.*) state/,
  { timeout: 6 * 60 * 1000 }, // Must cater for long running transaction state changes, e.g. UNKNOWN -> NOT_STORED
  async function (node, txn, pool) {
    const client = this.getClient(node);
    const sig = this.transactions[txn].body.kernels[0].excess_sig;
    this.lastResult = await waitFor(
      async () => {
        let tx_result = await client.transactionStateResult(sig);
        console.log(
          `Node ${node} response for ${txn} is: ${tx_result}, should be: ${pool}`
        );
        return tx_result === pool;
      },
      true,
      6 * 60 * 1000,
      5 * 1000
    );
    expect(this.lastResult).to.equal(true);
  }
);

// The number is rounded down. E.g. if 1% can fail out of 17, that is 16.83 have to succeed.
// It's means at least 16 have to succeed.
Then(
  /(.*) is in the (.*) of all nodes(, where (\d+)% can fail)?/,
  { timeout: 120 * 1000 },
  async function (txn, pool, canFail) {
    const sig = this.transactions[txn].body.kernels[0].excess_sig;
    await this.forEachClientAsync(
      async (client, name) => {
        await waitFor(
          async () => await client.transactionStateResult(sig),
          pool,
          115 * 1000
        );
        this.lastResult = await client.transactionState(sig);
        console.log(`Node ${name} response is: ${this.lastResult.result}`);
      },
      canFail ? parseInt(canFail) : 0
    );
  }
);

Then(/(.*) is in the mempool/, function (_txn) {
  expect(this.lastResult.result).to.equal("ACCEPTED");
});

Then(/(.*) should not be in the mempool/, function (_txn) {
  expect(this.lastResult.result).to.equal("REJECTED");
});

When(/I save the tip on (.*) as (.*)/, async function (node, name) {
  const client = this.getClient(node);
  const header = await client.getTipHeader();
  this.headers[name] = header;
});

Then(/node (.*) is at tip (.*)/, async function (node, name) {
  const client = this.getClient(node);
  const header = await client.getTipHeader();
  // console.log("headers:", this.headers);
  const existingHeader = this.headers[name];
  expect(existingHeader).to.not.be.null;
  expect(existingHeader.hash.toString("hex")).to.equal(
    header.hash.toString("hex")
  );
});

Then(
  /node (.*) lists headers (\d+) to (\d+) with correct heights/,
  async function (node, start, end) {
    const client = this.getClient(node);
    const fromHeight = end;
    const numHeaders = end - start + 1; // inclusive
    const headers = await client.getHeaders(fromHeight, numHeaders);
    const heights = headers.map((header) => parseInt(header.height));
    for (let height = start; height <= end; height++) {
      expect(heights).to.contain(height);
    }
  }
);

When(
  /I mine a block on (.*) with coinbase (.*)/,
  async function (name, coinbaseName) {
    const tipHeight = await this.getClient(name).getTipHeight();
    let autoTransactionResult = await this.createTransactions(
      name,
      tipHeight + 1
    );
    expect(autoTransactionResult).to.equal(true);
    await this.mineBlock(name, 0, (candidate) => {
      this.addOutput(coinbaseName, candidate.originalTemplate.coinbase);
      return candidate;
    });
  }
);

When(
  /I mine (\d+) custom weight blocks on (.*) with weight (\d+)/,
  { timeout: 1200 * 1000 }, // Must allow many blocks to be mined; time out below limits each block to be mined
  async function (numBlocks, name, weight) {
    const tipHeight = await this.getClient(name).getTipHeight();
    for (let i = 0; i < numBlocks; i++) {
      let autoTransactionResult = await this.createTransactions(
        name,
        tipHeight + i + 1
      );
      expect(autoTransactionResult).to.equal(true);
      // If a block cannot be mined quickly enough (or the process has frozen), timeout.
      await withTimeout(
        5 * 1000,
        this.mineBlock(name, parseInt(weight), (candidate) => {
          this.addTransactionOutput(
            tipHeight + i + 1 + 2,
            candidate.originalTemplate.coinbase
          );
          return candidate;
        })
      );
    }
  }
);

When(
  /mining node (.*) mines (\d+) blocks with min difficulty (\d+) and max difficulty (\d+)/,
  { timeout: 1200 * 1000 }, // Must allow many blocks to be mined; dynamic time out below limits actual time
  async function (miner, numBlocks, min, max) {
    const miningNode = this.getMiningNode(miner);
    await miningNode.init(
      numBlocks,
      null,
      min,
      max,
      miningNode.mineOnTipOnly,
      null
    );
    await withTimeout(
      (10 + parseInt(numBlocks) * 1) * 1000,
      await miningNode.startNew()
    );
  }
);

When(
  /I mine (.*) blocks with difficulty (.*) on (.*)/,
  { timeout: 20 * 1000 },
  async function (numBlocks, difficulty, node) {
    const miner = await this.createMiningNode("temp", node, "temp");
    await miner.init(
      parseInt(numBlocks),
      null,
      parseInt(difficulty),
      parseInt(difficulty),
      false,
      null
    );
    await miner.startNew();
  }
);

When(
  /mining node (.*) mines (\d+) blocks$/,
  { timeout: 1200 * 1000 }, // Must allow many blocks to be mined; dynamic time out below limits actual time
  async function (miner, numBlocks) {
    const miningNode = this.getMiningNode(miner);
    // Don't wait for sync before mining. Also use a max difficulty of 1, since most tests assume
    // that 1 block = 1 difficulty
    await miningNode.init(numBlocks, null, 1, 1, false, null);
    await withTimeout(
      (10 + parseInt(numBlocks) * 1) * 1000,
      await miningNode.startNew()
    );
  }
);

When(
  /I update the parent of block (.*) to be an orphan/,
  async function (_block) {
    // TODO
  }
);

When(
  /I mine (\d+) blocks on (.*)/,
  { timeout: 1200 * 1000 }, // Must allow many blocks to be mined; time out below limits each block to be mined
  async function (numBlocks, name) {
    const tipHeight = await this.getClient(name).getTipHeight();
    for (let i = 0; i < numBlocks; i++) {
      let autoTransactionResult = await this.createTransactions(
        name,
        tipHeight + i + 1
      );
      expect(autoTransactionResult).to.equal(true);
      await withTimeout(
        5 * 1000,
        this.mineBlock(name, 0, (candidate) => {
          this.addTransactionOutput(
            tipHeight + i + 1 + 2,
            candidate.originalTemplate.coinbase
          );
          return candidate;
        })
      );
    }
  }
);

When(
  /I mine (\d+) blocks using wallet (.*) on (.*)/,
  { timeout: 1200 * 1000 }, // Must allow many blocks to be mined; time out below limits each block to be mined
  async function (numBlocks, walletName, nodeName) {
    const nodeClient = this.getClient(nodeName);
    const walletClient = await this.getWallet(walletName).connectClient();
    const tipHeight = await this.getClient(nodeName).getTipHeight();
    for (let i = 0; i < numBlocks; i++) {
      let autoTransactionResult = await this.createTransactions(
        nodeName,
        tipHeight + 1 + i
      );
      expect(autoTransactionResult).to.equal(true);
      await withTimeout(5 * 1000, await nodeClient.mineBlock(walletClient));
    }
  }
);

When(
  /I merge mine (.*) blocks via (.*)/,
  { timeout: 1200 * 1000 }, // Must allow many blocks to be mined; time out below limits each block to be mined
  async function (numBlocks, mmProxy) {
    for (let i = 0; i < numBlocks; i++) {
      await withTimeout(5 * 1000, await this.mergeMineBlock(mmProxy));
    }
  }
);

When(
  /I co-mine (.*) blocks via merge mining proxy (.*) and mining node (.*)/,
  { timeout: 15000 * 1000 }, // Must allow many blocks to be mined; dynamic time out below limits actual time
  async function (numBlocks, mmProxy, miner) {
    const sha3MiningPromise = withTimeout(
      parseInt(numBlocks) * 4 * 1000,
      this.sha3MineBlocksUntilHeightIncreasedBy(miner, numBlocks, 120, true)
    );
    const mergeMiningPromise = withTimeout(
      parseInt(numBlocks) * 4 * 1000,
      this.mergeMineBlocksUntilHeightIncreasedBy(mmProxy, numBlocks)
    );
    await Promise.all([sha3MiningPromise, mergeMiningPromise]).then(
      ([res1, res2]) => {
        console.log(
          "Co-mining",
          numBlocks,
          "blocks concluded, tips at [",
          res1,
          ",",
          res2,
          "]"
        );
      }
    );
  }
);

When(
  /I mine but do not submit a block (.*) on (.*)/,
  async function (blockName, nodeName) {
    const tipHeight = await this.getClient(nodeName).getTipHeight();
    let autoTransactionResult = await this.createTransactions(
      nodeName,
      tipHeight + 1
    );
    expect(autoTransactionResult).to.equal(true);
    await this.mineBlock(
      nodeName,
      null,
      (block) => {
        this.addTransactionOutput(
          tipHeight + 1 + 2,
          block.originalTemplate.coinbase
        );
        this.saveBlock(blockName, block);
        return false;
      },
      0
    );
  }
);

When(/I submit block (.*) to (.*)/, async function (blockName, nodeName) {
  await this.submitBlock(blockName, nodeName);
});

When(
  /I mine a block on (.*) based on height (\d+)/,
  async function (node, atHeight) {
    const client = this.getClient(node);
    const template = client.getPreviousBlockTemplate(atHeight);
    const candidate = await client.getMinedCandidateBlock(0, template);
    let autoTransactionResult = await this.createTransactions(
      node,
      parseInt(atHeight)
    );
    expect(autoTransactionResult).to.equal(true);
    this.addTransactionOutput(
      parseInt(atHeight) + 1,
      candidate.originalTemplate.coinbase
    );
    await client.submitBlock(
      candidate.template,
      (block) => {
        return block;
      },
      (error) => {
        // Expect an error
        console.log(error);
        return false;
      }
    );
  }
);

When(
  /I mine a block on (.*) at height (\d+) with an invalid MMR/,
  async function (node, atHeight) {
    const client = this.getClient(node);
    const template = client.getPreviousBlockTemplate(atHeight);
    const candidate = await client.getMinedCandidateBlock(0, template);

    await client
      .submitBlock(candidate.template, (block) => {
        // console.log("Candidate:", block);
        block.block.header.output_mr[0] = 1;
        // block.block.header.height = atHeight + 1;
        // block.block.header.prev_hash = candidate.header.hash;
        return block;
      })
      .catch((err) => {
        console.log("Received expected error. This is fine actually:", err);
      });
  }
);

Then(
  /the UTXO (.*) has been mined according to (.*)/,
  async function (outputName, nodeName) {
    const client = this.getClient(nodeName);
    const hash = getTransactionOutputHash(this.outputs[outputName].output);
    const lastResult = await client.fetchMatchingUtxos([hash]);

    expect(
      lastResult,
      `UTXO (${outputName}) not found with hash ${hash.toString("hex")}`
    ).to.be.an("array").that.is.not.empty;

    expect(lastResult[0].output.commitment.toString("hex")).to.equal(
      this.outputs[outputName].output.commitment.toString("hex")
    );
  }
);

Then("I receive an error containing {string}", function (_string) {
  // TODO
});

Then(/(.*) should have (\d+) peers/, async function (nodeName, peerCount) {
  await sleep(500);
  const client = this.getClient(nodeName);
  const peers = await client.getPeers();
  expect(peers.length).to.equal(peerCount);
});

Then(
  /(.*) should have at least (\d+) peers/,
  async function (nodeName, peerCount) {
    await sleep(500);
    const client = this.getClient(nodeName);
    const peers = await client.getPeers();
    expect(peers.length).to.be.greaterThanOrEqual(peerCount);
  }
);

When("I print the world", function () {
  console.log(this);
});

Then(
  /I wait for wallet (.*) to have at least (.*) uT/,
  { timeout: 120 * 1000 },
  async function (wallet, amount) {
    const walletClient = await this.getWallet(wallet).connectClient();
    console.log("\n");
    console.log(
      "Waiting for " + wallet + " balance to be at least " + amount + " uT"
    );

    await waitFor(
      async () => walletClient.isBalanceAtLeast(amount),
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
);

Then(
  /I wait for wallet (.*) to have less than (.*) uT/,
  { timeout: 120 * 1000 },
  async function (wallet, amount) {
    let walletClient = await this.getWallet(wallet).connectClient();
    console.log("\n");
    console.log(
      "Waiting for " + wallet + " balance to less than " + amount + " uT"
    );

    await waitFor(
      async () => walletClient.isBalanceLessThan(amount),
      true,
      115 * 1000,
      5 * 1000,
      5
    );
    consoleLogBalance(await walletClient.getBalance());
    if (!(await walletClient.isBalanceLessThan(amount))) {
      console.log("Balance has not dropped below specified amount!");
    }
    expect(await walletClient.isBalanceLessThan(amount)).to.equal(true);
  }
);

Then(
  /wallet (.*) and wallet (.*) have the same balance/,
  { timeout: 120 * 1000 },
  async function (walletNameA, walletNameB) {
    const walletClientA = await this.getWallet(walletNameA).connectClient();
    var balanceA = await walletClientA.getBalance();
    console.log("\n", walletNameA, "balance:");
    consoleLogBalance(balanceA);
    const walletClientB = await this.getWallet(walletNameB).connectClient();
    for (let i = 1; i <= 12; i++) {
      await waitFor(
        async () => walletClientB.isBalanceAtLeast(balanceA.available_balance),
        true,
        5 * 1000
      );
      balanceA = await walletClientA.getBalance();
      if (walletClientB.isBalanceAtLeast(balanceA.available_balance) === true) {
        break;
      }
    }
    const balanceB = await walletClientB.getBalance();
    console.log(walletNameB, "balance:");
    consoleLogBalance(balanceB);
    expect(balanceA.available_balance).to.equal(balanceB.available_balance);
  }
);

async function send_tari(
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

When(
  /I send (.*) uT from wallet (.*) to wallet (.*) at fee (.*)/,
  { timeout: 120 * 1000 },
  async function (tariAmount, source, dest, feePerGram) {
    const sourceWallet = this.getWallet(source);
    const sourceClient = await sourceWallet.connectClient();
    const sourceInfo = await sourceClient.identify();

    const destPublicKey = this.getWalletPubkey(dest);

    this.lastResult = await send_tari(
      sourceWallet,
      dest,
      destPublicKey,
      tariAmount,
      feePerGram
    );
    expect(this.lastResult.results[0].is_success).to.equal(true);
    this.addTransaction(
      sourceInfo.public_key,
      this.lastResult.results[0].transaction_id
    );
    this.addTransaction(
      destPublicKey,
      this.lastResult.results[0].transaction_id
    );
    console.log(
      "  Transaction '" +
        this.lastResult.results[0].transaction_id +
        "' is_success(" +
        this.lastResult.results[0].is_success +
        ")"
    );
    //lets now wait for this transaction to be at least broadcast before we continue.
    await waitFor(
      async () =>
        sourceClient.isTransactionAtLeastBroadcast(
          this.lastResult.results[0].transaction_id
        ),
      true,
      60 * 1000,
      5 * 1000,
      5
    );
    let transactionPending = await sourceClient.isTransactionAtLeastBroadcast(
      this.lastResult.results[0].transaction_id
    );
    expect(transactionPending).to.equal(true);
  }
);

When(
  /I broadcast HTLC transaction with (.*) uT from wallet (.*) to wallet (.*) at fee (.*)/,
  { timeout: 25 * 5 * 1000 },
  async function (tariAmount, source, dest, feePerGram) {
    const sourceClient = await this.getWallet(source).connectClient();
    const destClient = await this.getWallet(dest).connectClient();

    const sourceInfo = await sourceClient.identify();
    const destInfo = await destClient.identify();
    console.log("Starting HTLC transaction of", tariAmount, "to", dest);
    let success = false;
    let retries = 1;
    const retries_limit = 25;
    while (!success && retries <= retries_limit) {
      await waitFor(
        async () => {
          try {
            this.lastResult = await sourceClient.sendHtlc({
              recipient: {
                address: destInfo.public_key,
                amount: tariAmount,
                fee_per_gram: feePerGram,
                message: "msg",
              },
            });
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

      success = this.lastResult.is_success;
      if (!success) {
        const wait_seconds = 5;
        console.log(
          "  " +
            lastResult.failure_message +
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
    if (success) {
      this.addTransaction(
        sourceInfo.public_key,
        this.lastResult.transaction_id
      );
      this.addTransaction(destInfo.public_key, this.lastResult.transaction_id);
    }
    expect(success).to.equal(true);
    //lets now wait for this transaction to be at least broadcast before we continue.
    await waitFor(
      async () =>
        sourceClient.isTransactionAtLeastBroadcast(
          this.lastResult.transaction_id
        ),
      true,
      60 * 1000,
      5 * 1000,
      5
    );
    let transactionPending = await sourceClient.isTransactionAtLeastBroadcast(
      this.lastResult.transaction_id
    );
    expect(transactionPending).to.equal(true);
  }
);

When(
  /I claim an HTLC transaction with wallet (.*) at fee (.*)/,
  { timeout: 25 * 5 * 1000 },
  async function (source, feePerGram) {
    const sourceClient = await this.getWallet(source).connectClient();

    const sourceInfo = await sourceClient.identify();
    console.log("Claiming HTLC transaction of", source);
    let success = false;
    let retries = 1;
    const retries_limit = 25;
    while (!success && retries <= retries_limit) {
      await waitFor(
        async () => {
          try {
            this.lastResult = await sourceClient.claimHtlc({
              output: this.lastResult.output_hash,
              pre_image: this.lastResult.pre_image,
              fee_per_gram: feePerGram,
            });
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

      success = this.lastResult.results.is_success;
      if (!success) {
        const wait_seconds = 5;
        console.log(
          "  " +
            this.lastResult.results.failure_message +
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

    if (success) {
      this.addTransaction(
        sourceInfo.public_key,
        this.lastResult.results.transaction_id
      );
    }
    expect(success).to.equal(true);
    //lets now wait for this transaction to be at least broadcast before we continue.
    await waitFor(
      async () =>
        sourceClient.isTransactionAtLeastBroadcast(
          this.lastResult.results.transaction_id
        ),
      true,
      60 * 1000,
      5 * 1000,
      5
    );

    let transactionPending = await sourceClient.isTransactionAtLeastBroadcast(
      this.lastResult.results.transaction_id
    );

    expect(transactionPending).to.equal(true);
  }
);

When(
  /I claim an HTLC refund transaction with wallet (.*) at fee (.*)/,
  { timeout: 25 * 5 * 1000 },
  async function (source, feePerGram) {
    const sourceClient = await this.getWallet(source).connectClient();

    const sourceInfo = await sourceClient.identify();
    console.log("Claiming HTLC refund transaction of", source);
    let success = false;
    let retries = 1;
    let hash = this.lastResult.output_hash;
    const retries_limit = 25;
    while (!success && retries <= retries_limit) {
      await waitFor(
        async () => {
          try {
            this.lastResult = await sourceClient.claimHtlcRefund({
              output_hash: hash,
              fee_per_gram: feePerGram,
            });
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

      success = this.lastResult.results.is_success;
      if (!success) {
        const wait_seconds = 5;
        console.log(
          "  " +
            this.lastResult.results.failure_message +
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

    if (success) {
      this.addTransaction(
        sourceInfo.public_key,
        this.lastResult.results.transaction_id
      );
    }
    expect(success).to.equal(true);
    //lets now wait for this transaction to be at least broadcast before we continue.
    await waitFor(
      async () =>
        sourceClient.isTransactionAtLeastBroadcast(
          this.lastResult.results.transaction_id
        ),
      true,
      60 * 1000,
      5 * 1000,
      5
    );

    let transactionPending = await sourceClient.isTransactionAtLeastBroadcast(
      this.lastResult.results.transaction_id
    );

    expect(transactionPending).to.equal(true);
  }
);

When(
  /I send(.*) uT without waiting for broadcast from wallet (.*) to wallet (.*) at fee (.*)/,
  { timeout: 20 * 1000 },
  async function (tariAmount, source, dest, feePerGram) {
    const sourceWallet = this.getWallet(source);
    const sourceClient = await sourceWallet.connectClient();
    const sourceInfo = await sourceClient.identify();

    const destPublicKey = this.getWalletPubkey(dest);

    this.lastResult = await send_tari(
      sourceWallet,
      dest,
      destPublicKey,
      tariAmount,
      feePerGram
    );
    expect(this.lastResult.results[0].is_success).to.equal(true);
    this.addTransaction(
      sourceInfo.public_key,
      this.lastResult.results[0].transaction_id
    );
    this.addTransaction(
      destPublicKey,
      this.lastResult.results[0].transaction_id
    );
    console.log(
      "  Transaction '" +
        this.lastResult.results[0].transaction_id +
        "' is_success(" +
        this.lastResult.results[0].is_success +
        ")"
    );
  }
);

When(
  /I multi-send (.*) transactions of (.*) uT from wallet (.*) to wallet (.*) at fee (.*)/,
  { timeout: 120 * 1000 },
  async function (number, tariAmount, source, dest, fee) {
    console.log("\n");
    const sourceClient = await this.getWallet(source).connectClient();
    const sourceInfo = await sourceClient.identify();
    const destClient = await this.getWallet(dest).connectClient();
    const destInfo = await destClient.identify();
    let tx_ids = [];
    for (let i = 0; i < number; i++) {
      this.lastResult = await send_tari(
        this.getWallet(source),
        destInfo.name,
        destInfo.public_key,
        tariAmount,
        fee
      );
      expect(this.lastResult.results[0].is_success).to.equal(true);
      tx_ids.push(this.lastResult.results[0].transaction_id);
      this.addTransaction(
        sourceInfo.public_key,
        this.lastResult.results[0].transaction_id
      );
      this.addTransaction(
        destInfo.public_key,
        this.lastResult.results[0].transaction_id
      );
      // console.log("  Transaction '" + this.lastResult.results[0]["transaction_id"] + "' is_success(" +
      //    this.lastResult.results[0]["is_success"] + ")");
    }
    //lets now wait for this transaction to be at least broadcast before we continue.
    let waitfor_result = await waitFor(
      async () => {
        let result = true;
        for (let i = 0; i < number; i++) {
          result =
            result && sourceClient.isTransactionAtLeastBroadcast(tx_ids[i]);
        }
        return result;
      },
      true,
      60 * 1000,
      5 * 1000,
      5
    );
    expect(waitfor_result).to.equal(true);
  }
);

When(
  /I multi-send (.*) uT from wallet (.*) to all wallets at fee (.*)/,
  { timeout: 25 * 5 * 1000 },
  async function (tariAmount, source, fee) {
    const sourceWalletClient = await this.getWallet(source).connectClient();
    const sourceInfo = await sourceWalletClient.identify();
    let tx_ids = [];
    for (const wallet in this.wallets) {
      if (this.getWallet(source).name === this.getWallet(wallet).name) {
        continue;
      }
      const destClient = await this.getWallet(wallet).connectClient();
      const destInfo = await destClient.identify();
      this.lastResult = await send_tari(
        this.getWallet(source),
        destInfo.name,
        destInfo.public_key,
        tariAmount,
        fee
      );
      expect(this.lastResult.results[0].is_success).to.equal(true);
      tx_ids.push(this.lastResult.results[0].transaction_id);
      this.addTransaction(
        sourceInfo.public_key,
        this.lastResult.results[0].transaction_id
      );
      this.addTransaction(
        destInfo.public_key,
        this.lastResult.results[0].transaction_id
      );
      // console.log("  Transaction '" + this.lastResult.results[0]["transaction_id"] + "' is_success(" +
      //    this.lastResult.results[0]["is_success"] + ")");
    }
    let waitfor_result = await waitFor(
      async () => {
        let result = true;
        tx_ids.forEach(
          (id) =>
            (result =
              result && sourceWalletClient.isTransactionAtLeastBroadcast(id))
        );
        return result;
      },
      true,
      60 * 1000,
      5 * 1000,
      5
    );
    expect(waitfor_result).to.equal(true);
  }
);

When(
  /I transfer (.*) uT from (.*) to (.*) and (.*) at fee (.*)/,
  { timeout: 40 * 1000 },
  async function (tariAmount, source, dest1, dest2, feePerGram) {
    const sourceClient = await this.getWallet(source).connectClient();
    const destClient1 = await this.getWallet(dest1).connectClient();
    const destClient2 = await this.getWallet(dest2).connectClient();

    const sourceInfo = await sourceClient.identify();
    const dest1Info = await destClient1.identify();
    const dest2Info = await destClient2.identify();
    console.log(
      "Starting transfer of",
      tariAmount,
      "to",
      dest1,
      "and to",
      dest2
    );
    let success = false;
    let retries = 1;
    const retries_limit = 25;
    let lastResult;
    while (!success && retries <= retries_limit) {
      await waitFor(
        async () => {
          try {
            lastResult = await sourceClient.transfer({
              recipients: [
                {
                  address: dest1Info.public_key,
                  amount: tariAmount,
                  fee_per_gram: feePerGram,
                  message: "msg",
                },
                {
                  address: dest2Info.public_key,
                  amount: tariAmount,
                  fee_per_gram: feePerGram,
                  message: "msg",
                },
              ],
            });
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

      success =
        lastResult.results[0].is_success && lastResult.results[1].is_success;
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
    if (success) {
      this.addTransaction(
        sourceInfo.public_key,
        lastResult.results[0].transaction_id
      );
      this.addTransaction(
        sourceInfo.public_key,
        lastResult.results[1].transaction_id
      );
      this.addTransaction(
        dest1Info.public_key,
        lastResult.results[0].transaction_id
      );
      this.addTransaction(
        dest2Info.public_key,
        lastResult.results[1].transaction_id
      );
    }
    expect(success).to.equal(true);
  }
);

When(
  /I transfer (.*) uT to self from wallet (.*) at fee (.*)/,
  { timeout: 120 * 1000 },
  async function (tariAmount, source, feePerGram) {
    const sourceClient = await this.getWallet(source).connectClient();
    const sourceInfo = await sourceClient.identify();
    this.lastResult = await send_tari(
      this.getWallet(source),
      sourceInfo.name,
      sourceInfo.public_key,
      tariAmount,
      feePerGram
    );

    expect(this.lastResult.results[0].is_success).to.equal(true);
    this.addTransaction(
      sourceInfo.public_key,
      this.lastResult.results[0].transaction_id
    );
    console.log(
      "  Transaction '" +
        this.lastResult.results[0].transaction_id +
        "' is_success(" +
        this.lastResult.results[0].is_success +
        ")"
    );
    //lets now wait for this transaction to be at least broadcast before we continue.
    await waitFor(
      async () =>
        sourceClient.isTransactionAtLeastBroadcast(
          this.lastResult.results[0].transaction_id
        ),
      true,
      60 * 1000,
      5 * 1000,
      5
    );
    let transactionPending = await sourceClient.isTransactionAtLeastBroadcast(
      this.lastResult.results[0].transaction_id
    );
    expect(transactionPending).to.equal(true);
  }
);

When(
  /I transfer (.*) uT from (.*) to ([A-Za-z0-9,]+) at fee (.*)/,
  { timeout: 120 * 1000 },
  async function (amount, source, dests, feePerGram) {
    const wallet = this.getWallet(source);
    const client = await wallet.connectClient();
    const destWallets = await Promise.all(
      dests.split(",").map((dest) => this.getWallet(dest).connectClient())
    );

    console.log("Starting Transfer of", amount, "to");
    let output;
    await waitFor(
      async () => {
        try {
          const recipients = destWallets.map((w) => ({
            address: w.public_key,
            amount: amount,
            fee_per_gram: feePerGram,
            message: "msg",
          }));
          output = await client.transfer({ recipients });
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

    console.log("output", output);
    lastResult = output;
  }
);

When(
  /I send a one-sided transaction of (.*) uT from (.*) to (.*) at fee (.*)/,
  { timeout: 65 * 1000 },
  async function (amount, source, dest, feePerGram) {
    const sourceWallet = this.getWallet(source);
    const sourceClient = await sourceWallet.connectClient();
    const sourceInfo = await sourceClient.identify();

    const destPublicKey = this.getWalletPubkey(dest);

    const oneSided = true;
    const lastResult = await send_tari(
      sourceWallet,
      dest,
      destPublicKey,
      amount,
      feePerGram,
      oneSided
    );
    expect(lastResult.results[0].is_success).to.equal(true);

    this.addTransaction(
      sourceInfo.public_key,
      lastResult.results[0].transaction_id
    );
    //lets now wait for this transaction to be at least broadcast before we continue.
    await waitFor(
      async () =>
        sourceClient.isTransactionAtLeastBroadcast(
          lastResult.results[0].transaction_id
        ),
      true,
      60 * 1000,
      5 * 1000,
      5
    );
    let transactionPending = await sourceClient.isTransactionAtLeastBroadcast(
      lastResult.results[0].transaction_id
    );
    expect(transactionPending).to.equal(true);
  }
);

When(
  /I cancel last transaction in wallet (.*)/,
  { timeout: 20 * 1000 },
  async function (walletName) {
    const wallet = this.getWallet(walletName);
    const walletClient = await wallet.connectClient();

    let lastTxId = this.lastResult.results[0].transaction_id;
    console.log(
      "Attempting to cancel transaction ",
      lastTxId,
      "from wallet",
      walletName
    );

    let result = await walletClient.cancelTransaction(lastTxId);
    console.log(
      "Cancellation successful? ",
      result.success,
      result.failure_message
    );
    assert(result.success, true);
  }
);

When(/I wait (.*) seconds/, { timeout: 600 * 1000 }, async function (int) {
  console.log("Waiting for", int, "seconds");
  await sleep(int * 1000);
  console.log("Waiting finished");
});

Then(
  /Batch transfer of (.*) transactions was a success from (.*) to ([A-Za-z0-9,]+)/,
  async function (txCount, walletListStr) {
    const clients = await Promise.all(
      walletListStr.split(",").map((s) => {
        const wallet = this.getWallet(s);
        return wallet.connectClient();
      })
    );

    const resultObj = lastResult.results;
    console.log(resultObj);
    let successCount = 0;
    for (let i = 0; i < txCount; i++) {
      const obj = resultObj[i];
      if (!obj.is_success) {
        console.log(obj.transaction_id, "failed");
        assert(obj.is_success, true);
      } else {
        console.log(
          "Transaction",
          obj.transaction_id,
          "passed from original request succeeded"
        );
        const req = {
          transaction_ids: [obj.transaction_id.toString()],
        };
        console.log(req);
        for (const client of clients) {
          try {
            const tx = await client.getTransactionInfo(req);
            successCount++;
            console.log(tx);
          } catch (err) {
            console.log(
              obj.transaction_id.toString(),
              "not found in :",
              await client.identify()
            );
          }
        }
      }
    }

    console.log(
      `Number of successful transactions is ${successCount} of ${txCount}`
    );
    assert(successCount === txCount);
    console.log("All transactions found");
  }
);

Then(
  /wallet (.*) detects all transactions are at least Pending/,
  { timeout: 1200 * 1000 }, // Must allow for many transactions; dynamic time out used below
  async function (walletName) {
    // Note: This initial step can take a long time if network conditions are not favourable
    // Pending -> Completed -> Broadcast -> Mined Unconfirmed -> Mined Confirmed
    const wallet = this.getWallet(walletName);
    const walletClient = await wallet.connectClient();
    const walletInfo = await walletClient.identify();

    const txIds = this.transactionsMap.get(walletInfo.public_key);
    if (txIds === undefined) {
      console.log("\nNo transactions for " + walletName + "!");
      expect(false).to.equal(true);
    }
    console.log(
      "\nDetecting",
      txIds.length,
      "transactions as at least Pending: ",
      walletName,
      txIds
    );
    for (let i = 0; i < txIds.length; i++) {
      console.log(
        "(" +
          (i + 1) +
          "/" +
          txIds.length +
          ") - " +
          wallet.name +
          ": Waiting for TxId:" +
          txIds[i] +
          " to register at least Pending in the wallet ..."
      );
      await waitFor(
        async () => await walletClient.isTransactionAtLeastPending(txIds[i]),
        true,
        (60 + txIds.length * 1) * 1000,
        5 * 1000,
        5
      );
      const transactionPending = await walletClient.isTransactionAtLeastPending(
        txIds[i]
      );
      expect(transactionPending).to.equal(true);
    }
  }
);

Then(
  /all wallets detect all transactions are at least Pending/,
  { timeout: 1200 * 1000 }, // Must allow for many transactions; dynamic time out used below
  async function () {
    // Note: This initial step to register pending can take a long time if network conditions are not favourable
    // Pending -> Completed -> Broadcast -> Mined Unconfirmed -> Mined Confirmed
    for (const walletName in this.wallets) {
      const wallet = this.getWallet(walletName);
      const walletClient = await wallet.connectClient();
      const walletInfo = await walletClient.identify();

      const txIds = this.transactionsMap.get(walletInfo.public_key);
      if (txIds === undefined) {
        console.log("\nNo transactions for " + walletName + "!");
        expect(false).to.equal(true);
      }
      console.log(
        "\nDetecting",
        txIds.length,
        "transactions as at least Pending: ",
        walletName,
        txIds
      );
      for (let i = 0; i < txIds.length; i++) {
        console.log(
          "(" +
            (i + 1) +
            "/" +
            txIds.length +
            ") - " +
            wallet.name +
            ": Waiting for TxId:" +
            txIds[i] +
            " to register at least Pending in the wallet ..."
        );
        await waitFor(
          async () => walletClient.isTransactionAtLeastPending(txIds[i]),
          true,
          (60 + txIds.length * 1) * 1000,
          5 * 1000,
          5
        );
        const transactionPending =
          await walletClient.isTransactionAtLeastPending(txIds[i]);
        expect(transactionPending).to.equal(true);
      }
    }
  }
);

Then(
  /wallet (.*) detects last transaction is Pending/,
  { timeout: 120 * 1000 },
  async function (walletName) {
    const wallet = this.getWallet(walletName);
    const walletClient = await wallet.connectClient();

    let lastTxId = this.lastResult.results[0].transaction_id;
    console.log(
      "Waiting for Transaction ",
      lastTxId,
      "to be pending in wallet",
      walletName
    );

    await waitFor(
      async () => walletClient.isTransactionPending(lastTxId),
      true,
      115 * 1000,
      5 * 1000,
      5
    );
    const transactionPending = await walletClient.isTransactionPending(
      lastTxId
    );

    expect(transactionPending).to.equal(true);
  }
);

Then(
  /wallet (.*) detects all transactions are at least Completed/,
  { timeout: 1200 * 1000 }, // Must allow for many transactions; dynamic time out used below
  async function (walletName) {
    // Pending -> Completed -> Broadcast -> Mined Unconfirmed -> Mined Confirmed
    const wallet = this.getWallet(walletName);
    const walletClient = await wallet.connectClient();
    const walletInfo = await walletClient.identify();

    const txIds = this.transactionsMap.get(walletInfo.public_key);
    if (txIds === undefined) {
      console.log("\nNo transactions for " + walletName + "!");
      expect(false).to.equal(true);
    }
    console.log(
      "\nDetecting",
      txIds.length,
      "transactions as at least Completed: ",
      walletName,
      txIds
    );
    for (let i = 0; i < txIds.length; i++) {
      // Get details
      console.log(
        "(" +
          (i + 1) +
          "/" +
          txIds.length +
          ") - " +
          wallet.name +
          ": Waiting for TxId:" +
          txIds[i] +
          " to register at least Completed in the wallet ..."
      );
      await waitFor(
        async () => walletClient.isTransactionAtLeastCompleted(txIds[i]),
        true,
        (60 + txIds.length * 1) * 1000,
        5 * 1000,
        5
      );
      const transactionCompleted =
        await walletClient.isTransactionAtLeastCompleted(txIds[i]);
      expect(transactionCompleted).to.equal(true);
    }
  }
);

Then(
  /all wallets detect all transactions are at least Completed/,
  { timeout: 1200 * 1000 }, // Must allow for many transactions; dynamic time out used below
  async function () {
    // Pending -> Completed -> Broadcast -> Mined Unconfirmed -> Mined Confirmed
    for (const walletName in this.wallets) {
      const wallet = this.getWallet(walletName);
      const walletClient = await wallet.connectClient();
      const walletInfo = await walletClient.identify();

      const txIds = this.transactionsMap.get(walletInfo.public_key);
      if (txIds === undefined) {
        console.log("\nNo transactions for " + walletName + "!");
        expect(false).to.equal(true);
      }
      console.log(
        "\nDetecting",
        txIds.length,
        "transactions as at least Completed: ",
        walletName,
        txIds
      );
      for (let i = 0; i < txIds.length; i++) {
        // Get details
        console.log(
          "(" +
            (i + 1) +
            "/" +
            txIds.length +
            ") - " +
            wallet.name +
            ": Waiting for TxId:" +
            txIds[i] +
            " to register at least Completed in the wallet ..."
        );
        await waitFor(
          async () => walletClient.isTransactionAtLeastCompleted(txIds[i]),
          true,
          (60 + txIds.length * 1) * 1000,
          5 * 1000,
          5
        );
        const transactionCompleted =
          await walletClient.isTransactionAtLeastCompleted(txIds[i]);
        expect(transactionCompleted).to.equal(true);
      }
    }
  }
);

Then(
  /wallet (.*) detects all transactions are at least Broadcast/,
  { timeout: 1200 * 1000 }, // Must allow for many transactions; dynamic time out used below
  async function (walletName) {
    // Pending -> Completed -> Broadcast -> Mined Unconfirmed -> Mined Confirmed
    const wallet = this.getWallet(walletName);
    const walletClient = await wallet.connectClient();
    const walletInfo = await walletClient.identify();

    let txIds = this.transactionsMap.get(walletInfo.public_key);
    console.log(walletName, txIds);
    if (txIds === undefined) {
      console.log("\nNo transactions for " + walletName + "!");
      expect(false).to.equal(true);
    }
    console.log(
      "\nDetecting",
      txIds.length,
      "transactions as at least Broadcast: ",
      walletName,
      txIds
    );
    for (let i = 0; i < txIds.length; i++) {
      // Get details
      console.log(
        "(" +
          (i + 1) +
          "/" +
          txIds.length +
          ") - " +
          wallet.name +
          ": Waiting for TxId:" +
          txIds[i] +
          " to register at least Broadcast in the wallet ..."
      );
      await waitFor(
        async () => walletClient.isTransactionAtLeastBroadcast(txIds[i]),
        true,
        (60 + txIds.length * 1) * 1000,
        5 * 1000,
        5
      );
      const transactionBroadcasted =
        await walletClient.isTransactionAtLeastBroadcast(txIds[i]);
      expect(transactionBroadcasted).to.equal(true);
    }
  }
);

Then(
  /all wallets detect all transactions are at least Broadcast/,
  { timeout: 1200 * 1000 }, // Must allow for many transactions; dynamic time out used below
  async function () {
    // Pending -> Completed -> Broadcast -> Mined Unconfirmed -> Mined Confirmed
    for (const walletName in this.wallets) {
      const wallet = this.getWallet(walletName);
      const walletClient = await wallet.connectClient();
      const walletInfo = await walletClient.identify();

      const txIds = this.transactionsMap.get(walletInfo.public_key);
      if (txIds === undefined) {
        console.log("\nNo transactions for " + walletName + "!");
        expect(false).to.equal(true);
      }
      console.log(
        "\nDetecting",
        txIds.length,
        "transactions as at least Broadcast: ",
        walletName,
        txIds
      );
      for (let i = 0; i < txIds.length; i++) {
        // Get details
        console.log(
          "(" +
            (i + 1) +
            "/" +
            txIds.length +
            ") - " +
            wallet.name +
            ": Waiting for TxId:" +
            txIds[i] +
            " to register at least Broadcast in the wallet ..."
        );
        await waitFor(
          async () => walletClient.isTransactionAtLeastBroadcast(txIds[i]),
          true,
          (60 + txIds.length * 1) * 1000,
          5 * 1000,
          5
        );
        const transactionBroadcasted =
          await walletClient.isTransactionAtLeastBroadcast(txIds[i]);
        expect(transactionBroadcasted).to.equal(true);
      }
    }
  }
);

Then(
  /wallet (.*) detects all transactions are at least Mined_Unconfirmed/,
  { timeout: 1200 * 1000 }, // Must allow for many transactions; dynamic time out used below
  async function (walletName) {
    // Pending -> Completed -> Broadcast -> Mined Unconfirmed -> Mined Confirmed
    const wallet = this.getWallet(walletName);
    const walletClient = await wallet.connectClient();
    const walletInfo = await walletClient.identify();

    const txIds = this.transactionsMap.get(walletInfo.public_key);
    if (txIds === undefined) {
      console.log("\nNo transactions for " + walletName + "!");
      expect(false).to.equal(true);
    }
    console.log(
      "\nDetecting",
      txIds.length,
      "transactions as at least Mined_Unconfirmed: ",
      walletName,
      txIds
    );
    for (let i = 0; i < txIds.length; i++) {
      console.log(
        "(" +
          (i + 1) +
          "/" +
          txIds.length +
          ") - " +
          wallet.name +
          ": Waiting for TxId:" +
          txIds[i] +
          " to be detected as Mined_Unconfirmed in the wallet ..."
      );
      await waitFor(
        async () => walletClient.isTransactionAtLeastMinedUnconfirmed(txIds[i]),
        true,
        (60 + txIds.length * 1) * 1000,
        5 * 1000,
        5
      );
      const isTransactionAtLeastMinedUnconfirmed =
        await walletClient.isTransactionAtLeastMinedUnconfirmed(txIds[i]);
      expect(isTransactionAtLeastMinedUnconfirmed).to.equal(true);
    }
  }
);

Then(
  /all wallets detect all transactions are at least Mined_Unconfirmed/,
  { timeout: 1200 * 1000 }, // Must allow for many transactions; dynamic time out used below
  async function () {
    // Pending -> Completed -> Broadcast -> Mined Unconfirmed -> Mined Confirmed
    for (const walletName in this.wallets) {
      const wallet = this.getWallet(walletName);
      const walletClient = await wallet.connectClient();
      const walletInfo = await walletClient.identify();

      const txIds = this.transactionsMap.get(walletInfo.public_key);
      if (txIds === undefined) {
        console.log("\nNo transactions for " + walletName + "!");
        expect(false).to.equal(true);
      }
      console.log(
        "\nDetecting",
        txIds.length,
        "transactions as at least Mined_Unconfirmed: ",
        walletName,
        txIds
      );
      for (let i = 0; i < txIds.length; i++) {
        console.log(
          "(" +
            (i + 1) +
            "/" +
            txIds.length +
            ") - " +
            wallet.name +
            ": Waiting for TxId:",
          txIds[i] + " to be detected as Mined_Unconfirmed in the wallet ..."
        );
        await waitFor(
          async () =>
            walletClient.isTransactionAtLeastMinedUnconfirmed(txIds[i]),
          true,
          (60 + txIds.length * 1) * 1000,
          5 * 1000,
          5
        );
        const isTransactionAtLeastMinedUnconfirmed =
          await walletClient.isTransactionAtLeastMinedUnconfirmed(txIds[i]);
        expect(isTransactionAtLeastMinedUnconfirmed).to.equal(true);
      }
    }
  }
);

Then(
  /wallet (.*) detects all transactions as Mined_Unconfirmed/,
  { timeout: 1200 * 1000 }, // Must allow for many transactions; dynamic time out used below
  async function (walletName) {
    // Pending -> Completed -> Broadcast -> Mined Unconfirmed -> Mined Confirmed
    const wallet = this.getWallet(walletName);
    const walletClient = await wallet.connectClient();
    const walletInfo = await walletClient.identify();

    const txIds = this.transactionsMap.get(walletInfo.public_key);
    if (txIds === undefined) {
      console.log("\nNo transactions for " + walletName + "!");
      expect(false).to.equal(true);
    }
    console.log(
      "\nDetecting",
      txIds.length,
      "transactions as Mined_Unconfirmed: ",
      walletName,
      txIds
    );
    for (let i = 0; i < txIds.length; i++) {
      console.log(
        "(" +
          (i + 1) +
          "/" +
          txIds.length +
          ") - " +
          wallet.name +
          ": Waiting for TxId:" +
          txIds[i] +
          " to be detected as Mined_Unconfirmed in the wallet ..."
      );
      await waitFor(
        async () => walletClient.isTransactionMinedUnconfirmed(txIds[i]),
        true,
        (60 + txIds.length * 1) * 1000,
        5 * 1000,
        5
      );
      const isTransactionMinedUnconfirmed =
        await walletClient.isTransactionMinedUnconfirmed(txIds[i]);
      expect(isTransactionMinedUnconfirmed).to.equal(true);
    }
  }
);

Then(
  /all wallets detect all transactions as Mined_Unconfirmed/,
  { timeout: 1200 * 1000 }, // Must allow for many transactions; dynamic time out used below
  async function () {
    // Pending -> Completed -> Broadcast -> Mined Unconfirmed -> Mined Confirmed
    for (const walletName in this.wallets) {
      const wallet = this.getWallet(walletName);
      const walletClient = await wallet.connectClient();
      const walletInfo = await walletClient.identify();

      const txIds = this.transactionsMap.get(walletInfo.public_key);
      if (txIds === undefined) {
        console.log("\nNo transactions for " + walletName + "!");
        expect(false).to.equal(true);
      }
      console.log(
        "\nDetecting",
        txIds.length,
        "transactions as Mined_Unconfirmed: ",
        walletName,
        txIds
      );
      for (let i = 0; i < txIds.length; i++) {
        console.log(
          "(" +
            (i + 1) +
            "/" +
            txIds.length +
            ") - " +
            wallet.name +
            ": Waiting for TxId:" +
            txIds[i] +
            " to be detected as Mined_Unconfirmed in the wallet ..."
        );
        await waitFor(
          async () => walletClient.isTransactionMinedUnconfirmed(txIds[i]),
          true,
          (60 + txIds.length * 1) * 1000,
          5 * 1000,
          5
        );
        const isTransactionMinedUnconfirmed =
          await walletClient.isTransactionMinedUnconfirmed(txIds[i]);
        expect(isTransactionMinedUnconfirmed).to.equal(true);
      }
    }
  }
);

Then(
  /wallet (.*) detects all transactions as Mined_Confirmed/,
  { timeout: 1200 * 1000 }, // Must allow for many transactions; dynamic time out used below
  async function (walletName) {
    // Pending -> Completed -> Broadcast -> Mined Unconfirmed -> Mined Confirmed
    const wallet = this.getWallet(walletName);
    const walletClient = await wallet.connectClient();
    const walletInfo = await walletClient.identify();

    const txIds = this.transactionsMap.get(walletInfo.public_key);
    if (txIds === undefined) {
      console.log("\nNo transactions for " + walletName + "!");
      expect(false).to.equal(true);
    }
    console.log(
      "\nDetecting",
      txIds.length,
      "transactions as Mined_Confirmed: ",
      walletName,
      txIds
    );
    for (let i = 0; i < txIds.length; i++) {
      console.log(
        "(" +
          (i + 1) +
          "/" +
          txIds.length +
          ") - " +
          wallet.name +
          ": Waiting for TxId:" +
          txIds[i] +
          " to be detected as Mined_Confirmed in the wallet ..."
      );
      await waitFor(
        async () => walletClient.isTransactionMinedConfirmed(txIds[i]),
        true,
        (60 + txIds.length * 1) * 1000,
        5 * 1000,
        5
      );
      const isTransactionMinedConfirmed =
        await walletClient.isTransactionMinedConfirmed(txIds[i]);
      expect(isTransactionMinedConfirmed).to.equal(true);
    }
  }
);

Then(
  /while mining via node (.*) all transactions in wallet (.*) are found to be Mined_Confirmed/,
  { timeout: 1200 * 1000 }, // Must allow for many transactions; dynamic time out used below
  async function (nodeName, walletName) {
    const wallet = this.getWallet(walletName);
    const walletClient = await wallet.connectClient();
    const walletInfo = await walletClient.identify();
    const nodeClient = this.getClient(nodeName);
    const txIds = this.transactionsMap.get(walletInfo.public_key);
    if (txIds === undefined) {
      console.log("\nNo transactions for " + walletName + "!");
      throw new Error("No transactions for " + walletName + "!");
    }
    console.log(
      "\nDetecting",
      txIds.length,
      "transactions as Mined_Confirmed: ",
      walletName,
      txIds
    );
    for (let i = 0; i < txIds.length; i++) {
      console.log(
        "(" +
          (i + 1) +
          "/" +
          txIds.length +
          ") - " +
          wallet.name +
          ": Waiting for TxId:" +
          txIds[i] +
          " to be detected as Mined_Confirmed in the wallet ..."
      );
      await waitFor(
        async () => {
          if (await walletClient.isTransactionMinedConfirmed(txIds[i])) {
            return true;
          } else {
            const tipHeight = await this.getClient(nodeName).getTipHeight();
            let autoTransactionResult = await this.createTransactions(
              nodeName,
              tipHeight + 1
            );
            expect(autoTransactionResult).to.equal(true);
            await nodeClient.mineBlock(walletClient);
            return false;
          }
        },
        true,
        (60 + txIds.length * 1) * 1000,
        5 * 1000,
        5
      );
      const isTransactionMinedConfirmed =
        await walletClient.isTransactionMinedConfirmed(txIds[i]);
      expect(isTransactionMinedConfirmed).to.equal(true);
    }
  }
);

Then(
  /while mining via SHA3 miner (.*) all transactions in wallet (.*) are found to be Mined_Confirmed/,
  { timeout: 1200 * 1000 }, // Must allow for many transactions; dynamic time out used below
  async function (miner, walletName) {
    const wallet = this.getWallet(walletName);
    const walletClient = await wallet.connectClient();
    const walletInfo = await walletClient.identify();
    const miningNode = this.getMiningNode(miner);

    const txIds = this.transactionsMap.get(walletInfo.public_key);
    if (txIds === undefined) {
      console.log("\nNo transactions for " + walletName + "!");
      expect(false).to.equal(true);
    }
    console.log(
      "\nDetecting",
      txIds.length,
      "transactions as Mined_Confirmed: ",
      walletName,
      txIds
    );
    for (let i = 0; i < txIds.length; i++) {
      console.log(
        "(" +
          (i + 1) +
          "/" +
          txIds.length +
          ") - " +
          wallet.name +
          ": Waiting for TxId:" +
          txIds[i] +
          " to be detected as Mined_Confirmed in the wallet ..."
      );
      await waitFor(
        async () => {
          if (await walletClient.isTransactionMinedConfirmed(txIds[i])) {
            return true;
          } else {
            await miningNode.init(1, null, 1, 100000, false, null);
            await miningNode.startNew();
            return false;
          }
        },
        true,
        (60 + txIds.length * 1) * 1000,
        5 * 1000,
        5
      );
      const isTransactionMinedConfirmed =
        await walletClient.isTransactionMinedConfirmed(txIds[i]);
      expect(isTransactionMinedConfirmed).to.equal(true);
    }
  }
);

Then(
  /all wallets detect all transactions as Mined_Confirmed/,
  { timeout: 1200 * 1000 },
  async function () {
    // Pending -> Completed -> Broadcast -> Mined Unconfirmed -> Mined Confirmed
    for (const walletName in this.wallets) {
      const wallet = this.getWallet(walletName);
      const walletClient = await wallet.connectClient();
      const walletInfo = await walletClient.identify();

      const txIds = this.transactionsMap.get(walletInfo.public_key);
      if (txIds === undefined) {
        console.log("\nNo transactions for " + walletName + "!");
        expect(false).to.equal(true);
      }
      console.log(
        "\nDetecting",
        txIds.length,
        "transactions as Mined_Confirmed: ",
        walletName,
        txIds
      );
      for (let i = 0; i < txIds.length; i++) {
        console.log(
          "(" +
            (i + 1) +
            "/" +
            txIds.length +
            ") - " +
            wallet.name +
            ": Waiting for TxId:" +
            txIds[i] +
            " to be detected as Mined_Confirmed in the wallet ..."
        );
        await waitFor(
          async () => walletClient.isTransactionMinedConfirmed(txIds[i]),
          true,
          (60 + txIds.length * 1) * 1000,
          5 * 1000,
          5
        );
        const isTransactionMinedConfirmed =
          await walletClient.isTransactionMinedConfirmed(txIds[i]);
        expect(isTransactionMinedConfirmed).to.equal(true);
      }
    }
  }
);

When(
  /I list all (.*) transactions for wallet (.*)/,
  async function (transaction_type, walletName) {
    const wallet = this.getWallet(walletName);
    const walletClient = await wallet.connectClient();
    var transactions;
    var type;
    if (transaction_type === "NORMAL") {
      transactions = await walletClient.getAllNormalTransactions();
      type = "NORMAL";
    } else {
      transactions = await walletClient.getAllCoinbaseTransactions();
      type = "COINBASE";
    }
    console.log("\nListing all `" + type + "` transactions: ", walletName);
    if (transactions.length > 0) {
      for (let i = 0; i < transactions.length; i++) {
        consoleLogTransactionDetails(transactions[i]);
      }
    } else {
      console.log("  No `" + type + "` transactions found!");
    }
  }
);

Then(
  /wallet (.*) has (.*) coinbase transactions/,
  async function (walletName, count) {
    const walletClient = await this.getWallet(walletName).connectClient();
    const transactions = await walletClient.getAllCoinbaseTransactions();
    expect(transactions.length).to.equal(Number(count));
    this.resultStack.push([walletName, transactions.length]);
  }
);

Then(
  /wallet (.*) detects at least (.*) coinbase transactions as Mined_Confirmed/,
  { timeout: 120 * 1000 },
  async function (walletName, count) {
    const walletClient = await this.getWallet(walletName).connectClient();
    await waitFor(
      async () => walletClient.areCoinbasesConfirmedAtLeast(count),
      true,
      110 * 1000,
      5 * 1000,
      5
    );
    const transactions =
      await walletClient.getAllSpendableCoinbaseTransactions();
    expect(parseInt(transactions.length) >= parseInt(count)).to.equal(true);
  }
);

Then(
  /wallet (.*) detects exactly (.*) coinbase transactions as Mined_Confirmed/,
  { timeout: 120 * 1000 },
  async function (walletName, count) {
    const walletClient = await this.getWallet(walletName).connectClient();
    await waitFor(
      async () => walletClient.areCoinbasesConfirmedAtLeast(count),
      true,
      110 * 1000,
      5 * 1000,
      5
    );
    const transactions =
      await walletClient.getAllSpendableCoinbaseTransactions();
    expect(parseInt(transactions.length) === parseInt(count)).to.equal(true);
  }
);

Then(
  /wallets ([A-Za-z0-9,]+) should have (.*) (.*) spendable coinbase outputs/,
  { timeout: 610 * 1000 },
  async function (wallets, comparison, amountOfCoinBases) {
    const atLeast = "AT_LEAST";
    const exactly = "EXACTLY";
    expect(comparison === atLeast || comparison === exactly).to.equal(true);
    const walletClients = await Promise.all(
      wallets.split(",").map((wallet) => this.getWallet(wallet).connectClient())
    );
    let coinbaseCount = 0;
    for (const client of walletClients) {
      coinbaseCount += await client.countAllCoinbaseTransactions();
    }
    let spendableCoinbaseCount;
    await waitFor(
      async () => {
        spendableCoinbaseCount = 0;
        for (const client of walletClients) {
          const count = await client.countAllSpendableCoinbaseTransactions();
          console.log(client.name, "count", count);
          spendableCoinbaseCount += count;
        }
        if (comparison === atLeast) {
          console.log(
            spendableCoinbaseCount,
            spendableCoinbaseCount >= parseInt(amountOfCoinBases)
          );
          return spendableCoinbaseCount >= parseInt(amountOfCoinBases);
        } else {
          console.log(
            spendableCoinbaseCount,
            spendableCoinbaseCount === parseInt(amountOfCoinBases)
          );
          return spendableCoinbaseCount === parseInt(amountOfCoinBases);
        }
      },
      true,
      600 * 1000,
      5 * 1000,
      5
    );

    console.log(
      "Found",
      coinbaseCount,
      "coinbases in wallets",
      wallets,
      "with",
      spendableCoinbaseCount,
      "being valid and Mined_Confirmed, expected",
      comparison,
      amountOfCoinBases,
      "\n"
    );
    if (comparison === atLeast) {
      expect(spendableCoinbaseCount >= parseInt(amountOfCoinBases)).to.equal(
        true
      );
    } else {
      expect(spendableCoinbaseCount === parseInt(amountOfCoinBases)).to.equal(
        true
      );
    }
  }
);

Then(
  /wallet (.*) has at least (.*) transactions that are all (.*) and valid/,
  { timeout: 610 * 1000 },
  async function (walletName, numberOfTransactions, transactionStatus) {
    const walletClient = await this.getWallet(walletName).connectClient();
    console.log(
      walletName +
        ": waiting for " +
        numberOfTransactions +
        " transactions to be " +
        transactionStatus +
        " and valid..."
    );
    var transactions;
    var numberCorrect;
    var statusCorrect;
    await waitFor(
      async () => {
        numberCorrect = true;
        statusCorrect = true;
        transactions = await walletClient.getAllNormalTransactions();
        if (transactions.length < parseInt(numberOfTransactions)) {
          console.log(
            "Has",
            transactions.length,
            "transactions, need",
            numberOfTransactions
          );
          numberCorrect = false;
          return false;
        }
        for (let i = 0; i < transactions.length; i++) {
          if (
            transactions[i]["status"] !== transactionStatus ||
            !transactions[i]["valid"]
          ) {
            console.log(
              "Transaction " +
                i +
                1 +
                " has " +
                transactions[i]["status"] +
                " and is valid(" +
                transactions[i]["valid"] +
                ")"
            );
            statusCorrect = false;
            return false;
          }
        }
        return true;
      },
      true,
      600 * 1000,
      5 * 1000,
      5
    );

    if (transactions === undefined) {
      expect("\nNo transactions found!").to.equal("");
    }
    expect(numberCorrect && statusCorrect).to.equal(true);
  }
);

Then(
  /all (.*) transactions for wallet (.*) and wallet (.*) have consistent but opposing validity/,
  async function (transaction_type, walletNameA, walletNameB) {
    let walletClientA = await this.getWallet(walletNameA).connectClient();
    let walletClientB = await this.getWallet(walletNameB).connectClient();
    var transactionsA;
    var transactionsB;
    var type;
    if (transaction_type === "NORMAL") {
      transactionsA = await walletClientA.getAllNormalTransactions();
      transactionsB = await walletClientB.getAllNormalTransactions();
      type = "NORMAL";
    } else {
      transactionsA = await walletClientA.getAllCoinbaseTransactions();
      transactionsB = await walletClientB.getAllCoinbaseTransactions();
      type = "COINBASE";
    }
    if (transactionsA === undefined || transactionsB === undefined) {
      expect("\nNo `" + type + "` transactions found!").to.equal("");
    }
    let validA = transactionsA[0]["valid"];
    for (let i = 0; i < transactionsA.length; i++) {
      if (validA !== transactionsA[i]["valid"]) {
        expect(
          "\n" +
            walletNameA +
            "'s `" +
            type +
            "` transactions do not have a consistent validity status"
        ).to.equal("");
      }
    }
    let validB = transactionsB[0]["valid"];
    for (let i = 0; i < transactionsB.length; i++) {
      if (validB !== transactionsB[i]["valid"]) {
        expect(
          "\n" +
            walletNameB +
            "'s `" +
            type +
            "` transactions do not have a consistent validity status"
        ).to.equal("");
      }
    }
    expect(validA).to.equal(!validB);
  }
);

Then(
  /all (.*) transactions for wallet (.*) are valid/,
  async function (transaction_type, walletName) {
    let walletClient = await this.getWallet(walletName).connectClient();
    var transactions;
    var type;
    if (transaction_type === "NORMAL") {
      transactions = await walletClient.getAllNormalTransactions();
      type = "NORMAL";
    } else {
      transactions = await walletClient.getAllCoinbaseTransactions();
      type = "COINBASE";
    }
    if (transactions === undefined) {
      expect("\nNo `" + type + "` transactions found!").to.equal("");
    }
    for (let i = 0; i < transactions.length; i++) {
      expect(transactions[i]["valid"]).to.equal(true);
    }
  }
);

When(/I request the difficulties of a node (.*)/, async function (node) {
  const client = this.getClient(node);
  const difficulties = await client.getNetworkDifficulties(2, 0, 2);
  this.lastResult = difficulties;
});

Then("difficulties are available", function () {
  assert(this.lastResult.length, 3);
  // check genesis block, chain in reverse height order
  assert(this.lastResult[2].difficulty, "1");
  assert(this.lastResult[2].estimated_hash_rate, "0");
  assert(this.lastResult[2].height, "1");
  assert(this.lastResult[2].pow_algo, "0");
});

When(
  /I coin split tari in wallet (.*) to produce (.*) UTXOs of (.*) uT each with fee_per_gram (.*) uT/,
  { timeout: 4800 * 1000 },
  async function (walletName, splitNum, splitValue, feePerGram) {
    console.log("\n");
    const numberOfSplits = Math.ceil(splitNum / 499);
    let splitsLeft = splitNum;

    const wallet = this.getWallet(walletName);
    const walletClient = await wallet.connectClient();
    const walletInfo = await walletClient.identify();

    console.log(
      "Performing",
      numberOfSplits,
      "coin splits to produce",
      splitNum,
      "outputs of",
      splitValue,
      "uT"
    );

    for (let i = 0; i < numberOfSplits; i++) {
      const splits = Math.min(499, splitsLeft);
      splitsLeft -= splits;
      let result;
      await waitFor(
        async () => {
          try {
            result = await walletClient.coin_split({
              amount_per_split: splitValue,
              split_count: splits,
              fee_per_gram: feePerGram,
              message: "Cucumber coinsplit",
              lockheight: 0,
            });
          } catch (error) {
            console.log(error);
            return false;
          }
          return true;
        },
        true,
        4700 * 1000,
        5 * 1000,
        5
      );
      let waitfor_result = await waitFor(
        async () => {
          return walletClient.isTransactionAtLeastBroadcast(result.tx_id);
        },
        true,
        60 * 1000,
        5 * 1000,
        5
      );
      expect(waitfor_result).to.equal(true);
      console.log(
        "Coin split",
        i + 1,
        "/",
        numberOfSplits,
        " completed with TxId: ",
        result
      );
      this.addTransaction(walletInfo.public_key, result.tx_id);
      this.lastResult = result;
    }
  }
);

When(
  /I send (.*) transactions of (.*) uT each from wallet (.*) to wallet (.*) at fee_per_gram (.*)/,
  { timeout: 120 * 1000 },
  async function (
    numTransactions,
    amount,
    sourceWallet,
    destWallet,
    feePerGram
  ) {
    console.log("\n");
    const sourceWalletClient = await this.getWallet(
      sourceWallet
    ).connectClient();
    const sourceInfo = await sourceWalletClient.identify();
    const destWalletClient = await this.getWallet(destWallet).connectClient();
    const destInfo = await destWalletClient.identify();

    console.log(
      "Sending",
      numTransactions,
      "transactions from",
      sourceWallet,
      "to",
      destWallet
    );

    let batch = 1;
    let tx_ids = [];
    for (let i = 0; i < numTransactions; i++) {
      const result = await send_tari(
        this.getWallet(sourceWallet),
        destInfo.name,
        destInfo.public_key,
        amount,
        feePerGram,
        false,
        "Transaction from " + sourceWallet + " to " + destWallet + " " + i,
        false
      );
      expect(result.results[0].is_success).to.equal(true);
      tx_ids.push(result.results[0].transaction_id);
      this.addTransaction(
        sourceInfo.public_key,
        result.results[0].transaction_id
      );
      this.addTransaction(
        destInfo.public_key,
        result.results[0].transaction_id
      );

      if (i / 10 >= batch) {
        batch++;
        console.log(i, "/", numTransactions, " transactions sent");
      }
      await sleep(50);
    }
    let waitfor_result = await waitFor(
      async () => {
        let result = true;
        tx_ids.forEach(
          (id) =>
            (result =
              result && sourceWalletClient.isTransactionAtLeastBroadcast(id))
        );
        return result;
      },
      true,
      60 * 1000,
      5 * 1000,
      5
    );
    expect(waitfor_result).to.equal(true);
    console.log(numTransactions, " transactions successfully sent.");
  }
);

Given(
  /I change the password of wallet (.*) to (.*) via command line/,
  async function (name, newPassword) {
    let wallet = this.getWallet(name);
    await wallet.changePassword("kensentme", newPassword);
  }
);

Then(
  /the password of wallet (.*) is (not)? ?(.*)/,
  async function (name, is_not, password) {
    let wallet = this.getWallet(name);
    try {
      await wallet.start(password);
    } catch (error) {
      expect(error).to.equal(
        is_not === "not" ? "Incorrect password" : undefined
      );
    }
  }
);

When(
  /I wait for (.*) to connect to (.*)/,
  async function (firstNode, secondNode) {
    const firstNodeClient = await this.getNodeOrWalletClient(firstNode);
    const secondNodeClient = await this.getNodeOrWalletClient(secondNode);
    const secondNodeIdentity = await secondNodeClient.identify();

    await waitForPredicate(async () => {
      let peers = await firstNodeClient.listConnectedPeers();
      return peers.some((p) => secondNodeIdentity.public_key === p.public_key);
    }, 50 * 1000);
  }
);

Then(/(.*) is connected to (.*)/, async function (firstNode, secondNode) {
  const firstNodeClient = await this.getNodeOrWalletClient(firstNode);
  const secondNodeClient = await this.getNodeOrWalletClient(secondNode);
  const secondNodeIdentity = await secondNodeClient.identify();
  let peers = await firstNodeClient.listConnectedPeers();
  assert(peers.some((p) => secondNodeIdentity.public_key === p.public_key));
});

When(
  /I wait for (.*) to have (.*) connectivity/,
  async function (nodeName, expectedStatus) {
    const node = await this.getNodeOrWalletClient(nodeName);
    const expected = ConnectivityStatus[expectedStatus.toUpperCase()];
    assert(
      expected !== undefined,
      `Invalid connectivity state ${expectedStatus}`
    );
    await waitForPredicate(async () => {
      let info = await node.getNetworkStatus();
      return info.status === expected;
    }, 50 * 1000);
  }
);

When(
  /I wait for (.*) to have (\d+) node connections/,
  async function (nodeName, numConnections) {
    const node = await this.getNodeOrWalletClient(nodeName);
    numConnections = +numConnections;
    await waitForPredicate(async () => {
      let info = await node.getNetworkStatus();
      if (info.num_node_connections > numConnections) {
        console.warn(
          `Node ${nodeName} has more connections than expected. Expected = ${numConnections} Got = ${info.num_node_connections}`
        );
      }
      return info.num_node_connections === numConnections;
    }, 50 * 1000);
  }
);

Given(
  "I change base node of {word} to {word} via command line",
  async function (wallet_name, base_node_name) {
    let wallet = this.getWallet(wallet_name);
    let base_node = this.getNode(base_node_name);
    let output = await wallet.runCommand(
      `set-base-node ${base_node.peerAddress().replace("::", " ")}`
    );
    let parse = output.buffer.match(/Setting base node peer\.\.\./);
    expect(parse, "Parsing the output buffer failed").to.not.be.null;
  }
);

async function wallet_run_command(
  wallet,
  command,
  timeOutSeconds = 15,
  message = "",
  printMessage = true
) {
  if (message === "") {
    message = "Wallet CLI command:\n    '" + command + "'";
  }
  if (printMessage) {
    console.log(message);
  }
  let output;
  await waitFor(
    async () => {
      try {
        output = await wallet.runCommand(command);
      } catch (error) {
        console.log(error);
        return false;
      }
      return true;
    },
    true,
    timeOutSeconds * 1000,
    5 * 1000,
    5
  );
  return output;
}

Then(
  "I get balance of wallet {word} is at least {int} uT via command line",
  { timeout: 180 * 1000 },
  async function (name, amount) {
    let wallet = this.getWallet(name);
    let output = await wallet_run_command(wallet, "get-balance", 180);
    let parse = output.buffer.match(/Available balance: (\d*.\d*) T/);
    expect(parse, "Parsing the output buffer failed").to.not.be.null;
    expect(parseFloat(parse[1])).to.be.greaterThanOrEqual(amount / 1000000);
  }
);

When(
  "I send {int} uT from {word} to {word} via command line",
  { timeout: 180 * 1000 },
  async function (amount, sender, receiver) {
    let wallet = this.getWallet(sender);
    let dest_pubkey = this.getWalletPubkey(receiver);
    await wallet_run_command(
      wallet,
      `send-tari ${amount} ${dest_pubkey} test message`,
      180
    );
    // await wallet.sendTari(dest_pubkey, amount, "test message");
  }
);

When(
  "I send one-sided {int} uT from {word} to {word} via command line",
  { timeout: 180 * 1000 },
  async function (amount, sender, receiver) {
    let wallet = this.getWallet(sender);
    let dest_pubkey = this.getWalletPubkey(receiver);
    await wallet_run_command(
      wallet,
      `send-one-sided ${amount} ${dest_pubkey} test message`,
      180
    );
    // await wallet.sendOneSided(dest_pubkey, amount, "test message");
  }
);

Then(
  "I make it rain from wallet {word} {int} tx per sec {int} sec {int} uT {int} increment to {word} via command line",
  { timeout: 300 * 1000 },
  async function (sender, freq, duration, amount, amount_inc, receiver) {
    let wallet = this.getWallet(sender);
    let dest_pubkey = this.getWalletPubkey(receiver);
    await wallet_run_command(
      wallet,
      `make-it-rain ${freq} ${duration} ${amount} ${amount_inc} now ${dest_pubkey} negotiated test message`,
      300
    );
  }
);

Then(
  "I get count of utxos of wallet {word} and it's at least {int} via command line",
  async function (name, amount) {
    let wallet = this.getWallet(name);
    let output = await wallet_run_command(wallet, `count-utxos`);
    let parse = output.buffer.match(/Total number of UTXOs: (\d+)/);
    expect(parse, "Parsing the output buffer failed").to.not.be.null;
    expect(parseInt(parse[1])).to.be.greaterThanOrEqual(amount);
  }
);

When(
  "I do coin split on wallet {word} to {int} uT {int} coins via command line",
  { timeout: 180 * 1000 },
  async function (name, amount_per_coin, number_of_coins) {
    let wallet = this.getWallet(name);
    await wallet_run_command(
      wallet,
      `coin-split ${amount_per_coin} ${number_of_coins}`,
      180
    );
  }
);

When(
  "I discover peer {word} on wallet {word} via command line",
  { timeout: 120 * 1000 }, // Ample time should be allowed for peer discovery
  async function (node, name) {
    let wallet = this.getWallet(name);
    let peer = this.getNode(node).peerAddress().split("::")[0];
    let output = await wallet_run_command(wallet, `discover-peer ${peer}`, 120);
    let parse = output.buffer.match(/Discovery succeeded/);
    expect(parse, "Parsing the output buffer failed").to.not.be.null;
  }
);

When(
  "I run whois {word} on wallet {word} via command line",
  { timeout: 20 * 1000 },
  async function (who, name) {
    await sleep(5000);
    let wallet = this.getWallet(name);
    let pubkey = this.getNode(who).peerAddress().split("::")[0];
    let output = await wallet_run_command(wallet, `whois ${pubkey}`, 20);
    let parse = output.buffer.match(/Public Key: (.+)\n/);
    expect(parse, "Parsing the output buffer failed").to.not.be.null;
    expect(parse[1]).to.be.equal(pubkey);
  }
);

When(
  "I set custom base node of {word} to {word} via command line",
  async function (wallet_name, base_node_name) {
    let wallet = this.getWallet(wallet_name);
    let base_node = this.getNode(base_node_name);
    let output = await wallet_run_command(
      wallet,
      `set-custom-base-node ${base_node.peerAddress().replace("::", " ")}`
    );
    let parse = output.buffer.match(
      /Custom base node peer saved in wallet database\./
    );
    expect(parse, "Parsing the output buffer failed").to.not.be.null;
  }
);

When(
  "I clear custom base node of wallet {word} via command line",
  async function (name) {
    let wallet = this.getWallet(name);
    let output = await wallet_run_command(wallet, "clear-custom-base-node");
    let parse = output.buffer.match(
      /Custom base node peer cleared from wallet database./
    );
    expect(parse, "Parsing the output buffer failed").to.not.be.null;
  }
);

When(
  "I export the utxos of wallet {word} via command line",
  async function (name) {
    let wallet = this.getWallet(name);
    let output = await wallet_run_command(wallet, "export-utxos");
    let parse_cnt = output.buffer.match(/Total number of UTXOs: (\d+)/);
    expect(parse_cnt, "Parsing the output buffer failed").to.not.be.null;
    let utxo_cnt = parseInt(parse_cnt[1]);
    for (let i = 1; i <= utxo_cnt; ++i) {
      let regex = new RegExp(`${i}. Value: \\d*.\\d* T`);
      expect(output.buffer.match(regex), "Parsing the output buffer failed").to
        .not.be.null;
    }
  }
);

Then(
  /I wait until base node (.*) has (.*) unconfirmed transactions in its mempool/,
  { timeout: 120 * 1000 },
  async function (baseNode, numTransactions) {
    const client = this.getClient(baseNode);
    await waitFor(
      async () => {
        let stats = await client.getMempoolStats();
        return stats.unconfirmed_txs;
      },
      numTransactions,
      115 * 1000
    );

    let stats = await client.getMempoolStats();
    console.log(
      "Base node",
      baseNode,
      "has ",
      stats.unconfirmed_txs,
      " unconfirmed transaction in its mempool"
    );
    expect(stats.unconfirmed_txs).to.equal(numTransactions);
  }
);

Then(
  /node (.*) lists heights (\d+) to (\d+)/,
  async function (node, first, last) {
    const client = this.getClient(node);
    const start = first;
    const end = last;
    let heights = [];

    for (let i = start; i <= end; i++) {
      heights.push(i);
    }
    const blocks = await client.getBlocks(heights);
    const results = blocks.map((result) =>
      parseInt(result.block.header.height)
    );
    let i = 0; // for ordering check
    for (let height = start; height <= end; height++) {
      expect(results[i]).equal(height);
      i++;
    }
  }
);

Then(
  "I wait for recovery of wallet {word} to finish",
  { timeout: 600 * 1000 },
  async function (wallet_name) {
    const wallet = this.getWallet(wallet_name);
    while (wallet.recoveryInProgress) {
      await sleep(1000);
    }
    expect(wallet.recoveryProgress[1]).to.be.greaterThan(0);
    expect(wallet.recoveryProgress[0]).to.be.equal(wallet.recoveryProgress[1]);
  }
);

When(
  "I have {int} base nodes with pruning horizon {int} force syncing on node {word}",
  { timeout: 20 * 1000 },
  async function (nodes_count, horizon, force_sync_to) {
    const promises = [];
    const force_sync_address = this.getNode(force_sync_to).peerAddress();
    for (let i = 0; i < nodes_count; i++) {
      const base_node = this.createNode(`BaseNode${i}`, {
        pruningHorizon: horizon,
      });
      base_node.setPeerSeeds([force_sync_address]);
      base_node.setForceSyncPeers([force_sync_address]);
      promises.push(
        base_node.startNew().then(() => this.addNode(`BaseNode${i}`, base_node))
      );
    }
    await Promise.all(promises);
  }
);

When(
  "I have a stratum transcoder {word} connected to {word} and {word}",
  { timeout: 20 * 1000 },
  async function (transcoder, node, wallet) {
    const baseNode = this.getNode(node);
    const walletNode = this.getWallet(wallet);
    const stratum_transcoder = new StratumTranscoderProcess(
      transcoder,
      baseNode.getGrpcAddress(),
      walletNode.getGrpcAddress(),
      this.logFilePathProxy
    );
    await stratum_transcoder.startNew();
    this.addProxy(transcoder, stratum_transcoder);
  }
);

When(
  "I call getinfo from stratum transcoder {word}",
  { timeout: 20 * 1000 },
  async function (transcoder) {
    const proxy = this.getProxy(transcoder);
    const transcoderClient = proxy.getClient();
    await transcoderClient.getInfo();
  }
);

Then(
  "I get a valid getinfo response from stratum transcoder {word}",
  { timeout: 20 * 1000 },
  async function (transcoder) {
    const proxy = this.getProxy(transcoder);
    const transcoderClient = proxy.getClient();
    let resp = await transcoderClient.getLastResponse();
    console.log(resp);
    expect(resp["result"]).to.be.an("object");
    expect(resp["result"]["best_block"]).to.be.not.undefined;
    expect(resp["result"]["blockchain_version"]).to.be.not.undefined;
    expect(resp["result"]["height_of_longest_chain"]).to.be.not.undefined;
    expect(resp["result"]["initial_sync_achieved"]).to.be.not.undefined;
    expect(resp["result"]["local_height"]).to.be.not.undefined;
    expect(resp["result"]["lock_height"]).to.be.not.undefined;
    expect(resp["result"]["max_block_interval"]).to.be.not.undefined;
    expect(resp["result"]["max_weight"]).to.be.not.undefined;
    expect(resp["result"]["min_diff"]).to.be.not.undefined;
    expect(resp["result"]["tip_height"]).to.be.not.undefined;
  }
);

function check_stratum_header_response(resp) {
  expect(resp["result"]).to.be.an("object");
  expect(resp["result"]["blockheader"]).to.be.an("object");
  expect(resp["result"]["blockheader"]["depth"]).to.be.not.undefined;
  expect(resp["result"]["blockheader"]["difficulty"]).to.be.not.undefined;
  expect(resp["result"]["blockheader"]["block_size"]).to.be.not.undefined;
  expect(resp["result"]["blockheader"]["hash"]).to.be.not.undefined;
  expect(resp["result"]["blockheader"]["height"]).to.be.not.undefined;
  expect(resp["result"]["blockheader"]["major_version"]).to.be.not.undefined;
  expect(resp["result"]["blockheader"]["minor_version"]).to.be.not.undefined;
  expect(resp["result"]["blockheader"]["nonce"]).to.be.not.undefined;
  expect(resp["result"]["blockheader"]["num_txes"]).to.be.not.undefined;
  expect(resp["result"]["blockheader"]["orphan_status"]).to.be.not.undefined;
  expect(resp["result"]["blockheader"]["prev_hash"]).to.be.not.undefined;
  expect(resp["result"]["blockheader"]["reward"]).to.be.not.undefined;
  expect(resp["result"]["blockheader"]["timestamp"]).to.be.not.undefined;
  expect(resp["status"]).to.be.not.undefined;
}

When(
  "I call getblocktemplate from stratum transcoder {word}",
  { timeout: 20 * 1000 },
  async function (transcoder) {
    const proxy = this.getProxy(transcoder);
    const transcoderClient = proxy.getClient();
    await transcoderClient.getBlockTemplate();
  }
);

Then(
  "I get a valid getblocktemplate response from stratum transcoder {word}",
  { timeout: 20 * 1000 },
  async function (transcoder) {
    const proxy = this.getProxy(transcoder);
    const transcoderClient = proxy.getClient();
    let resp = await transcoderClient.getLastResponse();
    console.log(await transcoderClient.getLastResponse());
    expect(resp["result"]).to.be.an("object");
    expect(resp["result"]["blockheader_blob"]).to.be.not.undefined;
    expect(resp["result"]["blocktemplate_blob"]).to.be.not.undefined;
    expect(resp["result"]["difficulty"]).to.be.not.undefined;
    expect(resp["result"]["expected_reward"]).to.be.not.undefined;
    expect(resp["result"]["height"]).to.be.not.undefined;
    expect(resp["result"]["prev_hash"]).to.be.not.undefined;
  }
);

When(
  "I call getblockheaderbyhash from stratum transcoder {word}",
  { timeout: 20 * 1000 },
  async function (transcoder) {
    const proxy = this.getProxy(transcoder);
    const transcoderClient = proxy.getClient();
    //use last retrieved header data for this
    let resp = await transcoderClient.getLastResponse();
    await transcoderClient.getBlockHeaderByHash(
      resp["result"]["blockheader"]["hash"]
    );
  }
);

Then(
  "I get a valid getblockheaderbyhash response from stratum transcoder {word}",
  { timeout: 20 * 1000 },
  async function (transcoder) {
    const proxy = this.getProxy(transcoder);
    const transcoderClient = proxy.getClient();
    let resp = await transcoderClient.getLastResponse();
    console.log(resp);
    check_stratum_header_response(resp);
  }
);

When(
  "I call getblockheaderbyheight from stratum transcoder {word}",
  { timeout: 20 * 1000 },
  async function (transcoder) {
    const proxy = this.getProxy(transcoder);
    const transcoderClient = proxy.getClient();
    //use last retrieved header data for this
    let resp = await transcoderClient.getLastResponse();
    await transcoderClient.getBlockHeaderByHeight(
      resp["result"]["blockheader"]["height"]
    );
  }
);

Then(
  "I get a valid getblockheaderbyheight response from stratum transcoder {word}",
  { timeout: 20 * 1000 },
  async function (transcoder) {
    const proxy = this.getProxy(transcoder);
    const transcoderClient = proxy.getClient();
    let resp = await transcoderClient.getLastResponse();
    console.log(resp);
    check_stratum_header_response(resp);
  }
);

When(
  "I call getlastblockheader from stratum transcoder {word}",
  { timeout: 20 * 1000 },
  async function (transcoder) {
    const proxy = this.getProxy(transcoder);
    const transcoderClient = proxy.getClient();
    await transcoderClient.getLastBlockHeader();
  }
);

Then(
  "I get a valid getlastblockheader response from stratum transcoder {word}",
  { timeout: 20 * 1000 },
  async function (transcoder) {
    const proxy = this.getProxy(transcoder);
    const transcoderClient = proxy.getClient();
    let resp = await transcoderClient.getLastResponse();
    console.log(resp);
    check_stratum_header_response(resp);
  }
);

When(
  "I call getbalance from stratum transcoder {word}",
  { timeout: 20 * 1000 },
  async function (transcoder) {
    const proxy = this.getProxy(transcoder);
    const transcoderClient = proxy.getClient();
    await transcoderClient.getBalance();
  }
);

Then(
  "I get a valid getbalance response from stratum transcoder {word}",
  { timeout: 20 * 1000 },
  async function (transcoder) {
    const proxy = this.getProxy(transcoder);
    const transcoderClient = proxy.getClient();
    let resp = await transcoderClient.getLastResponse();
    console.log(await transcoderClient.getLastResponse());
    expect(resp["result"]).to.be.an("object");
    expect(resp["result"]["available_balance"]).to.be.not.undefined;
    expect(resp["result"]["pending_incoming_balance"]).to.be.not.undefined;
    expect(resp["result"]["pending_outgoing_balance"]).to.be.not.undefined;
  }
);

When(
  "I call transfer from stratum transcoder {word} using the public key of {word}, {word} and amount {int} uT each",
  { timeout: 50 * 1000 },
  async function (transcoder, wallet1, wallet2, amountEach) {
    let walletPK1 = this.getWalletPubkey(wallet1);
    let walletPK2 = this.getWalletPubkey(wallet2);
    let destinations = [
      { address: walletPK1, amount: amountEach },
      { address: walletPK2, amount: amountEach },
    ];
    const proxy = this.getProxy(transcoder);
    const transcoderClient = proxy.getClient();
    await transcoderClient.transferFunds(destinations);
  }
);

Then(
  "I get a valid transfer response from stratum transcoder {word}",
  { timeout: 2 * 60 * 1000 },
  async function (transcoder) {
    const proxy = this.getProxy(transcoder);
    const transcoderClient = proxy.getClient();
    let resp = await transcoderClient.getLastResponse();
    expect(resp["result"]).to.be.an("object");
    expect(resp["result"]["transaction_results"]).to.be.an("array");
    let results = resp["result"]["transaction_results"];
    for (let i = 0; i < results.length; i++) {
      console.log("Transaction result (" + i + "):");
      console.log(results[i]);
      expect(results[i]["transaction_id"]).to.be.not.undefined;
      expect(results[i]["address"]).to.be.not.undefined;
      expect(results[i]["is_success"]).to.be.not.undefined;
      expect(results[i]["failure_message"]).to.be.not.undefined;
      expect(results[i]["is_success"]).to.be.equal(true);
    }
  }
);

When(
  "I call submitblock from stratum transcoder {word}",
  { timeout: 20 * 1000 },
  async function (transcoder) {
    const proxy = this.getProxy(transcoder);
    const transcoderClient = proxy.getClient();
    //use last retrieved blocktemplate_blob for this, this can be done as target difficulty = 1 for tests
    let resp = await transcoderClient.getLastResponse();
    await transcoderClient.submitBlock(resp["result"]["blocktemplate_blob"]);
  }
);

Then(
  "I get a valid submitblock response from stratum transcoder {word}",
  { timeout: 20 * 1000 },
  async function (transcoder) {
    const proxy = this.getProxy(transcoder);
    const transcoderClient = proxy.getClient();
    let resp = await transcoderClient.getLastResponse();
    console.log(resp);
    expect(resp["result"]).to.be.an("object");
    expect(resp["result"]["status"]).to.be.not.undefined;
    expect(resp["result"]["untrusted"]).to.be.not.undefined;
    expect(resp["result"]["status"]).to.be.equal("OK");
    expect(resp["result"]["untrusted"]).to.be.equal(false);
  }
);
