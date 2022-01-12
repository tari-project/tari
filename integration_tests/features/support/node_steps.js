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

const { Given, When, Then } = require("@cucumber/cucumber");
const {
  waitForIterate,
  waitFor,
  waitForPredicate,
  withTimeout,
} = require("../../helpers/util");
const {
  AUTOUPDATE_HASHES_TXT_URL,
  AUTOUPDATE_HASHES_TXT_SIG_URL,
  AUTOUPDATE_HASHES_TXT_BAD_SIG_URL,
} = require("../../helpers/constants");
const expect = require("chai").expect;

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

Given("I have {int} base nodes", { timeout: 20 * 1000 }, async function (n) {
  for (let i = 0; i < n; i++) {
    await this.createAndAddNode(`basenode${i}`);
  }
});

Given("I have 1 base node", { timeout: 20 * 1000 }, async function () {
  await this.createAndAddNode(`basenode`);
});

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

When(/I start base node (.*)/, { timeout: 20 * 1000 }, async function (name) {
  await this.startNode(name);
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
  /I run blockchain recovery on node (\S*)/,
  { timeout: 120 * 1000 },
  async function (name) {
    await this.startNode(name, ["--rebuild-db"]);
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

When(/I stop node (.*)/, async function (name) {
  await this.stopNode(name);
});
