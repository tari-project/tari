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
const { Given, When, Then } = require("@cucumber/cucumber");
const WalletProcess = require("../../helpers/walletProcess");
const {
  waitFor,
  consoleLogBalance,
  sleep,
  consoleLogTransactionDetails,
} = require("../../helpers/util");
const {
  AUTOUPDATE_HASHES_TXT_URL,
  AUTOUPDATE_HASHES_TXT_SIG_URL,
  AUTOUPDATE_HASHES_TXT_BAD_SIG_URL,
  BLOCK_REWARD,
  CONFIRMATION_PERIOD,
} = require("../../helpers/constants");
const expect = require("chai").expect;

Given("I have {int} wallet(s)", { timeout: -1 }, async function (numWallets) {
  for (let i = 0; i < numWallets; i++) {
    await this.createAndAddWallet(`wallet${i}`, "", {});
  }
});

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
  /I have (a )?wallet (.*) connected to seed node (.*)/,
  { timeout: 20 * 1000 },
  async function (a, walletName, seedName) {
    await this.createAndAddWallet(
      walletName,
      this.seeds[seedName].peerAddress()
    );
  }
);

Given(
  "I have wallet {word} with {int}T connected to base node {word}",
  { timeout: 120 * 1000 },
  async function (walletName, balance, nodeName) {
    await this.createAndAddWallet(
      walletName,
      this.nodes[nodeName].peerAddress()
    );
    let numberOfBlocks = Math.ceil(balance / BLOCK_REWARD);
    console.log("Creating miner");
    let tempMiner = await this.createMiningNode(
      "tempMiner",
      nodeName,
      walletName
    );
    console.log(numberOfBlocks);
    console.log("starting miner");
    await tempMiner.mineBlocksUntilHeightIncreasedBy(numberOfBlocks, 1, false);
    // mine some blocks to confirm
    await this.mineBlocks(nodeName, CONFIRMATION_PERIOD);
    let walletClient = await this.getWallet(walletName).connectClient();
    await waitFor(
      async () => walletClient.isBalanceAtLeast(balance * 1000),
      true,
      120 * 1000,
      5 * 1000,
      5
    );
  }
);

Given(
  "I have wallet {word} connected to base node {word}",
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
      let name = "Wallet_" + String(i).padStart(2, "0");
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
    let walletA = this.getWallet(walletNameA);
    const seedWords = walletA.getSeedWords();
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
  /I recover all wallets connected to all seed nodes/,
  { timeout: 120 * 1000 },
  async function () {
    for (let walletName in this.wallets) {
      let wallet = this.getWallet(walletName);
      const seedWords = wallet.getSeedWords();
      let recoveredWalletName = "recovered_" + wallet.name;
      console.log(
        "Recover " +
          wallet.name +
          " into " +
          recoveredWalletName +
          ", seed words:\n  " +
          seedWords
      );
      const walletB = new WalletProcess(
        recoveredWalletName,
        false,
        {},
        this.logFilePathWallet,
        seedWords
      );

      walletB.setPeerSeeds([this.seedAddresses()]);
      await walletB.startNew();
      this.addWallet(recoveredWalletName, walletB);
      let walletClient = await this.getWallet(
        recoveredWalletName
      ).connectClient();
      let walletInfo = await walletClient.identify();
      this.addWalletPubkey(recoveredWalletName, walletInfo.public_key);
    }
  }
);

