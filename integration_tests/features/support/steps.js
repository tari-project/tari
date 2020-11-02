// features/support/steps.js
const { Given, When, Then } = require("cucumber");
const BaseNodeProcess = require('../../helpers/baseNodeProcess');
const expect = require('chai').expect;
const {waitFor} = require('../../helpers/util');


Given(/I have a seed node (.*)/, {timeout: 20*1000}, async function (name) {
    return await this.createSeedNode(name);
    // Write code here that turns the phrase above into concrete actions
});

Given(/I have a base node (.*) connected to (.*)/, {timeout: 20*1000}, async function (name, seedNode) {
    const miner = new BaseNodeProcess();
    miner.setPeerSeeds([this.seeds[seedNode].peerAddress()]);
    await miner.startNew();
    this.addNode(name, miner);
});

Given(/I have a base node (.*) unconnected/, {timeout: 20*1000}, async function (name) {
    const node = new BaseNodeProcess();
    await node.startNew();
    this.addNode(name, node);
});

When(/I mine (\d+) blocks on (.*)/, {timeout: 20*1000}, async function (numBlocks, name) {
    for(let i=0;i<numBlocks;i++) {
        await this.mineBlock(name);
    }
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

When(/I save the tip on (.*) as (.*)/, async function (node, name) {
    let client = this.getClient(node);
    let header= await client.getTipHeader();
    this.headers[name] = header;
});

Then(/node (.*) is at tip (.*)/, async function (node, name) {
    let client = this.getClient(node);
    let header= await client.getTipHeader();
    console.log("headers:", this.headers);
    expect(this.headers[name].hash).to.equal(header.hash);
});


When(/I mine a block on (.*) based on height (\d+)/, async function (node, atHeight) {
    let client = this.getClient(node);
    let template = client.getPreviousBlockTemplate(atHeight);
    console.log("Candidate before: ", template);
    let candidate = await client.getMinedCandidateBlock(template);
    console.log("Candidate for mining:", candidate);

    await client.submitBlock(candidate, block => {
        console.log("Candidate:", block);
        // block.block.header.output_mr[0] = 1;
        // block.block.header.height = atHeight + 1;
        // block.block.header.prev_hash = candidate.header.hash;
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
        console.log("Candidate:", block);
        // block.block.header.output_mr[0] = 1;
        // block.block.header.height = atHeight + 1;
        // block.block.header.prev_hash = candidate.header.hash;
        return block;
    }, error => {
        // Expect an error
        console.log(error);
        return false;
    })
});
