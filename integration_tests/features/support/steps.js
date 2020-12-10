// features/support/steps.js
const { Given, When, Then } = require("cucumber");
const BaseNodeProcess = require('../../helpers/baseNodeProcess');
const expect = require('chai').expect;
const {waitFor, getTransactionOutputHash} = require('../../helpers/util');

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
            console.log(candidate.block.body.outputs);
            this.addOutput(coinbaseName, candidate.block.body.outputs[0]);
            return candidate;
        });
});

When(/I mine (\d+) blocks on (.*)/, {timeout: 600*1000}, async function (numBlocks, name) {
    for(let i=0;i<numBlocks;i++) {
        await this.mineBlock(name);
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

    await client.submitBlock(candidate, block => {
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

    await client.submitBlock(candidate, block => {
        // console.log("Candidate:", block);
        block.block.header.output_mr[0] = 1;
        // block.block.header.height = atHeight + 1;
        // block.block.header.prev_hash = candidate.header.hash;
        return block;
    }).catch(err => {
        console.log("Received expected error. This is fine actually:", err);
    })
});

Then(/I find that the UTXO (.*) exists according to (.*)/, async function (outputName, nodeName) {
    let client = this.getClient(nodeName);
    let hash = getTransactionOutputHash(this.outputs[outputName]);
    let lastResult = await client.fetchMatchingUtxos([hash]);
    expect(lastResult[0].output.commitment.toString('hex')).to.equal(this.outputs[outputName].commitment.toString('hex'));
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
