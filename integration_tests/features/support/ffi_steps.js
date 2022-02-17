const { Given, When, Then } = require("@cucumber/cucumber");
const expect = require("chai").expect;

const { sleep, waitForIterate } = require("../../helpers/util");

Then("I want to get emoji id of ffi wallet {word}", async function (name) {
  let wallet = this.getWallet(name);
  let emoji_id = wallet.identifyEmoji();
  console.log(emoji_id);
  expect(emoji_id.length).to.be.equal(
    22 * 3, // 22 emojis, 3 bytes per one emoji
    `Emoji id has wrong length : ${emoji_id}`
  );
});

When(
  "I send {int} uT from ffi wallet {word} to wallet {word} at fee {int}",
  function (amount, sender, receiver, feePerGram) {
    let ffiWallet = this.getWallet(sender);
    let result = ffiWallet.sendTransaction(
      this.getWalletPubkey(receiver),
      amount,
      feePerGram,
      `Send from ffi ${sender} to ${receiver} at fee ${feePerGram}`,
      false
    );
    console.log(result);
  }
);

When(
  "I send {int} uT from ffi wallet {word} to wallet {word} at fee {int} via one-sided transactions",
  function (amount, sender, receiver, feePerGram) {
    let ffiWallet = this.getWallet(sender);
    let result = ffiWallet.sendTransaction(
      this.getWalletPubkey(receiver),
      amount,
      feePerGram,
      `Send from ffi ${sender} to ${receiver} at fee ${feePerGram}`,
      true
    );
    console.log(result);
  }
);

