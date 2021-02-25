// features/support/steps.js
const assert = require('assert');
const { Given, When, Then } = require("cucumber");
const BaseNodeProcess = require('../../helpers/baseNodeProcess');
const MergeMiningProxyProcess = require('../../helpers/mergeMiningProxyProcess');
const WalletProcess = require('../../helpers/walletProcess');
const expect = require('chai').expect;
const {waitFor, getTransactionOutputHash, sleep, consoleLogTransactionDetails, consoleLogBalance,
    consoleLogCoinbaseDetails} = require('../../helpers/util');
const TransactionBuilder = require('../../helpers/transactionBuilder');
var lastResult;


Given(/I have a seed node (.*)/, {timeout: 20*1000}, async function (name) {
    return await this.createSeedNode(name);
    // Write code here that turns the phrase above into concrete actions
});

Given('I have {int} seed nodes',{timeout:20*1000}, async function (n) {
    let promises =[]
    for (let i = 0; i<n; i++) {
        promises.push(this.createSeedNode(`SeedNode${i}`));
    }
    await Promise.all(promises);
});

Given(/I have a base node (.*) connected to all seed nodes/, {timeout: 20*1000}, async function (name) {
    const miner =  new BaseNodeProcess(name);
    miner.setPeerSeeds([this.seedAddresses()]);
    await miner.startNew();
    this.addNode(name, miner);
    });

Given(/I have a base node (.*) connected to seed (.*)/, {timeout: 20*1000}, async function (name, seedNode) {
    const miner =  new BaseNodeProcess(name);
    console.log(this.seeds[seedNode].peerAddress());
    miner.setPeerSeeds([this.seeds[seedNode].peerAddress()]);
    await miner.startNew();
    this.addNode(name, miner);
});

Given(/I have a base node (.*) connected to node (.*)/, {timeout: 20*1000}, async function (name, node) {
    const miner =  new BaseNodeProcess(name);
    miner.setPeerSeeds([this.nodes[node].peerAddress()]);
    await miner.startNew();
    this.addNode(name, miner);
    await sleep(1000);
});



Given(/I have a pruned node (.*) connected to node (.*)/, {timeout: 20*1000}, async function (name, node) {
    const miner =  new BaseNodeProcess(name, { pruningHorizon: 5});
    miner.setPeerSeeds([this.nodes[node].peerAddress()]);
    await miner.startNew();
    this.addNode(name, miner);
    await sleep(1000);
});

Given(/I have a lagging delayed node (.*) connected to node (.*) with blocks_behind_before_considered_lagging (\d+)/, {timeout: 20*1000}, async function (name, node, delay) {
    const miner =  new BaseNodeProcess(name, { blocks_behind_before_considered_lagging: delay});
    miner.setPeerSeeds([this.nodes[node].peerAddress()]);
    await miner.startNew();
    this.addNode(name, miner);
    await sleep(1000);
});

Given(/I have a base node (.*) unconnected/, {timeout: 20*1000}, async function (name) {
    const node = new BaseNodeProcess(name);
    await node.startNew();
    this.addNode(name, node);
});

Given('I have {int} base nodes connected to all seed nodes', {timeout: 190*1000}, async  function (n) {
    let promises = [];
    for (let i=0; i< n; i++) {
       const miner = new BaseNodeProcess(`BaseNode${i}`);
       miner.setPeerSeeds([this.seedAddresses()]);
       promises.push(miner.startNew().then(() => this.addNode(`BaseNode${i}`, miner)));
   }
    await Promise.all(promises);
});

Given(/I have wallet (.*) connected to seed node (.*)/, {timeout: 20*1000}, async function (walletName, seedName) {
    let wallet = new WalletProcess(walletName);
    wallet.setPeerSeeds([this.seeds[seedName].peerAddress()]);
    await wallet.startNew();
    this.addWallet(walletName, wallet);
});

Given(/I have wallet (.*) connected to all seed nodes/, {timeout: 20*1000}, async function (name) {
    let wallet = new WalletProcess(name);
    wallet.setPeerSeeds([this.seedAddresses()]);
    await wallet.startNew();
    this.addWallet(name, wallet);
});

Given(/I have non-default wallet (.*) connected to all seed nodes using (.*)/, {timeout: 20*1000}, async function (name, mechanism) {
    // mechanism: DirectOnly, StoreAndForwardOnly, DirectAndStoreAndForward
    let wallet = new WalletProcess(name, { routingMechanism: mechanism });
    console.log(wallet.name, wallet.options);
    wallet.setPeerSeeds([this.seedAddresses()]);
    await wallet.startNew();
    this.addWallet(name, wallet);
});

