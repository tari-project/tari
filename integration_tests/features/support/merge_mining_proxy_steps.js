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
const MergeMiningProxyProcess = require("../../helpers/mergeMiningProxyProcess");
const assert = require("assert");
const expect = require("chai").expect;

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

When(/I ask for a block height from proxy (.*)/, async function (mmProxy) {
  this.lastResult = "NaN";
  const proxy = this.getProxy(mmProxy);
  const proxyClient = proxy.createClient();
  const height = await proxyClient.getHeight();
  this.lastResult = height;
});

Then("Proxy response height is valid", function () {
  expect(Number.isInteger(this.lastResult)).to.be.true;
});

When(/I ask for a block template from proxy (.*)/, async function (mmProxy) {
  this.lastResult = {};
  const proxy = this.getProxy(mmProxy);
  const proxyClient = proxy.createClient();
  const template = await proxyClient.getBlockTemplate();
  this.lastResult = template;
});

Then("Proxy response block template is valid", function () {
  expect(this.lastResult).to.be.an('object');
  expect(this.lastResult).to.not.be.null;
  expect(this.lastResult._aux).to.not.be.undefined;
  expect(this.lastResult.status).to.equal("OK");
});

When(/I submit a block through proxy (.*)/, async function (mmProxy) {
  const blockTemplateBlob = this.lastResult.blocktemplate_blob;
  const proxy = this.getProxy(mmProxy);
  const proxyClient = proxy.createClient();
  const result = await proxyClient.submitBlock(blockTemplateBlob);
  this.lastResult = result;
});

Then(
  "Proxy response block submission is valid with submitting to origin",
  function () {
    expect(this.lastResult.result).to.be.an('object');
    expect(this.lastResult.result).to.not.be.null;
    expect(this.lastResult.result._aux).to.not.be.undefined;
    expect(this.lastResult.result.status).to.equal("OK");
  }
);

Then(
  "Proxy response block submission is valid without submitting to origin",
  function () {
    expect(this.lastResult.result).to.not.be.null;
    expect(this.lastResult.status).to.equal("OK");
  }
);

When(
  /I ask for the last block header from proxy (.*)/,
  async function (mmProxy) {
    const proxy = this.getProxy(mmProxy);
    const proxyClient = proxy.createClient();
    const result = await proxyClient.getLastBlockHeader();
    this.lastResult = result;
  }
);

Then("Proxy response for last block header is valid", function () {
  expect(this.lastResult).to.be.an('object');
  expect(this.lastResult).to.not.be.null;
  expect(this.lastResult.result._aux).to.not.be.undefined;
  expect(this.lastResult.result.status).to.equal("OK");
  
  this.lastResult = this.lastResult.result.block_header.hash;
});

When(
  /I ask for a block header by hash using last block header from proxy (.*)/,
  async function (mmProxy) {
    const proxy = this.getProxy(mmProxy);
    const proxyClient = proxy.createClient();
    const result = await proxyClient.getBlockHeaderByHash(this.lastResult);
    this.lastResult = result;
  }
);

Then("Proxy response for block header by hash is valid", function () {
  expect(this.lastResult).to.be.an('object');
  expect(this.lastResult).to.not.be.null;
  expect(this.lastResult.result.status).to.equal("OK");
});
