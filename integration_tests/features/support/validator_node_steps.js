//  Copyright 2021. The Tari Project
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

const { When, Given, Then } = require("@cucumber/cucumber");
const { expect } = require("chai");
const { sleep } = require("../../helpers/util");

When(
  "I register an NFT asset with committee of {int}",
  async function (committeeSize) {
    // Create committeeSize validator nodes
    for (let i = 0; i < committeeSize; i++) {
      await this.createAndAddDanNode(`danNode${i}`);
    }

    let committee = Object.values(this.dan_nodes).map(async (node) => {
      let pk = node.getPubKey();
      await node.stop();
      return pk;
    });

    console.log(committee);
    console.log(this);

    let wallet = this.currentWallet();
    let client = await wallet.connectClient();
    console.log(await client.getBalance());
    await client.registerAsset("Asset 1");

    return "pending";
  }
);

When("I create {int} NFT(s)", function () {
  return "pending";
});

Given(
  "I have committee from {int} validator nodes connected",
  { timeout: 20 * 1000 },
  async function (nodes_cnt) {
    console.log(`Starting ${nodes_cnt} validator nodes`);
    const promises = [];
    for (let i = 0; i < nodes_cnt; i++) {
      promises.push(this.createAndAddDanNode(`DanNode${i}`));
    }
    await Promise.all(promises);
    let committee = Array(nodes_cnt)
      .fill()
      .map((_, i) => this.getNode(`DanNode${i}`).getPubKey());
    let peers = Array(nodes_cnt)
      .fill()
      .map((_, i) => this.getNode(`DanNode${i}`).peerAddress());
    for (let i = 0; i < nodes_cnt; ++i) {
      let dan_node = this.getNode(`DanNode${i}`);
      dan_node.setCommittee(committee);
      dan_node.setPeerSeeds(peers.filter((_, j) => i != j));
      promises.push(dan_node.start());
    }
    await Promise.all(promises);
  }
);

Then(
  /I send instruction successfully with metadata (.*)/,
  { timeout: 20 * 1000 },
  async function (metadata) {
    console.log("metadata", metadata);
    let dan_node = this.getNode("DanNode0"); // Only the first node has GRPC
    let grpc_dan_node = await dan_node.createGrpcClient();
    let response = await grpc_dan_node.executeInstruction(
      "f665775dbbf4e428e5c8c2bb1c5e7d2e508e93c83250c495ac617a0a1fb2d76d", // asset
      "update", // method
      metadata,
      "eee280877ef836f1026d8a848a5da3eb6364cd0343372235e6ca10e2a697fc6f" // token
    );
    expect(response.status).to.be.equal("Accepted");
  }
);

Then(
  "At least {int} out of {int} validator nodes have filled asset data",
  { timeout: 1200 * 1000 },
  async function (at_least, total) {
    let retries = 1;
    let success = false;
    let retries_limit = 239;
    while (retries < retries_limit) {
      let count = 0;
      for (let i = 0; i < total; ++i) {
        let node = this.getNode(`DanNode${i}`);
        if (node.hasAssetData()) {
          count += 1;
        }
      }
      success = count >= at_least;
      if (success) break;
      ++retries;
      await sleep(5000);
    }
    expect(success).to.be.true;
  }
);