Given(/I have (.*) non-default wallets connected to all seed nodes using (.*)/, {timeout: 190*1000}, async  function (n, mechanism) {
    // mechanism: DirectOnly, StoreAndForwardOnly, DirectAndStoreAndForward
    let promises = [];
    for (let i=0; i< n; i++) {
        if (i<10) {
            const wallet = new WalletProcess("Wallet_0" + String(i), { routingMechanism: mechanism});
            console.log(wallet.name, wallet.options);
            wallet.setPeerSeeds([this.seedAddresses()]);
            promises.push(wallet.startNew().then(() => this.addWallet("Wallet_0" + String(i), wallet)));
        } else {
            const wallet = new WalletProcess("Wallet_0" + String(i), { routingMechanism: mechanism});
            console.log(wallet.name, wallet.options);
            wallet.setPeerSeeds([this.seedAddresses()]);
            promises.push(wallet.startNew().then(() => this.addWallet("Wallet_" + String(i), wallet)));
        }
   }
    await Promise.all(promises);
});



Given(/I have a merge mining proxy (.*) connected to (.*) and (.*)/,{timeout: 20*1000}, async function (mmProxy, node, wallet) {
    let baseNode = this.getNode(node);
    let walletNode = this.getWallet(wallet);
    const proxy = new MergeMiningProxyProcess(mmProxy, baseNode.getGrpcAddress(), walletNode.getGrpcAddress());
    await proxy.startNew();
    this.addProxy(mmProxy, proxy);
});


When(/I start (.*)/, {timeout: 20*1000}, async function (name) {
    await this.startNode(name);
});

When(/I stop (.*)/, function (name) {
    this.stopNode(name)
});

Then(/node (.*) is at height (\d+)/, {timeout: 120*1000}, async function (name, height) {
    let client =this.getClient(name);
    await waitFor(async() => client.getTipHeight(), height, 115*1000);
    expect(await client.getTipHeight()).to.equal(height);
});

Then('all nodes are at height {int}', {timeout: 120*1000},async function (height) {
    await this.forEachClientAsync(async (client, name) => {
        await waitFor(async() => client.getTipHeight(), height, 115*1000);
        const currTip = await client.getTipHeight();
        console.log(`Node ${name} is at tip: ${currTip} (should be ${height})`);
        expect(currTip).to.equal(height);
    })
});

When(/I create a transaction (.*) spending (.*) to (.*)/, function (txnName, inputs, output) {

    let txInputs = inputs.split(",").map(input  => this.outputs[input]);
    let txn = new TransactionBuilder();
    txInputs.forEach(txIn => txn.addInput(txIn));
    let txOutput = txn.addOutput(txn.getSpendableAmount());
    this.addOutput(output, txOutput);
    this.transactions[txnName] = txn.build();
});

When(/I submit transaction (.*) to (.*)/, async  function (txn,  node) {
    this.lastResult = await this.getClient(node).submitTransaction(this.transactions[txn]);
    expect(this.lastResult.result).to.equal('ACCEPTED');
});

When(/I submit locked transaction (.*) to (.*)/, async  function (txn,  node) {
    this.lastResult = await this.getClient(node).submitTransaction(this.transactions[txn]);
    expect(this.lastResult.result).to.equal('REJECTED');
});

When(/I spend outputs (.*) via (.*)/, async function (inputs, node) {
    let txInputs = inputs.split(",").map(input  => this.outputs[input]);
    console.log(txInputs);
    let txn = new TransactionBuilder();
    txInputs.forEach(txIn => txn.addInput(txIn));
    console.log(txn.getSpendableAmount());
   let output =  txn.addOutput(txn.getSpendableAmount());
   console.log(output);
    this.lastResult = await this.getClient(node).submitTransaction(txn.build());
    expect(this.lastResult.result).to.equal('ACCEPTED');
});


Then(/(.*) is in the mempool/, function (txn) {
    expect(this.lastResult.result).to.equal('ACCEPTED');
});

Then(/(.*) should not be in the mempool/, function (txn) {
    expect(this.lastResult.result).to.equal('REJECTED');
});

When(/I save the tip on (.*) as (.*)/, async function (node, name) {
    let client = this.getClient(node);
    let header= await client.getTipHeader();
    this.headers[name] = header;
});

Then(/node (.*) is at tip (.*)/, async function (node, name) {
    let client = this.getClient(node);
    let header= await client.getTipHeader();
    // console.log("headers:", this.headers);
    expect(this.headers[name].hash).to.equal(header.hash);
});

When(/I mine a block on (.*) with coinbase (.*)/, {timeout: 600*1000}, async function (name, coinbaseName) {
        await this.mineBlock(name, candidate => {
            this.addOutput(coinbaseName, candidate.originalTemplate.coinbase);
            return candidate;
        });
});

