const { Given, When, Then } = require("cucumber");
const expect = require("chai").expect;

const { sleep, waitForIterate } = require("../../helpers/util");

When(
  "I have ffi wallet {word} connected to base node {word}",
  { timeout: 20 * 1000 },
  async function (name, node) {
    let wallet = await this.createAndAddFFIWallet(name);
    let peer = this.nodes[node].peerAddress().split("::");
    wallet.addBaseNodePeer(peer[0], peer[1]);
  }
);

Then(
  "I want to get emoji id of ffi wallet {word}",
  { timeout: 20 * 1000 },
  async function (name) {
    let wallet = this.getWallet(name);
    let emoji_id = wallet.identifyEmoji();
    console.log(emoji_id);
    expect(emoji_id.length).to.be.equal(
      22 * 3, // 22 emojis, 3 bytes per one emoji
      `Emoji id has wrong length : ${emoji_id}`
    );
  }
);

When(
  "I send {int} uT from ffi wallet {word} to wallet {word} at fee {int}",
  { timeout: 20 * 1000 },
  function (amount, sender, receiver, fee) {
    let ffi_wallet = this.getWallet(sender);
    let result = ffi_wallet.sendTransaction(
      this.getWalletPubkey(receiver),
      amount,
      fee,
      `Send from ffi ${sender} to ${receiver} at fee ${fee}`
    );
    console.log(result);
  }
);

When(
  "I set passphrase {word} of ffi wallet {word}",
  { timeout: 20 * 1000 },
  function (passphrase, name) {
    let wallet = this.getWallet(name);
    wallet.applyEncryption(passphrase);
  }
);

Then(
  "I have {int} received and {int} send transaction in ffi wallet {word}",
  { timeout: 120 * 1000 },
  async function (received, send, name) {
    let wallet = this.getWallet(name);
    let completed = wallet.getCompletedTxs();
    let inbound = 0;
    let outbound = 0;
    let length = completed.getLength();
    let inboundTxs = wallet.getInboundTxs();
    inbound += inboundTxs.getLength();
    inboundTxs.destroy();
    let outboundTxs = wallet.getOutboundTxs();
    outbound += outboundTxs.getLength();
    outboundTxs.destroy();
    for (let i = 0; i < length; i++) {
      {
        let tx = completed.getAt(i);
        if (tx.isOutbound()) {
          outbound++;
        } else {
          inbound++;
        }
        tx.destroy();
      }
    }
    completed.destroy();

    expect(outbound, "Outbound transaction count mismatch").to.be.equal(send);
    expect(inbound, "Inbound transaction count mismatch").to.be.equal(received);
  }
);

Then(
  "ffi wallet {word} has {int} broadcast transaction",
  { timeout: 120 * 1000 },
  async function (name, count) {
    let wallet = this.getWallet(name);
    let broadcast = await wallet.getBroadcastTransactionsCount();
    let retries = 1;
    const retries_limit = 24;
    while (broadcast != count && retries <= retries_limit) {
      await sleep(5000);
      broadcast = await wallet.getBroadcastTransactionsCount();
      ++retries;
    }
    expect(broadcast, "Number of broadcasted messages mismatch").to.be.equal(
      count
    );
  }
);

When(
  "I add contact with alias {word} and pubkey {word} to ffi wallet {word}",
  { timeout: 20 * 1000 },
  function (alias, wallet_name, ffi_wallet_name) {
    let ffi_wallet = this.getWallet(ffi_wallet_name);
    ffi_wallet.addContact(alias, this.getWalletPubkey(wallet_name));
  }
);

Then(
  "I have contact with alias {word} and pubkey {word} in ffi wallet {word}",
  { timeout: 20 * 1000 },
  function (alias, wallet_name, ffi_wallet_name) {
    let wallet = this.getWalletPubkey(wallet_name);
    let ffi_wallet = this.getWallet(ffi_wallet_name);
    let contacts = ffi_wallet.getContactList();
    let length = contacts.getLength();
    let found = false;
    for (let i = 0; i < length; i++) {
      {
        let contact = contacts.getAt(i);
        let hex = contact.getPubkeyHex();
        if (wallet === hex) {
          found = true;
        }
        contact.destroy();
      }
    }
    contacts.destroy();
    expect(found).to.be.equal(true);
  }
);