When(
  "I set passphrase {word} of ffi wallet {word}",
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

When(
  "I add contact with alias {word} and pubkey {word} to ffi wallet {word}",
  function (alias, walletName, ffiWalletName) {
    let ffiWallet = this.getWallet(ffiWalletName);
    ffiWallet.addContact(alias, this.getWalletPubkey(walletName));
  }
);

Then(
  "I have contact with alias {word} and pubkey {word} in ffi wallet {word}",
  function (alias, walletName, ffiWalletName) {
    let wallet = this.getWalletPubkey(walletName);
    let ffiWallet = this.getWallet(ffiWalletName);
    let contacts = ffiWallet.getContactList();
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
  function (alias, walletName) {
    let ffiWallet = this.getWallet(walletName);
    let contacts = ffiWallet.getContactList();
    let length = contacts.getLength();
    for (let i = 0; i < length; i++) {
      {
        let contact = contacts.getAt(i);
        let calias = contact.getAlias();
        if (alias === calias) {
          ffiWallet.removeContact(contact);
        }
        contact.destroy();
      }
    }
    contacts.destroy();
  }
);

Then(
  "I don't have contact with alias {word} in ffi wallet {word}",
  function (alias, walletName) {
    let ffiWallet = this.getWallet(walletName);
    let contacts = ffiWallet.getContactList();
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
  function (node, walletName) {
    let wallet = this.getWallet(walletName);
    let peer = this.nodes[node].peerAddress().split("::");
    wallet.addBaseNodePeer(peer[0], peer[1]);
  }
);

Then(
  "I wait for ffi wallet {word} to have {int} pending outbound transaction(s)",
  { timeout: 125 * 1000 },
  async function (walletName, count) {
    let wallet = this.getWallet(walletName);
    let broadcast = wallet.getOutboundTransactions();
    let length = broadcast.getLength();
    broadcast.destroy();
    let retries = 1;
    const retries_limit = 120;
    while (length != count && retries <= retries_limit) {
      await sleep(1000);
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
  function (walletName, count) {
    const wallet = this.getWallet(walletName);
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
  async function (walletName, nodeName) {
    let ffiWallet = await this.createAndAddFFIWallet(walletName, null);
    let peer = this.nodes[nodeName].peerAddress().split("::");
    ffiWallet.addBaseNodePeer(peer[0], peer[1]);
  }
);

Then(
  "I recover wallet {word} into ffi wallet {word} from seed words on node {word}",
  async function (walletName, ffiWalletName, node) {
    let wallet = this.getWallet(walletName);
    const seed_words_text = wallet.getSeedWords();
    wallet.stop();
    await sleep(1000);
    let ffiWallet = await this.createAndAddFFIWallet(
      ffiWalletName,
      seed_words_text
    );
    let peer = this.nodes[node].peerAddress().split("::");
    ffiWallet.addBaseNodePeer(peer[0], peer[1]);
    ffiWallet.startRecovery(peer[0]);
  }
);

Then(
  "I retrieve the mnemonic word list for {word} from ffi wallet {word}",
  async function (language, walletName) {
    const wallet = this.getWallet(walletName);
    const mnemonicWordList = wallet.getMnemonicWordListForLanguage(language);
    console.log("Mnemonic word list for", language, ":", mnemonicWordList);
    expect(mnemonicWordList.length).to.equal(2048);
  }
);

Then(
  "Check callbacks for finished inbound tx on ffi wallet {word}",
  async function (walletName) {
    const wallet = this.getWallet(walletName);
    expect(wallet.receivedTransaction).to.be.greaterThanOrEqual(1);
    expect(wallet.transactionBroadcast).to.be.greaterThanOrEqual(1);
    wallet.clearCallbackCounters();
  }
);

Then(
  "Check callbacks for finished outbound tx on ffi wallet {word}",
  async function (walletName) {
    const wallet = this.getWallet(walletName);
    expect(wallet.receivedTransactionReply).to.be.greaterThanOrEqual(1);
    expect(wallet.transactionBroadcast).to.be.greaterThanOrEqual(1);
    wallet.clearCallbackCounters();
  }
);

Then(
  "I wait for ffi wallet {word} to receive {int} transaction",
  { timeout: 125 * 1000 },
  async function (walletName, amount) {
    let wallet = this.getWallet(walletName);

    console.log("\n");
    console.log(
      "Waiting for " + walletName + " to receive " + amount + " transaction(s)"
    );

    await waitForIterate(
      () => {
        return wallet.getCounters().received >= amount;
      },
      true,
      1000,
      120
    );

    if (wallet.getCounters().received < amount) {
      console.log(
        `Expected to receive at least ${amount} transaction(s) but got ${
          wallet.getCounters().received
        }`
      );
    } else {
      console.log(wallet.getCounters());
    }
    expect(wallet.getCounters().received).to.be.gte(amount);
  }
);

Then(
  "I wait for ffi wallet {word} to receive {int} finalization",
  { timeout: 125 * 1000 },
  async function (walletName, amount) {
    let wallet = this.getWallet(walletName);

    console.log("\n");
    console.log(
      "Waiting for " +
        walletName +
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
      120
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
  { timeout: 125 * 1000 },
  async function (walletName, amount) {
    let wallet = this.getWallet(walletName);

    console.log("\n");
    console.log(
      "Waiting for " +
        walletName +
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
      120
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
  { timeout: 125 * 1000 },
  async function (walletName, amount) {
    let wallet = this.getWallet(walletName);

    console.log("\n");
    console.log(
      "Waiting for " +
        walletName +
        " to receive " +
        amount +
        " transaction(s) mined"
    );

    await waitForIterate(
      () => {
        return wallet.getCounters().mined >= amount;
      },
      true,
      1000,
      120
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
  "I wait for ffi wallet {word} to receive {word} {int} SAF message",
  { timeout: 125 * 1000 },
  async function (walletName, comparison, amount) {
    const atLeast = "AT_LEAST";
    const exactly = "EXACTLY";
    expect(comparison === atLeast || comparison === exactly).to.equal(true);
    let wallet = this.getWallet(walletName);
    console.log(
      "\nWaiting for " +
        walletName +
        " to receive " +
        comparison +
        " " +
        amount +
        " SAF messages(s)"
    );

    await waitForIterate(
      () => {
        return wallet.getCounters().saf >= amount;
      },
      true,
      1000,
      120
    );

    if (wallet.getCounters().saf < amount) {
      console.log(
        `Expected to receive ${amount} SAF messages but received ${
          wallet.getCounters().saf
        }`
      );
    } else {
      console.log(wallet.getCounters());
    }
    if (comparison === atLeast) {
      expect(wallet.getCounters().saf).to.be.gte(amount);
    } else {
      expect(wallet.getCounters().saf).to.be.equal(amount);
    }
  }
);

Then(
  "I wait for ffi wallet {word} to have at least {int} uT",
  { timeout: 125 * 1000 },
  async function (walletName, amount) {
    let wallet = this.getWallet(walletName);

    console.log("\n");
    console.log(
      "Waiting for " + walletName + " balance to be at least " + amount + " uT"
    );

    await waitForIterate(
      () => {
        return wallet.getBalance().available >= amount;
      },
      true,
      1000,
      120
    );

    let balance = wallet.pollBalance().available;

    if (!(balance >= amount)) {
      console.log("Balance not adequate!");
    }

    expect(balance >= amount).to.equal(true);
  }
);

Then(
  "ffi wallet {word} detects {word} {int} ffi transactions to be {word}",
  { timeout: 125 * 1000 },
  async function (walletName, comparison, amount, status) {
    // Pending -> Completed -> Broadcast -> Mined Unconfirmed -> Mined Confirmed
    const atLeast = "AT_LEAST";
    const exactly = "EXACTLY";
    expect(comparison === atLeast || comparison === exactly).to.equal(true);
    const broadcast = "TRANSACTION_STATUS_BROADCAST";
    const fauxUnconfirmed = "TRANSACTION_STATUS_FAUX_UNCONFIRMED";
    const fauxConfirmed = "TRANSACTION_STATUS_FAUX_CONFIRMED";
    expect(
      status === broadcast ||
        status === fauxUnconfirmed ||
        status === fauxConfirmed
    ).to.equal(true);
    const wallet = this.getWallet(walletName);

    console.log("\n");
    console.log(
      "Waiting for " +
        walletName +
        " to have detected " +
        comparison +
        " " +
        amount +
        " " +
        status +
        " transaction(s)"
    );

    await waitForIterate(
      () => {
        switch (status) {
          case broadcast:
            return wallet.getCounters().broadcast >= amount;
          case fauxUnconfirmed:
            return wallet.getCounters().fauxUnconfirmed >= amount;
          case fauxConfirmed:
            return wallet.getCounters().fauxConfirmed >= amount;
          default:
            expect(status).to.equal("please add this<< TransactionStatus");
        }
      },
      true,
      1000,
      120
    );

    let amountOfCallbacks;
    switch (status) {
      case broadcast:
        amountOfCallbacks = wallet.getCounters().broadcast;
        break;
      case fauxUnconfirmed:
        amountOfCallbacks = wallet.getCounters().fauxUnconfirmed;
        break;
      case fauxConfirmed:
        amountOfCallbacks = wallet.getCounters().fauxConfirmed;
        break;
      default:
        expect(status).to.equal("please add this<< TransactionStatus");
    }

    if (!(amountOfCallbacks >= amount)) {
      console.log("\nCounter not adequate!", wallet.getCounters());
    } else {
      console.log(wallet.getCounters());
    }
    if (comparison === atLeast) {
      expect(amountOfCallbacks >= amount).to.equal(true);
    } else {
      expect(amountOfCallbacks === amount).to.equal(true);
    }
  }
);

Then(
  "I wait for recovery of ffi wallet {word} to finish",
  { timeout: 600 * 1000 },
  function (walletName) {
    const wallet = this.getWallet(walletName);
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
    console.log(`Starting ${walletName}`);
    let wallet = this.getWallet(walletName);
    await wallet.restart();
    let [pub_key, address] = this.nodes[node].peerAddress().split("::");
    wallet.addBaseNodePeer(pub_key, address);
  }
);

Then("I want to get public key of ffi wallet {word}", function (name) {
  let wallet = this.getWallet(name);
  let public_key = wallet.identify();
  expect(public_key.length).to.be.equal(
    64,
    `Public key has wrong length : ${public_key}`
  );
});

Then(
  "I want to view the transaction kernels for completed transactions in ffi wallet {word}",
  { timeout: 20 * 1000 },
  function (name) {
    let ffiWallet = this.getWallet(name);
    let transactions = ffiWallet.getCompletedTxs();
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

When(
  "I stop ffi wallet {word}",
  { timeout: 20 * 1000 },
  async function (walletName) {
    console.log(`Stopping ${walletName}`);
    let wallet = this.getWallet(walletName);
    await wallet.stop();
    wallet.resetCounters();
  }
);

Then(
  "I start TXO validation on ffi wallet {word}",
  { timeout: 125 * 1000 },
  async function (walletName) {
    const wallet = this.getWallet(walletName);
    await wallet.startTxoValidation();
    while (!wallet.getTxoValidationStatus().txo_validation_complete) {
      await sleep(1000);
    }
  }
);

Then(
  "I start TX validation on ffi wallet {word}",
  { timeout: 125 * 1000 },
  async function (walletName) {
    const wallet = this.getWallet(walletName);
    await wallet.startTxValidation();
    while (!wallet.getTxValidationStatus().tx_validation_complete) {
      await sleep(1000);
    }
  }
);

Then(
  "I wait for ffi wallet {word} to connect to {word}",
  { timeout: 125 * 1000 },
  async function (walletName, nodeName) {
    const wallet = this.getWallet(walletName, true);
    const client = this.getClient(nodeName);
    // let client = await node.connectClient();
    let nodeIdentity = await client.identify();

    await waitForIterate(
      () => {
        let publicKeys = wallet.listConnectedPublicKeys();
        return (
          publicKeys && publicKeys.some((p) => p === nodeIdentity.public_key)
        );
      },
      true,
      1000,
      10
    );
  }
);