When(/I mine (\d+) blocks on (.*)/, {timeout: 600*1000}, async function (numBlocks, name) {
    for(let i=0;i<numBlocks;i++) {
        await this.mineBlock(name);
    }
});

When(/I merge mine (.*) blocks via (.*)/, {timeout: 600*1000}, async function (numBlocks, mmProxy) {
    for(let i=0;i<numBlocks;i++) {
        await this.mergeMineBlock(mmProxy);
    }
});


When(/I mine but don't submit a block (.*) on (.*)/, async function (blockName, nodeName) {
    await this.mineBlock(nodeName, block => {
        this.saveBlock(blockName, block);
        return false;
    });
});

When(/I submit block (.*) to (.*)/, function (blockName, nodeName) {
    this.submitBlock(blockName, nodeName);
});


When(/I mine a block on (.*) based on height (\d+)/, async function (node, atHeight) {
    let client = this.getClient(node);
    let template = client.getPreviousBlockTemplate(atHeight);
    let candidate = await client.getMinedCandidateBlock(template);

    await client.submitBlock(candidate.template, block => {
        return block;
    }, error => {
        // Expect an error
        console.log(error);
        return false;
    })
});



When(/I mine a block on (.*) at height (\d+) with an invalid MMR/, async function (node, atHeight) {
    let client = this.getClient(node);
    let template = client.getPreviousBlockTemplate(atHeight);
    let candidate = await client.getMinedCandidateBlock(template);

    await client.submitBlock(candidate.template, block => {
        // console.log("Candidate:", block);
        block.block.header.output_mr[0] = 1;
        // block.block.header.height = atHeight + 1;
        // block.block.header.prev_hash = candidate.header.hash;
        return block;
    }).catch(err => {
        console.log("Received expected error. This is fine actually:", err);
    })
});

Then(/the UTXO (.*) has been mined according to (.*)/, async function (outputName, nodeName) {
    let client = this.getClient(nodeName);
    let hash = getTransactionOutputHash(this.outputs[outputName].output);
    let lastResult = await client.fetchMatchingUtxos([hash]);
    expect(lastResult[0].output.commitment.toString('hex')).to.equal(this.outputs[outputName].output.commitment.toString('hex'));
});


Then('I receive an error containing {string}', function (string) {
    // TODO
});

Then(/(.*) should have (\d+) peers/, async function (nodeName, peerCount){
    await sleep(500);
    console.log(nodeName);
    let client = this.getClient(nodeName);
    let peers = await client.getPeers();
    // we add a non existing node when the node starts before adding any actual peers. So the count should always be 1 higher
    expect(peers.length).to.equal(peerCount+1)
})

When('I print the world', function () {
    console.log(this);
});

When(/I wait for wallet (.*) to have at least (.*) tari/, {timeout: 250*1000}, async function (wallet, amount) {
    let walletClient = this.getWallet(wallet).getClient();
    console.log("\n");
    console.log("Waiting for " + wallet + " balance to be at least " + amount + " uT");
    let balance = await walletClient.getBalance();
    consoleLogBalance(balance);
    if (balance["available_balance"] < amount) {
        await waitFor(async() => walletClient.isBalanceAtLeast(amount), true, 240*1000, 5*1000, 5);
        if (!walletClient.isBalanceAtLeast(amount)) {
            console.log("Balance not adequate!");
        }
        consoleLogBalance(await walletClient.getBalance());
    }
});

async function send_tari(sourceWallet, destWallet, tariAmount, fee) {
    // TODO: Remove the while loop when wait on step above and get-balance GRPC interface is implemented
    let sourceWalletClient = sourceWallet.getClient();
    let destInfo = await destWallet.getClient().identify();
    console.log(sourceWallet.name + " sending " + tariAmount + "uT to " + destWallet.name + " `" + destInfo["public_key"] + "`");
    let success = false;
    let retries = 1;
    let retries_limit = 25;
    var lastResult;
    while (!success && retries <= retries_limit) {
        lastResult = await sourceWalletClient.transfer({
            "recipients": [{"address": destInfo["public_key"],
            "amount": tariAmount,
            "fee_per_gram": fee,
            "message": "msg"}]
        });
        success = lastResult.results[0]["is_success"]
        if (!success) {
            let wait_seconds = 5;
            console.log("  " + lastResult.results[0]["failure_message"] + ", trying again after " + wait_seconds +
                "s (" + retries + " of " + retries_limit + ")");
            await sleep(wait_seconds * 1000);
            retries++;
        }
    }
    return lastResult;
}

