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

const { Given, Then } = require("@cucumber/cucumber");
const { expect } = require("chai");
const { findUtxoWithOutputMessage } = require("../../helpers/util");

Given(
  "I have a validator node {word} connected to base node {word} and wallet {word}",
  { timeout: 20 * 1000 },
  async function (vn_name, base_node_name, wallet_name) {
    let vn = await this.createValidatorNode(
      vn_name,
      base_node_name,
      wallet_name
    );
    await this.addDanNode(vn_name, vn);
  }
);

Then(
  "validator node {word} has {string} set to {word}",
  { timeout: 20 * 1000 },
  async function (vn_name, option_name, option_value) {
    let vn = this.getNode(vn_name);
    await vn.stop();

    vn.options["validator_node." + option_name] = option_value;

    await vn.startNew();
  }
);

Then(
  "I publish a contract acceptance transaction for contract {word} for the validator node {word}",
  { timeout: 20 * 1000 },
  async function (contract_name, vn_name) {
    let dan_node = this.getNode(vn_name);
    let grpc_dan_node = await dan_node.createGrpcClient();
    let response = await grpc_dan_node.publishContractAcceptance(
      await this.fetchContract(contract_name)
    );
    expect(response.status).to.be.equal("Accepted");
    console.debug({ response });
  }
);

Then(
  "I publish a contract update proposal acceptance transaction for the validator node {word}",
  { timeout: 120 * 1000 },
  async function (vn_name) {
    let dan_node = this.getNode(vn_name);
    let grpc_dan_node = await dan_node.createGrpcClient();
    let response = await grpc_dan_node.publishContractUpdateProposalAcceptance(
      "a58fb2adefcc40242f20f2d896e14451549dd60839fee78a7bd40ba2cc0a0e91", // contract_id
      0 // proposal_id
    );
    expect(response.status).to.be.equal("Accepted");
    console.debug({ response });
  }
);

Then(
  "wallet {word} will have a successfully mined contract acceptance transaction for contract {word}",
  { timeout: 40 * 1000 },
  async function (wallet_name, contract_name) {
    let wallet = await this.getWallet(wallet_name);
    let contract_id = await this.fetchContract(contract_name);
    let message = `Contract acceptance for contract with id=${contract_id}`;

    let utxos = await findUtxoWithOutputMessage(wallet, message);
    expect(utxos.length).to.equal(1);
  }
);

Then(
  "wallet {word} will have a successfully mined contract update proposal for contract {word}",
  { timeout: 40 * 1000 },
  async function (wallet_name, contract_name) {
    let wallet = await this.getWallet(wallet_name);
    let contract_id = await this.fetchContract(contract_name);
    let message = `Contract update proposal acceptance for contract_id=${contract_id} and proposal_id=0`;

    let utxos = await findUtxoWithOutputMessage(wallet, message);
    expect(utxos.length).to.equal(1);
  }
);
