const assert = require('assert');
const grpc = require('grpc');
const protoLoader = require('@grpc/proto-loader');
const grpc_promise = require('grpc-promise');
let client;
let walletClient;

const PROTO_PATH = __dirname + '/../../applications/tari_app_grpc/proto/base_node.proto';
const packageDefinition = protoLoader.loadSync(
    PROTO_PATH,
    {
        keepCase: true,
        longs: String,
        enums: String,
        defaults: true,
        oneofs: true
    });
const protoDescriptor = grpc.loadPackageDefinition(packageDefinition);
const tari = protoDescriptor.tari.rpc;
client = new tari.BaseNode('127.0.0.1:50051', grpc.credentials.createInsecure());
grpc_promise.promisifyAll(client);

const WALLET_PROTO_PATH = __dirname + '/../../applications/tari_app_grpc/proto/wallet.proto';
const packageDefinition2 = protoLoader.loadSync(
    WALLET_PROTO_PATH,
    {
        keepCase: true,
        longs: String,
        enums: String,
        defaults: true,
        oneofs: true
    });
const protoDescriptor2 = grpc.loadPackageDefinition(packageDefinition2);
const tariWallet = protoDescriptor2.tari.rpc;
walletClient = new tariWallet.Wallet('127.0.0.1:50061', grpc.credentials.createInsecure());
grpc_promise.promisifyAll(walletClient);

describe('Base Node', function () {
    this.timeout(10000); // five minutes
    describe('GetVersion', function () {
        it('should return', function () {

            return client.getVersion()
                .sendMessage({}).then(constants => {
                    console.log("returned");
                    console.log(constants);
                });
        });
    });

    describe('GetBlockTemplate', function () {
        it('Should return', function () {
            return client.getNewBlockTemplate().sendMessage({}).then(result => {

                console.log(result);
            });
        })
    });

    describe('Miner', function () {
        it('As a miner I want to mine a block', function () {
            let block;
            return client.getNewBlockTemplate()
                .sendMessage({pow_algo: 1})
                .then(template => {
                    console.log(template);
                    block = template.new_block_template;
                    return walletClient.getCoinbase()
                        .sendMessage({
                            "reward": template.block_reward,
                            "fee": 0,
                            "height": block.header.height
                        });
                }).then(coinbase => {

                        console.log("Coinbase:", coinbase);
                        const cb = coinbase.transaction;
                        block.body.outputs = block.body.outputs.concat(cb.body.outputs);
                        block.body.kernels = block.body.kernels.concat(cb.body.kernels);
                        return client.getNewBlock().sendMessage(block);
                    }
                ).then(b => {
                        console.log("Block:" , b);
                        return client.submitBlock().sendMessage(b.block);
                    }
                ).then(empty => {
                        console.log(empty);

                        return client.getTipInfo().sendMessage({});
                    }
                ).then(tipInfo => {

                    console.log(tipInfo);
                    assert.equal(tipInfo.metadata.height_of_longest_chain, 1);
                });
        })
    });
});