When(/I send (.*) uT from wallet (.*) to wallet (.*) at fee (.*)/, {timeout: 25*5*1000}, async function (tariAmount, source, dest, fee) {
    let sourceInfo = await this.getWallet(source).getClient().identify();
    let destInfo = await this.getWallet(dest).getClient().identify();
    this.lastResult = await send_tari(this.getWallet(source), this.getWallet(dest), tariAmount, fee);
    expect(this.lastResult.results[0]["is_success"]).to.equal(true);
    this.addTransaction(sourceInfo["public_key"], this.lastResult.results[0]["transaction_id"]);
    this.addTransaction(destInfo["public_key"], this.lastResult.results[0]["transaction_id"]);
    console.log("  Transaction '" + this.lastResult.results[0]["transaction_id"] + "' is_success(" +
        this.lastResult.results[0]["is_success"] + ")");
});

When(/I multi-send (.*) transactions of (.*) uT from wallet (.*) to wallet (.*) at fee (.*)/, {timeout: 25*5*1000}, async function (number, tariAmount, source, dest, fee) {
    console.log("\n");
    let sourceInfo = await this.getWallet(source).getClient().identify();
    let destInfo = await this.getWallet(dest).getClient().identify();
    for (var i = 0; i < number; i++) {
        this.lastResult = await send_tari(this.getWallet(source), this.getWallet(dest), tariAmount, fee);
        expect(this.lastResult.results[0]["is_success"]).to.equal(true);
        this.addTransaction(sourceInfo["public_key"], this.lastResult.results[0]["transaction_id"]);
        this.addTransaction(destInfo["public_key"], this.lastResult.results[0]["transaction_id"]);
        console.log("  Transaction '" + this.lastResult.results[0]["transaction_id"] + "' is_success(" +
            this.lastResult.results[0]["is_success"] + ")");
    }
});

When(/I multi-send (.*) uT from wallet (.*) to all wallets at fee (.*)/, {timeout: 25*5*1000}, async function (tariAmount, source, fee) {
    let sourceWalletClient = this.getWallet(source).getClient();
    let sourceInfo = await sourceWalletClient.identify();

    for (const wallet in this.wallets) {
        if (this.getWallet(source).name == this.getWallet(wallet).name) {
            continue;
        }
        let destInfo = await this.getWallet(wallet).getClient().identify()
        this.lastResult = await send_tari(this.getWallet(source), this.getWallet(wallet), tariAmount, fee);
        expect(this.lastResult.results[0]["is_success"]).to.equal(true);
        this.addTransaction(sourceInfo["public_key"], this.lastResult.results[0]["transaction_id"]);
        this.addTransaction(destInfo["public_key"], this.lastResult.results[0]["transaction_id"]);
        console.log("  Transaction '" + this.lastResult.results[0]["transaction_id"] + "' is_success(" +
            this.lastResult.results[0]["is_success"] + ")");
    }
});

When(/I send (.*) tari from (.*) to (.*),(.*) at fee (.*)/, async function (tariAmount,source,dest,dest2,fee) {
 let wallet = this.getWallet(source);
 let client = wallet.getClient();
 let destWallet = this.getWallet(dest);
 let destClient = destWallet.getClient();
 let destWallet2 = this.getWallet(dest2);
 let destClient2 = destWallet2.getClient();

 var destInfo = await destClient.identify();
 var dest2Info = await destClient2.identify();
 console.log("Starting Transfer of",tariAmount,"to");
 console.log(destInfo);
 console.log(dest2Info);
 let output = await client.transfer({"recipients": [{"address": destInfo["public_key"],
                                                     "amount": tariAmount,
                                                      "fee_per_gram": fee,
                                                      "message": "msg"
                                                     },
                                                     {
                                                      "address": dest2Info["public_key"],
                                                      "amount": tariAmount,
                                                      "fee_per_gram": fee,
                                                      "message": "msg"}]
                                                    });
  console.log("output", output);
  lastResult = output;
});

When(/I wait (.*) seconds/, {timeout: 600*1000}, async  function (int) {
    console.log("Waiting for", int, "seconds");
    await sleep(int*1000);
    console.log("Waiting finished");
});

