//  Copyright 2022. The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

const assert = require("assert");
const { When, Then } = require("@cucumber/cucumber");
const expect = require("chai").expect;

const {
  waitFor,
  waitForPredicate,
  getTransactionOutputHash,
  sleep,
  withTimeout,
} = require("../../helpers/util");
const { ConnectivityStatus } = require("../../helpers/types");
const TransactionBuilder = require("../../helpers/transactionBuilder");

Then(/all transactions must have succeeded/, function () {
  expect(this.lastTransactionsSucceeded).to.be(true);
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
    this.lastResult = result.tx_ids;
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
    this.lastResult = result.tx_ids;
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
    this.lastResult = result.tx_ids;
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
    for (
      let imported_tx = 0;
      imported_tx < this.lastResult.length;
      imported_tx++
    ) {
      for (let found_tx = 0; found_tx < found_txs.length; found_tx++) {
        if (found_txs[found_tx].tx_id === this.lastResult[imported_tx]) {
          found_count++;
          expect(found_txs[found_tx].status).to.equal(
            "TRANSACTION_STATUS_IMPORTED"
          );
          expect(found_txs[found_tx].valid).to.equal(false);
        }
      }
    }
    expect(found_count).to.equal(this.lastResult.length);
  }
);

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
  /I create a custom transaction (.*) spending (.*) to (.*) with fee (\d+) and unique id '([^']+)'/i,
  function (txnName, inputs, output, fee, unique_id) {
    const txInputs = inputs.split(",").map((input) => this.outputs[input]);
    const txn = new TransactionBuilder();
    txn.changeFee(fee);
    txInputs.forEach((txIn) => txn.addInput(txIn));
    const txOutput = txn.addOutput(txn.getSpendableAmount(), {
      unique_id,
    });
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

When("I mine {int} block(s)", { timeout: -1 }, async function (numBlocks) {
  let name = this.currentBaseNodeName();
  // const tipHeight = await this.getClient(name).getTipHeight();
  for (let i = 0; i < numBlocks; i++) {
    await withTimeout(60 * 1000, this.mineBlock(name, 0));
  }
});

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

When("I wait {int} seconds", { timeout: 600 * 1000 }, async function (seconds) {
  console.log("Waiting for", seconds, "seconds");
  await sleep(seconds * 1000);
  console.log("Waiting finished");
});

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

When(/I request the difficulties of a node (.*)/, async function (node) {
  const client = this.getClient(node);
  const difficulties = await client.getNetworkDifficulties(2, 0, 2);
  this.lastResult = difficulties;
});

Then("difficulties are available", function () {
  assert.strictEqual(this.lastResult.length, 3);
  // check genesis block, chain in reverse height order
  expect(this.lastResult[2].difficulty).to.equal("1");
  expect(this.lastResult[2].sha3_estimated_hash_rate).to.equal("0");
  expect(this.lastResult[2].monero_estimated_hash_rate).to.equal("0");
  expect(this.lastResult[2].height).to.equal("2");
  expect(this.lastResult[2].pow_algo).to.equal("0");
});

When(
  "I wait for {word} to connect to {word}",
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
