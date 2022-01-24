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

const { When, Then } = require("@cucumber/cucumber");
const StratumTranscoderProcess = require("../../helpers/stratumTranscoderProcess");
const { expect } = require("chai");
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