Then(/Batch transfer of (.*) transactions was a success from (.*) to (.*),(.*)/,  async function (txCount,walletA,walletB,walletC) {
   let walletAObj = this.getWallet(walletA);
   let clientA = walletAObj.getClient();
   let walletBObj = this.getWallet(walletB);
   let clientB = walletBObj.getClient();
   let walletCObj = this.getWallet(walletC);
   let clientC = walletCObj.getClient();

   var resultObj = lastResult["results"];
   console.log(resultObj);
   for(var i = 0; i < txCount; i++) {
       var found = 0;
       var obj = resultObj[i];
       if (obj["is_success"] == false) {
            console.log(obj["transaction_id"],"failed");
            assert(obj["is_success"],true);
       } else {
            console.log("Transaction",obj["transaction_id"],"passed from original request succeeded");
            let req = { "transaction_ids" : [
              obj["transaction_id"].toString()
            ]};
            console.log(req);
            try {
              let txA = await clientA.getTransactionInfo(req);
              console.log(txA);
            } catch (err) {
               console.log(obj["transaction_id"].toString(),"not found in :", await clientA.identify());
            }
            try {
              let txB = await clientB.getTransactionInfo(req);
              console.log(txB);
              found++;
             } catch (err) {
               console.log(obj["transaction_id"].toString(),"not found in :", await clientB.identify());
             }
            try {
              let txC = await clientC.getTransactionInfo(req);
              console.log(txC);
              found++;
            } catch (err) {
               console.log(obj["transaction_id"].toString(),"not found in :",await clientC.identify());
            }
       }
   }

   console.log("Number of transactions found is",found,"of",txCount);
   assert(found == txCount);
   console.log("All transactions found");
});

Then(/wallet (.*) detects all transactions are at least Pending/, {timeout: 3800*1000}, async function (walletName) {
    // Note: This initial step can take a long time if network conditions are not favourable
    // Pending -> Completed -> Broadcast -> Mined Unconfirmed -> Mined Confirmed
    let wallet = this.getWallet(walletName)
    let walletClient = wallet.getClient();
    var walletInfo = await walletClient.identify();

    let txIds = this.transactionsMap.get(walletInfo["public_key"]);
    console.log("\nDetecting transactions as at least Pending: ", walletName, txIds)
    for (i = 0; i < txIds.length; i++) {
         console.log("\n" + wallet.name + ": Waiting for transaction " + txIds[i] + " to register at least Pending in the wallet ...");
         await waitFor(async() => wallet.getClient().isTransactionAtLeastPending(txIds[i]), true, 3700*1000, 5*1000, 5);
         let transactionPending = await wallet.getClient().isTransactionAtLeastPending(txIds[i]);
         let txnDetails = await wallet.getClient().getTransactionDetails(txIds[i]);
         consoleLogTransactionDetails(txnDetails, txIds[i]);
         expect(transactionPending).to.equal(true);
   }

});

Then(/all wallets detect all transactions are at least Pending/, {timeout: 3800*1000}, async function () {
    // Note: This initial step to register pending can take a long time if network conditions are not favourable
    // Pending -> Completed -> Broadcast -> Mined Unconfirmed -> Mined Confirmed
    for (const walletName in this.wallets) {
        let wallet = this.getWallet(walletName)
        let walletClient = wallet.getClient();
        var walletInfo = await walletClient.identify();

        let txIds = this.transactionsMap.get(walletInfo["public_key"]);
        console.log("\nDetecting transactions as at least Pending: ", walletName, txIds)
        for (i = 0; i < txIds.length; i++) {
             console.log("\n" + wallet.name + ": Waiting for transaction " + txIds[i] + " to register at least Pending in the wallet ...");
             await waitFor(async() => wallet.getClient().isTransactionAtLeastPending(txIds[i]), true, 3700*1000, 5*1000, 5);
             let transactionPending = await wallet.getClient().isTransactionAtLeastPending(txIds[i]);
             let txnDetails = await wallet.getClient().getTransactionDetails(txIds[i]);
             consoleLogTransactionDetails(txnDetails, txIds[i]);
             expect(transactionPending).to.equal(true);
       }
   }
});

Then(/wallet (.*) detects all transactions are at least Completed/, {timeout: 1200*1000}, async function (walletName) {
    // Pending -> Completed -> Broadcast -> Mined Unconfirmed -> Mined Confirmed
    let wallet = this.getWallet(walletName)
    let walletClient = wallet.getClient();
    var walletInfo = await walletClient.identify();

    let txIds = this.transactionsMap.get(walletInfo["public_key"]);
    console.log("\nDetecting transactions as at least Completed: ", walletName, txIds)
    for (i = 0; i < txIds.length; i++) {
        // Get details
        console.log("\n" + wallet.name + ": Waiting for transaction " + txIds[i] + " to register at least Completed in the wallet ...");
        await waitFor(async() => wallet.getClient().isTransactionAtLeastCompleted(txIds[i]), true, 600*1000, 5*1000, 5);
        let transactionCompleted = await wallet.getClient().isTransactionAtLeastCompleted(txIds[i]);
        let txnDetails = await wallet.getClient().getTransactionDetails(txIds[i]);
        consoleLogTransactionDetails(txnDetails, txIds[i]);
        expect(transactionCompleted).to.equal(true);
    }
});