When(
  "I remove contact with alias {word} from ffi wallet {word}",
  { timeout: 20 * 1000 },
  function (alias, wallet_name) {
    let ffi_wallet = this.getWallet(wallet_name);
    let contacts = ffi_wallet.getContactList();
    let length = contacts.getLength();
    for (let i = 0; i < length; i++) {
      {
        let contact = contacts.getAt(i);
        let calias = contact.getAlias();
        if (alias === calias) {
          ffi_wallet.removeContact(contact);
        }
        contact.destroy();
      }
    }
    contacts.destroy();
  }
);

Then(
  "I don't have contact with alias {word} in ffi wallet {word}",
  { timeout: 20 * 1000 },
  function (alias, wallet_name) {
    let ffi_wallet = this.getWallet(wallet_name);
    let contacts = ffi_wallet.getContactList();
    let length = contacts.getLength();
    let found = false;
    for (let i = 0; i < length; i++) {
      {
        let contact = contacts.getAt(i);
        let calias = contact.getAlias();
        if (alias === calias) {
          found = true;
        }
        contact.destroy();
      }
    }
    contacts.destroy();
    expect(found).to.be.equal(false);
  }
);

When(
  "I set base node {word} for ffi wallet {word}",
  function (node, wallet_name) {
    let wallet = this.getWallet(wallet_name);
    let peer = this.nodes[node].peerAddress().split("::");
    wallet.addBaseNodePeer(peer[0], peer[1]);
  }
);

Then(
  "I wait for ffi wallet {word} to have {int} pending outbound transaction(s)",
  { timeout: 180 * 1000 },
  async function (wallet_name, count) {
    let wallet = this.getWallet(wallet_name);
    let broadcast = wallet.getOutboundTransactions();
    let length = broadcast.getLength();
    broadcast.destroy();
    let retries = 1;
    const retries_limit = 24;
    while (length != count && retries <= retries_limit) {
      await sleep(5000);
      broadcast = wallet.getOutboundTransactions();
      length = broadcast.getLength();
      broadcast.destroy();
      ++retries;
    }
    expect(length, "Number of pending messages mismatch").to.be.equal(count);
  }
);

Then(
  "I cancel all outbound transactions on ffi wallet {word} and it will cancel {int} transaction",
  async function (wallet_name, count) {
    const wallet = this.getWallet(wallet_name);
    let txs = wallet.getOutboundTransactions();
    let cancelled = 0;
    for (let i = 0; i < txs.getLength(); i++) {
      let tx = txs.getAt(i);
      let cancellation = wallet.cancelPendingTransaction(tx.getTransactionID());
      tx.destroy();
      if (cancellation) {
        cancelled++;
      }
    }
    txs.destroy();
    expect(cancelled).to.be.equal(count);
  }
);

Given(
  "I have a ffi wallet {word} connected to base node {word}",
  { timeout: 20 * 1000 },
  async function (walletName, nodeName) {
    let ffi_wallet = await this.createAndAddFFIWallet(walletName, null);
    let peer = this.nodes[nodeName].peerAddress().split("::");
    ffi_wallet.addBaseNodePeer(peer[0], peer[1]);
  }
);

Then(
  "I recover wallet {word} into ffi wallet {word} from seed words on node {word}",
  { timeout: 20 * 1000 },
  async function (wallet_name, ffi_wallet_name, node) {
    let wallet = this.getWallet(wallet_name);
    const seed_words_text = wallet.getSeedWords();
    await wallet.stop();
    await sleep(1000);
    let ffi_wallet = await this.createAndAddFFIWallet(
      ffi_wallet_name,
      seed_words_text
    );
    let peer = this.nodes[node].peerAddress().split("::");
    ffi_wallet.addBaseNodePeer(peer[0], peer[1]);
    ffi_wallet.startRecovery(peer[0]);
  }
);