Given(
  /I recover wallet (.*) into (\d+) wallets connected to all seed nodes/,
  { timeout: 30 * 1000 },
  async function (walletNameA, numwallets) {
    let walletA = this.getWallet(walletNameA);
    const seedWords = walletA.getSeedWords();
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
  /I wait for recovered wallets to have at least (\d+) uT/,
  { timeout: 60 * 1000 },
  async function (amount) {
    for (let walletName in this.wallets) {
      if (walletName.split("_")[0] == "recovered") {
        const walletClient = await this.getWallet(walletName).connectClient();
        console.log("\n");
        console.log(
          "Waiting for wallet " +
            walletName +
            " balance to be at least " +
            amount +
            " uT"
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

When(/I stop wallet ([^\s]+)/, async function (walletName) {
  let wallet = this.getWallet(walletName);
  await wallet.stop();
});

When(/I stop all wallets/, async function () {
  for (let walletName in this.wallets) {
    let wallet = this.getWallet(walletName);
    await wallet.stop();
  }
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
          expect(found_txs[found_tx].valid).to.equal(true);
        }
      }
    }
    expect(found_count).to.equal(this.lastResult.length);
  }
);

Then(
  "wallet {word} has {int}T",
  { timeout: 120 * 1000 },
  async function (wallet, amount) {
    await this.waitForWalletToHaveBalance(wallet, amount * 1000);
  }
);

Then(
  /I wait for wallet (.*) to have at least (.*) uT/,
  { timeout: 120 * 1000 },
  async function (wallet, amount) {
    await this.waitForWalletToHaveBalance(wallet, amount);
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

When(
  /I send (.*) uT from wallet (.*) to wallet (.*) at fee (.*)/,
  { timeout: 120 * 1000 },
  async function (tariAmount, source, dest, feePerGram) {
    await this.transfer(tariAmount, source, dest, feePerGram);
  }
);

When(
  "I transfer {int}T from {word} to {word}",
  { timeout: 120 * 1000 },
  async function (tariAmount, source, dest) {
    await this.transfer(tariAmount * 1000, source, dest, 10);
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
            this.lastResult.failure_message +
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
  { timeout: 120 * 1000 },
  async function (tariAmount, source, dest, feePerGram) {
    const sourceWallet = this.getWallet(source);
    const sourceClient = await sourceWallet.connectClient();
    const sourceInfo = await sourceClient.identify();

    const destPublicKey = this.getWalletPubkey(dest);

    this.lastResult = await this.send_tari(
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
      this.lastResult = await this.send_tari(
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
      this.lastResult = await this.send_tari(
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
    this.lastResult = await this.send_tari(
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
    this.lastResult = output;
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
    const lastResult = await this.send_tari(
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

Then(
  /Batch transfer of (.*) transactions was a success from (.*) to ([A-Za-z0-9,]+)/,
  async function (txCount, walletListStr) {
    const clients = await Promise.all(
      walletListStr.split(",").map((s) => {
        const wallet = this.getWallet(s);
        return wallet.connectClient();
      })
    );

    const resultObj = this.lastResult.results;
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
  /wallet (.*) has at least (.*) transactions that are all (.*) and not cancelled/,
  { timeout: 610 * 1000 },
  async function (walletName, numberOfTransactions, transactionStatus) {
    const walletClient = await this.getWallet(walletName).connectClient();
    console.log(
      walletName +
        ": waiting for " +
        numberOfTransactions +
        " transactions to be " +
        transactionStatus +
        " and not cancelled..."
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
            transactions[i]["is_cancelled"]
          ) {
            console.log(
              "Transaction " +
                i +
                1 +
                " has " +
                transactions[i]["status"] +
                " (need " +
                transactionStatus +
                ") and is not cancelled(" +
                transactions[i]["is_cancelled"] +
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
  /all (.*) transactions for wallet (.*) and wallet (.*) have consistent but opposing cancellation status/,
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
    let cancelledA = transactionsA[0]["is_cancelled"];
    for (let i = 0; i < transactionsA.length; i++) {
      if (cancelledA !== transactionsA[i]["is_cancelled"]) {
        expect(
          "\n" +
            walletNameA +
            "'s `" +
            type +
            "` transactions do not have a consistent cancellation status"
        ).to.equal("");
      }
    }
    let cancelledB = transactionsB[0]["is_cancelled"];
    for (let i = 0; i < transactionsB.length; i++) {
      if (cancelledB !== transactionsB[i]["is_cancelled"]) {
        expect(
          "\n" +
            walletNameB +
            "'s `" +
            type +
            "` transactions do not have a consistent cancellation status"
        ).to.equal("");
      }
    }
    expect(cancelledA).to.equal(!cancelledB);
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
      expect(transactions[i]["is_cancelled"]).to.equal(false);
    }
  }
);

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
      const result = await this.send_tari(
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

Then(
  "I register asset {word} on wallet {word}",
  { timeout: 20 * 1000 },
  async function (asset_name, wallet_name) {
    const wallet = this.getWallet(wallet_name);
    const walletClient = await wallet.connectClient();
    const public_key = await walletClient.registerAsset(asset_name);
    console.log(`Asset ${asset_name} registered with public key ${public_key}`);
  }
);

Then(
  "I have asset {word} on wallet {word} with status {word}",
  { timeout: 20 * 1000 },
  async function (asset_name, wallet_name, status) {
    const wallet = this.getWallet(wallet_name);
    const walletClient = await wallet.connectClient();
    const assets = await walletClient.getOwnedAssets();
    expect(
      assets.some(
        (asset) =>
          asset.name === asset_name &&
          asset.registration_output_status.toUpperCase() === status
      )
    ).to.be.true;
  }
);

Then(
  "I mint tokens {string} for asset {word} on wallet {word}",
  { timeout: 20 * 1000 },
  async function (token_names, asset_name, wallet_name) {
    const wallet = this.getWallet(wallet_name);
    const walletClient = await wallet.connectClient();
    const assets = await walletClient.getOwnedAssets();
    const asset = assets.find((asset) => asset.name === asset_name);
    const tokens = token_names.split(" ");
    await walletClient.mintTokens(asset.public_key, tokens);
  }
);

Then(
  "I have token {word} for asset {word} on wallet {word} in state {word}",
  { timeout: 20 * 1000 },
  async function (token_name, asset_name, wallet_name, status) {
    const wallet = this.getWallet(wallet_name);
    const walletClient = await wallet.connectClient();
    const assets = await walletClient.getOwnedAssets();
    const asset = assets.find((asset) => asset.name === asset_name);
    let tokens = await walletClient.getOwnedTokens(asset.public_key);
    expect(
      tokens.some(
        (token) =>
          String.fromCharCode(...token.unique_id) === token_name &&
          token.output_status.toUpperCase() === status
      )
    ).to.be.true;
  }
);