Then(/all wallets detect all transactions are at least Completed/, {timeout: 1200*1000}, async function () {
    // Pending -> Completed -> Broadcast -> Mined Unconfirmed -> Mined Confirmed
    for (const walletName in this.wallets) {
        let wallet = this.getWallet(walletName)
        let walletClient = wallet.getClient();
        var walletInfo = await walletClient.identify();

        let txIds = this.transactionsMap.get(walletInfo["public_key"]);
        console.log("\nDetecting transactions as at least Completed: ", walletName, txIds)
        for (i = 0; i < txIds.length; i++) {
            // Get details
            console.log("\n" + wallet.name + ": Waiting for transaction " + txIds[i] + " to register at least Completed in the wallet ...");
            await waitFor(async() => wallet.getClient().isTransactionAtLeastCompleted(txIds[i]), true, 1100*1000, 5*1000, 5);
            let transactionCompleted = await wallet.getClient().isTransactionAtLeastCompleted(txIds[i]);
            let txnDetails = await wallet.getClient().getTransactionDetails(txIds[i]);
            consoleLogTransactionDetails(txnDetails, txIds[i]);
            expect(transactionCompleted).to.equal(true);
        }
    }
});

Then(/wallet (.*) detects all transactions are at least Broadcast/, {timeout: 1200*1000}, async function (walletName) {
    // Pending -> Completed -> Broadcast -> Mined Unconfirmed -> Mined Confirmed
    let wallet = this.getWallet(walletName)
    let walletClient = wallet.getClient();
    var walletInfo = await walletClient.identify();

    let txIds = this.transactionsMap.get(walletInfo["public_key"]);
    console.log("\nDetecting transactions as at least Broadcast: ", walletName, txIds)
    for (i = 0; i < txIds.length; i++) {
        // Get details
        console.log("\n" + wallet.name + ": Waiting for transaction " + txIds[i] + " to register at least Broadcast in the wallet ...");
        await waitFor(async() => wallet.getClient().isTransactionAtLeastBroadcast(txIds[i]), true, 600*1000, 5*1000, 5);
        let transactionBroadcasted = await wallet.getClient().isTransactionAtLeastBroadcast(txIds[i]);
        let txnDetails = await wallet.getClient().getTransactionDetails(txIds[i]);
        consoleLogTransactionDetails(txnDetails, txIds[i]);
        expect(transactionBroadcasted).to.equal(true);
    }
});

Then(/all wallets detect all transactions are at least Broadcast/, {timeout: 1200*1000}, async function () {
    // Pending -> Completed -> Broadcast -> Mined Unconfirmed -> Mined Confirmed
    for (const walletName in this.wallets) {
        let wallet = this.getWallet(walletName)
        let walletClient = wallet.getClient();
        var walletInfo = await walletClient.identify();

        let txIds = this.transactionsMap.get(walletInfo["public_key"]);
        console.log("\nDetecting transactions as at least Broadcast: ", walletName, txIds)
        for (i = 0; i < txIds.length; i++) {
            // Get details
            console.log("\n" + wallet.name + ": Waiting for transaction " + txIds[i] + " to register at least Broadcast in the wallet ...");
            await waitFor(async() => wallet.getClient().isTransactionAtLeastBroadcast(txIds[i]), true, 1100*1000, 5*1000, 5);
            let transactionBroadcasted = await wallet.getClient().isTransactionAtLeastBroadcast(txIds[i]);
            let txnDetails = await wallet.getClient().getTransactionDetails(txIds[i]);
            consoleLogTransactionDetails(txnDetails, txIds[i]);
            expect(transactionBroadcasted).to.equal(true);
        }
    }
});

Then(/wallet (.*) detects all transactions are at least Mined_Unconfirmed/, {timeout: 1200*1000}, async function (walletName) {
    // Pending -> Completed -> Broadcast -> Mined Unconfirmed -> Mined Confirmed
    let wallet = this.getWallet(walletName)
    let walletClient = wallet.getClient();
    var walletInfo = await walletClient.identify();

    let txIds = this.transactionsMap.get(walletInfo["public_key"]);
    console.log("\nDetecting transactions as at least Mined_Unconfirmed: ", walletName, txIds)
    for (i = 0; i < txIds.length; i++) {
        console.log("\n" + wallet.name + ": Waiting for transaction " + txIds[i] + " to be detected as Mined_Unconfirmed in the wallet ...");
        await waitFor(async() => wallet.getClient().isTransactionAtLeastMinedUnconfirmed(txIds[i]), true, 600*1000, 5*1000, 5);
        let isTransactionAtLeastMinedUnconfirmed = await wallet.getClient().isTransactionAtLeastMinedUnconfirmed(txIds[i]);
        let txnDetails = await wallet.getClient().getTransactionDetails(txIds[i]);
        consoleLogTransactionDetails(txnDetails, txIds[i]);
        expect(isTransactionAtLeastMinedUnconfirmed).to.equal(true);
    }
});

