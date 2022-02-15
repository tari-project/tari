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

const { Given, Then, When } = require("@cucumber/cucumber");
const { expect } = require("chai");
const { waitFor, sleep, byteArrayToHex } = require("../../helpers/util");
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
  "I register asset {word} on wallet {word} via command line",
  { timeout: 20 * 1000 },
  async function (asset_name, wallet_name) {
    let wallet = this.getWallet(wallet_name);
    let output = await wallet_run_command(
      wallet,
      `register-asset ${asset_name}`
    );
    console.log("output buffer:", output.buffer);
    expect(output.buffer).to.have.string("Registering asset");
    expect(output.buffer).to.have.string("with public key:");
    // hack out the public key
    let split = output.buffer.split("with public key: ");
    split = split[1].split("\n");
    this.asset_public_key = split[0];
    expect(this.asset_public_key.length).to.equal(64);
  }
);

Then(
  "I create committee checkpoint for asset on wallet {word} via command line",
  { timeout: 20 * 1000 },
  async function (wallet_name) {
    // scenario needs "I register asset..." first to populate asset public key
    expect(this.asset_public_key).to.exist;
    const member =
      "3ef702f33925dc65143f7bebcbe0c53902e8772a8fe7f5ddb703587c0203267d";
    let wallet = this.getWallet(wallet_name);
    let output = await wallet_run_command(
      wallet,
      `create-committee-definition ${this.asset_public_key} ${member}`
    );
    // console.log(output.buffer);
    expect(output.buffer).to.have.string(" committee members");
    let regex = /with \d+ committee members/;
    let match = output.buffer.match(regex);
    expect(match[0]).to.equal("with 1 committee members");
  }
);

Then(
  "I mint tokens {string} for asset {word} on wallet {word} via command line",
  { timeout: 20 * 1000 },
  async function (token_names, asset_name, wallet_name) {
    let wallet = this.getWallet(wallet_name);
    const walletClient = await wallet.connectClient();
    const assets = await walletClient.getOwnedAssets();
    const asset = assets.find((asset) => asset.name === asset_name);
    let output = await wallet_run_command(
      wallet,
      `mint-tokens ${byteArrayToHex(asset.public_key)} ${token_names}`
    );
    // console.log(output.buffer);
    expect(output.buffer).to.have.string("Minting tokens for asset");
  }
);