Then(
  "Check callbacks for finished inbound tx on ffi wallet {word}",
  async function (wallet_name) {
    const wallet = this.getWallet(wallet_name);
    expect(wallet.receivedTransaction).to.be.greaterThanOrEqual(1);
    expect(wallet.transactionBroadcast).to.be.greaterThanOrEqual(1);
    wallet.clearCallbackCounters();
  }
);

Then(
  "Check callbacks for finished outbound tx on ffi wallet {word}",
  async function (wallet_name) {
    const wallet = this.getWallet(wallet_name);
    expect(wallet.receivedTransactionReply).to.be.greaterThanOrEqual(1);
    expect(wallet.transactionBroadcast).to.be.greaterThanOrEqual(1);
    wallet.clearCallbackCounters();
  }
);

Then(
  "I wait for ffi wallet {word} to receive {int} transaction",
  { timeout: 710 * 1000 },
  async function (wallet_name, amount) {
    let wallet = this.getWallet(wallet_name);

    console.log("\n");
    console.log(
      "Waiting for " + wallet_name + " to receive " + amount + " transaction(s)"
    );

    await waitForIterate(
      () => {
        return wallet.getCounters().received >= amount;
      },
      true,
      1000,
      700
    );

    if (!(wallet.getCounters().received >= amount)) {
      console.log("Counter not adequate!");
    } else {
      console.log(wallet.getCounters());
    }
    expect(wallet.getCounters().received >= amount).to.equal(true);
  }
);

Then(
  "I wait for ffi wallet {word} to receive {int} finalization",
  { timeout: 710 * 1000 },
  async function (wallet_name, amount) {
    let wallet = this.getWallet(wallet_name);

    console.log("\n");
    console.log(
      "Waiting for " +
        wallet_name +
        " to receive " +
        amount +
        " transaction finalization(s)"
    );

    await waitForIterate(
      () => {
        return wallet.getCounters().finalized >= amount;
      },
      true,
      1000,
      700
    );

    if (!(wallet.getCounters().finalized >= amount)) {
      console.log("Counter not adequate!");
    } else {
      console.log(wallet.getCounters());
    }
    expect(wallet.getCounters().finalized >= amount).to.equal(true);
  }
);

Then(
  "I wait for ffi wallet {word} to receive {int} broadcast",
  { timeout: 710 * 1000 },
  async function (wallet_name, amount) {
    let wallet = this.getWallet(wallet_name);

    console.log("\n");
    console.log(
      "Waiting for " +
        wallet_name +
        " to receive " +
        amount +
        " transaction broadcast(s)"
    );

    await waitForIterate(
      () => {
        return wallet.getCounters().broadcast >= amount;
      },
      true,
      1000,
      700
    );

    if (!(wallet.getCounters().broadcast >= amount)) {
      console.log("Counter not adequate!");
    } else {
      console.log(wallet.getCounters());
    }
    expect(wallet.getCounters().broadcast >= amount).to.equal(true);
  }
);

Then(
  "I wait for ffi wallet {word} to receive {int} mined",
  { timeout: 710 * 1000 },
  async function (wallet_name, amount) {
    let wallet = this.getWallet(wallet_name);

    console.log("\n");
    console.log(
      "Waiting for " +
        wallet_name +
        " to receive " +
        amount +
        " transaction mined"
    );

    await waitForIterate(
      () => {
        return wallet.getCounters().mined >= amount;
      },
      true,
      1000,
      700
    );

    if (!(wallet.getCounters().mined >= amount)) {
      console.log("Counter not adequate!");
    } else {
      console.log(wallet.getCounters());
    }
    expect(wallet.getCounters().mined >= amount).to.equal(true);
  }
);