Then(/all wallets detect all transactions are at least Mined_Unconfirmed/, {timeout: 1200*1000}, async function () {
    // Pending -> Completed -> Broadcast -> Mined Unconfirmed -> Mined Confirmed
    for (const walletName in this.wallets) {
        let wallet = this.getWallet(walletName)
        let walletClient = wallet.getClient();
        var walletInfo = await walletClient.identify();

        let txIds = this.transactionsMap.get(walletInfo["public_key"]);
        console.log("\nDetecting transactions as at least Mined_Unconfirmed: ", walletName, txIds)
        for (i = 0; i < txIds.length; i++) {
            console.log("\n" + wallet.name + ": Waiting for transaction " + txIds[i] + " to be detected as Mined_Unconfirmed in the wallet ...");
            await waitFor(async() => wallet.getClient().isTransactionAtLeastMinedUnconfirmed(txIds[i]), true, 1100*1000, 5*1000, 5);
            let isTransactionAtLeastMinedUnconfirmed = await wallet.getClient().isTransactionAtLeastMinedUnconfirmed(txIds[i]);
            let txnDetails = await wallet.getClient().getTransactionDetails(txIds[i]);
            consoleLogTransactionDetails(txnDetails, txIds[i]);
            expect(isTransactionAtLeastMinedUnconfirmed).to.equal(true);
        }
    }
});

Then(/wallet (.*) detects all transactions as Mined_Unconfirmed/, {timeout: 1200*1000}, async function (walletName) {
    // Pending -> Completed -> Broadcast -> Mined Unconfirmed -> Mined Confirmed
    let wallet = this.getWallet(walletName)
    let walletClient = wallet.getClient();
    var walletInfo = await walletClient.identify();

    let txIds = this.transactionsMap.get(walletInfo["public_key"]);
    console.log("\nDetecting transactions as Mined_Unconfirmed: ", walletName, txIds)
    for (i = 0; i < txIds.length; i++) {
        console.log("\n" + wallet.name + ": Waiting for transaction " + txIds[i] + " to be detected as Mined_Unconfirmed in the wallet ...");
        await waitFor(async() => wallet.getClient().isTransactionMinedUnconfirmed(txIds[i]), true, 600*1000, 5*1000, 5);
        let isTransactionMinedUnconfirmed = await wallet.getClient().isTransactionMinedUnconfirmed(txIds[i]);
        let txnDetails = await wallet.getClient().getTransactionDetails(txIds[i]);
        consoleLogTransactionDetails(txnDetails, txIds[i]);
        expect(isTransactionMinedUnconfirmed).to.equal(true);
    }
});

Then(/all wallets detect all transactions as Mined_Unconfirmed/, {timeout: 1200*1000}, async function () {
    // Pending -> Completed -> Broadcast -> Mined Unconfirmed -> Mined Confirmed
    for (const walletName in this.wallets) {
        let wallet = this.getWallet(walletName)
        let walletClient = wallet.getClient();
        var walletInfo = await walletClient.identify();

        let txIds = this.transactionsMap.get(walletInfo["public_key"]);
        console.log("\nDetecting transactions as Mined_Unconfirmed: ", walletName, txIds)
        for (i = 0; i < txIds.length; i++) {
            console.log("\n" + wallet.name + ": Waiting for transaction " + txIds[i] + " to be detected as Mined_Unconfirmed in the wallet ...");
            await waitFor(async() => wallet.getClient().isTransactionMinedUnconfirmed(txIds[i]), true, 1100*1000, 5*1000, 5);
            let isTransactionMinedUnconfirmed = await wallet.getClient().isTransactionMinedUnconfirmed(txIds[i]);
            let txnDetails = await wallet.getClient().getTransactionDetails(txIds[i]);
            consoleLogTransactionDetails(txnDetails, txIds[i]);
            expect(isTransactionMinedUnconfirmed).to.equal(true);
        }
    }
});

Then(/wallet (.*) detects all transactions as Mined_Confirmed/, {timeout: 1200*1000}, async function (walletName) {
    // Pending -> Completed -> Broadcast -> Mined Unconfirmed -> Mined Confirmed
    let wallet = this.getWallet(walletName)
    let walletClient = wallet.getClient();
    var walletInfo = await walletClient.identify();

    let txIds = this.transactionsMap.get(walletInfo["public_key"]);
    console.log("\nDetecting transactions as Mined_Confirmed: ", walletName, txIds)
    for (i = 0; i < txIds.length; i++) {
        console.log("\n" + wallet.name + ": Waiting for transaction " + txIds[i] + " to be detected as Mined_Confirmed in the wallet ...");
        await waitFor(async() => wallet.getClient().isTransactionMinedConfirmed(txIds[i]), true, 600*1000, 5*1000, 5);
        let isTransactionMinedConfirmed = await wallet.getClient().isTransactionMinedConfirmed(txIds[i]);
        let txnDetails = await wallet.getClient().getTransactionDetails(txIds[i]);
        consoleLogTransactionDetails(txnDetails, txIds[i]);
        expect(isTransactionMinedConfirmed).to.equal(true);
    }
});

