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
const ValidatorNodeProcess = require("../../helpers/validatorNodeProcess");

Given(
  "I have a validator node {word} connected to base node {word} and wallet {word} with {word} set to {word}",
  { timeout: 20 * 1000 },
  async function (
    vn_name,
    base_node_name,
    wallet_name,
    option_key,
    option_value
  ) {
    const baseNode = this.getNode(base_node_name);
    const walletNode = this.getWallet(wallet_name);

    const baseNodeGrpcAddress = `127.0.0.1:${baseNode.getGrpcPort()}`;
    const walletGrpcAddress = `127.0.0.1:${walletNode.getGrpcPort()}`;

    const options = {};
    options[option_key] = option_value;

    const danNode = new ValidatorNodeProcess(
      vn_name,
      false,
      options,
      this.logFilePathBaseNode,
      undefined,
      baseNodeGrpcAddress,
      walletGrpcAddress
    );
    await danNode.startNew();
    await this.addDanNode(vn_name, danNode);
  }
);

Then(
  "I publish a contract acceptance transaction for the validator node {word}",
  { timeout: 20 * 1000 },
  async function (vn_name) {
    let dan_node = this.getNode(vn_name);
    let grpc_dan_node = await dan_node.createGrpcClient();
    let response = await grpc_dan_node.publishContractAcceptance(
      "90b1da4524ea0e9479040d906db9194d8af90f28d05ff2d64c0a82eb93125177" // contract_id
    );
    expect(response.status).to.be.equal("Accepted");
    console.log({ response });
  }
);

Then(
  "I publish a contract update proposal acceptance transaction for the validator node {word}",
  { timeout: 20 * 1000 },
  async function (vn_name) {
    let dan_node = this.getNode(vn_name);
    let grpc_dan_node = await dan_node.createGrpcClient();
    let response = await grpc_dan_node.publishContractUpdateProposalAcceptance(
      "90b1da4524ea0e9479040d906db9194d8af90f28d05ff2d64c0a82eb93125177", // contract_id
      0 // proposal_id
    );
    expect(response.status).to.be.equal("Accepted");
    console.log({ response });
  }
);