Then(
  "I wait for ffi wallet {word} to receive at least {int} SAF message",
  { timeout: 710 * 1000 },
  async function (wallet_name, amount) {
    let wallet = this.getWallet(wallet_name);

    console.log("\n");
    console.log(
      "Waiting for " +
        wallet_name +
        " to receive at least " +
        amount +
        " SAF messages(s)"
    );

    await waitForIterate(
      () => {
        return wallet.getCounters().saf >= amount;
      },
      true,
      1000,
      700
    );

    if (!(wallet.getCounters().saf >= amount)) {
      console.log("Counter not adequate!");
    } else {
      console.log(wallet.getCounters());
    }
    expect(wallet.getCounters().saf >= amount).to.equal(true);
  }
);

Then(
  "I wait for ffi wallet {word} to have at least {int} uT",
  { timeout: 710 * 1000 },
  async function (wallet_name, amount) {
    let wallet = this.getWallet(wallet_name);

    console.log("\n");
    console.log(
      "Waiting for " + wallet_name + " balance to be at least " + amount + " uT"
    );

    await waitForIterate(
      () => {
        return wallet.getBalance().available >= amount;
      },
      true,
      1000,
      700
    );

    let balance = wallet.getBalance().available;

    if (!(balance >= amount)) {
      console.log("Balance not adequate!");
    }

    expect(balance >= amount).to.equal(true);
  }
);

Then(
  "I wait for recovery of ffi wallet {word} to finish",
  { timeout: 600 * 1000 },
  function (wallet_name) {
    const wallet = this.getWallet(wallet_name);
    while (!wallet.recoveryFinished) {
      sleep(1000).then();
    }
  }
);

When("I start ffi wallet {word}", async function (walletName) {
  let wallet = this.getWallet(walletName);
  await wallet.startNew(null, null);
});

When(
  "I restart ffi wallet {word} connected to base node {word}",
  async function (walletName, node) {
    let wallet = this.getWallet(walletName);
    await wallet.restart();
    let peer = this.nodes[node].peerAddress().split("::");
    wallet.addBaseNodePeer(peer[0], peer[1]);
  }
);

Then(
  "I want to get public key of ffi wallet {word}",
  { timeout: 20 * 1000 },
  function (name) {
    let wallet = this.getWallet(name);
    let public_key = wallet.identify();
    expect(public_key.length).to.be.equal(
      64,
      `Public key has wrong length : ${public_key}`
    );
  }
);

Then(
  "I want to view the transaction kernels for completed transactions in ffi wallet {word}",
  { timeout: 20 * 1000 },
  function (name) {
    let ffi_wallet = this.getWallet(name);
    let transactions = ffi_wallet.getCompletedTxs();
    let length = transactions.getLength();
    expect(length > 0).to.equal(true);
    for (let i = 0; i < length; i++) {
      let tx = transactions.getAt(i);
      let kernel = tx.getKernel();
      let data = kernel.asObject();
      console.log("Transaction kernel info:");
      console.log(data);
      expect(data.excess.length > 0).to.equal(true);
      expect(data.nonce.length > 0).to.equal(true);
      expect(data.sig.length > 0).to.equal(true);
      kernel.destroy();
      tx.destroy();
    }
    transactions.destroy();
  }
);

When("I stop ffi wallet {word}", function (walletName) {
  let wallet = this.getWallet(walletName);
  wallet.stop();
  wallet.resetCounters();
});

Then(
  "I start TXO validation on ffi wallet {word}",
  async function (wallet_name) {
    const wallet = this.getWallet(wallet_name);
    await wallet.startTxoValidation();
    while (!wallet.getTxoValidationStatus().txo_validation_complete) {
      await sleep(1000);
    }
  }
);

Then(
  "I start TX validation on ffi wallet {word}",
  async function (wallet_name) {
    const wallet = this.getWallet(wallet_name);
    await wallet.startTxValidation();
    while (!wallet.getTxValidationStatus().tx_validation_complete) {
      await sleep(1000);
    }
  }
);
