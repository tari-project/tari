// features/support/steps.js
const assert = require('assert');
const { Given, When, Then } = require("cucumber");
const BaseNodeProcess = require('../../helpers/baseNodeProcess');
const MergeMiningProxyProcess = require('../../helpers/mergeMiningProxyProcess');
const WalletProcess = require('../../helpers/walletProcess');
const expect = require('chai').expect;
const {waitFor, getTransactionOutputHash} = require('../../helpers/util');
const TransactionBuilder = require('../../helpers/transactionBuilder');
var lastResult;

function sleep(ms) {
    return new Promise(resolve => setTimeout(resolve, ms));
  }

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

Given(/I have a base node (.*) unconnected/, {timeout: 20*1000}, async function (name) {
    const node = new BaseNodeProcess(name);
    await node.startNew();
    this.addNode(name, node);
});

Given('I have {int} base nodes connected to all seed nodes',{timeout: 190*1000}, async  function (n) {
    let promises = [];
    for (let i=0; i< n; i++) {
       const miner = new BaseNodeProcess(`BaseNode${i}`);
       miner.setPeerSeeds([this.seedAddresses()]);
       promises.push(miner.startNew().then(() => this.addNode(`BaseNode${i}`, miner)));
   }
    await Promise.all(promises);
});

Given(/I have wallet (.*) connected to all seed nodes/, {timeout: 20*1000}, async function (name) {
    let wallet = new WalletProcess(name);
    wallet.setPeerSeeds([this.seedAddresses()]);
    await wallet.startNew();
    this.addWallet(name, wallet);
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

Then(/node (.*) is at height (\d+)/, {timeout: 60*1000}, async function (name, height) {
    let client =this.getClient(name);
    await waitFor(async() => client.getTipHeight(), height, 55000);
    expect(await client.getTipHeight()).to.equal(height);
});

Then('all nodes are at height {int}', {timeout: 120*1000},async function (height) {
    await this.forEachClientAsync(async (client, name) => {
        await waitFor(async() => client.getTipHeight(), height, 115000);
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

When(/I send (.*) tari from (.*) to one wallet (.*) at fee (.*)/, {timeout: 260*1000}, async function (tariAmount, source, dest, fee) {
    let sourceWalletClient = this.getWallet(source).getClient();
    let destWalletClient = this.getWallet(dest).getClient();
    var destInfo = await destWalletClient.identify();
    console.log("\n");
    console.log("Sending " + tariAmount + "uT to `" + destInfo["public_key"] + "`");
    let success = false;
    let retries = 1;
    let retries_limit = 25;
    while (!success && retries <= retries_limit) {
        this.lastResult = await sourceWalletClient.transfer({
            "recipients": [{"address": destInfo["public_key"],
            "amount": tariAmount,
            "fee_per_gram": fee,
            "message": "msg"}]
        });
        success = this.lastResult.results[0]["is_success"]
        if (!success) {
            let wait_seconds = 10;
            console.log("  " + this.lastResult.results[0]["failure_message"] + ", trying again after " + wait_seconds +
                "s (" + retries + " of " + retries_limit + ")");
            await sleep(wait_seconds * 1000);
            retries++;
        }
    }
    console.log("  Transaction '" + this.lastResult.results[0]["transaction_id"] + "' is_success(" +
        this.lastResult.results[0]["is_success"] + ")");
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

Then(/Transaction status of last result from (.*) to (.*) is known to both wallets/, {timeout: 2*360*10*1000}, async function (walletA, walletB) {
    let wallets = [this.getWallet(walletA), this.getWallet(walletB)];
    let found = [false, false];

    if (this.lastResult.results[0]["is_success"] == false) {
        console.log("Transaction '" + this.lastResult.results[0]["transaction_id"] + "' failed");
        expect(this.lastResult.results[0]["is_success"]).to.equal(true);
    }
    let tx_id = this.lastResult.results[0]["transaction_id"];
    console.log("\n");
    var i, retries, retries_limit;
    for (i = 0; i < wallets.length; i++) {
        console.log("Get transaction status from " + wallets[i].name + " for tx_id " + tx_id);
        retries = 1;
        retries_limit = 360;
        while (!found[i] && retries <= retries_limit) {
            try {
                let txnDetails = await wallets[i].getClient().getTransactionInfo({
                    "transaction_ids": [ tx_id.toString() ]
                });
                found[i] = true;
                console.log("  " + wallets[i].name + ": transaction '" + txnDetails.transactions[0]["tx_id"] +
                    "' has status '" + txnDetails.transactions[0]["status"] + "' and is_cancelled(" +
                    txnDetails.transactions[0]["is_cancelled"] + ")");
            } catch (err) {
                let wait_seconds = 10;
                console.log("  msg: '" + err.details + "', trying again after " + wait_seconds +
                    "s (" + retries + " of " + retries_limit + ")");
                await sleep(wait_seconds * 1000);
                retries++;
            }
        }
    }
    console.log("\n");

    expect(found[0] && found[1]).to.equal(true);
});

