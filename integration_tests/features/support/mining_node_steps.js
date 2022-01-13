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

const { Given, When } = require("@cucumber/cucumber");
const MiningNodeProcess = require("../../helpers/miningNodeProcess");
const { withTimeout } = require("../../helpers/util");

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
  /I have mining node (.*) connected to (?:base|seed) node (.*) and wallet (.*)/,
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
  /mining node (.*) mines (\d+) blocks?$/,
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