Then(/all wallets detect all transactions as Mined_Confirmed/, {timeout: 1200*1000}, async function () {
    // Pending -> Completed -> Broadcast -> Mined Unconfirmed -> Mined Confirmed
    for (const walletName in this.wallets) {
        let wallet = this.getWallet(walletName)
        let walletClient = wallet.getClient();
        var walletInfo = await walletClient.identify();

        let txIds = this.transactionsMap.get(walletInfo["public_key"]);
        console.log("\nDetecting transactions as Mined_Confirmed: ", walletName, txIds)
        for (i = 0; i < txIds.length; i++) {
            console.log("\n" + wallet.name + ": Waiting for transaction " + txIds[i] + " to be detected as Mined_Confirmed in the wallet ...");
            await waitFor(async() => wallet.getClient().isTransactionMinedConfirmed(txIds[i]), true, 1100*1000, 5*1000, 5);
            let isTransactionMinedConfirmed = await wallet.getClient().isTransactionMinedConfirmed(txIds[i]);
            let txnDetails = await wallet.getClient().getTransactionDetails(txIds[i]);
            consoleLogTransactionDetails(txnDetails, txIds[i]);
            expect(isTransactionMinedConfirmed).to.equal(true);
        }
    }
});

When(/I list all coinbase transactions for wallet (.*)/, {timeout: 20*1000}, async function (walletName) {
    let wallet = this.getWallet(walletName);
    let walletClient = wallet.getClient();
    console.log("\nListing all coinbase transactions: ", walletName)
    let transactions = await walletClient.getAllCoinbaseTransactions();
    if (transactions.length > 0) {
        for (i = 0; i < transactions.length; i++) {
            consoleLogCoinbaseDetails(transactions[i]);
        }
    } else {
         console.log("  No coinbase transactions found!");
    }
});

Then(/wallet (.*) has (.*) coinbase transactions/, {timeout: 20*1000}, async function (walletName, count) {
    let walletClient = this.getWallet(walletName).getClient();
    let transactions = await walletClient.getAllCoinbaseTransactions();
    expect(transactions.length).to.equal(Number(count));
    this.resultStack.push([walletName, transactions.length]);
});

Then(/wallet (.*) detects at least (.*) coinbase transactions as Mined_Confirmed/, {timeout: 605*1000}, async function (walletName, count) {
    let walletClient = this.getWallet(walletName).getClient();
    await waitFor(async() => walletClient.areCoinbasesConfirmedAtLeast(count), true, 600*1000, 5*1000, 5);
    let transactions = await walletClient.getAllSpendableCoinbaseTransactions();
    expect(transactions.length >= count).to.equal(true);
});

Then(/the number of coinbase transactions for wallet (.*) and wallet (.*) are (.*) less/, {timeout: 20*1000}, async function (
    walletNameA,
    walletNameB,
    count
) {
    let walletClientA = this.getWallet(walletNameA).getClient();
    let transactionsA = await walletClientA.getAllCoinbaseTransactions();
    let walletClientB = this.getWallet(walletNameB).getClient();
    let transactionsB = await walletClientB.getAllCoinbaseTransactions();
    if (this.resultStack.length >= 2) {
        let walletStats = [this.resultStack.pop(), this.resultStack.pop()];
        console.log(
            "\nCoinbase comparison: Expect this (current + deficit)", transactionsA.length, transactionsB.length, Number(count),
            "to equal this (previous)", walletStats[0][1], walletStats[1][1]
        );
        expect(
            transactionsA.length + transactionsB.length + Number(count)
        ).to.equal(
            walletStats[0][1] + walletStats[1][1]
        );
    } else {
        except("\nCoinbase comparison: Not enough results saved on the stack!").to.equal("")
    }
});


When(/I request the difficulties of a node (.*)/, async function (node) {
          let client = this.getClient(node);
          let difficulties = await client.getNetworkDifficulties(2,0,2);
          this.lastResult = difficulties;
});

Then('Difficulties are available', function () {
           console.log(this.lastResult);
           assert(this.lastResult.length,3);
           // check genesis block, chain in reverse height order
           assert(this.lastResult[2]["difficulty"],'1');
           assert(this.lastResult[2]["estimated_hash_rate"],'0');
           assert(this.lastResult[2]["height"],'1');
           assert(this.lastResult[2]["pow_algo"],'0');

});
