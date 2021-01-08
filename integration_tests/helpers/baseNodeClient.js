const expect = require('chai').expect;
const grpc = require('grpc');
const protoLoader = require('@grpc/proto-loader');
const grpc_promise = require('grpc-promise');
const TransactionBuilder = require('./transactionBuilder');
const {SHA3} = require('sha3');
const {toLittleEndian} = require('./util');
const cloneDeep = require('clone-deep');

class BaseNodeClient {

    constructor(clientOrPort) {
        if (typeof (clientOrPort) === "number") {
            this.client = this.createGrpcClient(clientOrPort);
        } else {
            this.client = clientOrPort;
        }
        this.blockTemplates = {};
    }

    createGrpcClient(port) {
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
        let client = new tari.BaseNode('127.0.0.1:' + port, grpc.credentials.createInsecure());
        grpc_promise.promisifyAll(client);
        return client;
    }

    getHeaderAt(height) {
        return this.client.listHeaders().sendMessage({from_height: height, num_headers: 1}).then(header => {
            console.log("Header:", header);
            return header;
        })
    }

    getPeers() {
        return this.client.getPeers().sendMessage({}).then(peers=> {
            console.log("Got ", peers.length," peers:");
            return peers;
        })
    }

    getTipHeader() {
        return this.client.listHeaders().sendMessage({from_height: 0, num_headers: 1}).then(header => {
            return header;
        })
    }

    getPreviousBlockTemplate(height) {
        return cloneDeep(this.blockTemplates["height" + height]);
    }

    getBlockTemplate() {
        return this.client.getNewBlockTemplate()
            .sendMessage({pow_algo: 2})
            .then(template => {
                let res = {minerData: template.miner_data, block: template.new_block_template};
                this.blockTemplates["height" + template.new_block_template.header.height] = cloneDeep(res);
                return res;
            });
    }

    submitBlockWithCoinbase(template, coinbase) {

        const cb = coinbase;
        template.body.outputs = template.body.outputs.concat(cb.outputs);
        template.body.kernels = template.body.kernels.concat(cb.kernels);
        return this.client.getNewBlock().sendMessage(template)
            .then(b => {
                    return this.client.submitBlock().sendMessage(b.block);
                }
            );
    }

    submitTemplate(template, beforeSubmit) {
        return this.client.getNewBlock().sendMessage(template.template)
            .then(b => {
                    //console.log("Sha3 diff", this.getSha3Difficulty(b.block.header));
                    if (beforeSubmit) {
                        b = beforeSubmit({block:b, originalTemplate:template});
                        if (!b) {
                            return Promise.resolve();
                        }
                        b = b.block;
                    }
                    return this.client.submitBlock().sendMessage(b.block);
                }
            );
    }

    submitBlock(b) {
        return this.client.submitBlock().sendMessage(b.block);
    }

    submitTransaction(txn) {
        return this.client.submitTransaction().sendMessage({transaction: txn}).then(
          res => {
            return res
          }
        );
    }

    getTipHeight() {
        return this.client.getTipInfo()
            .sendMessage({})
            .then(tip => {
                return parseInt(tip.metadata.height_of_longest_chain);
            });
    }

    fetchMatchingUtxos(hashes) {
        return this.client.fetchMatchingUtxos()
            .sendMessage({hashes: hashes})
            .then(result => {
                return result
            });
    }

    mineBlock(walletClient) {

        if (!walletClient) {
            return this.mineBlockWithoutWallet();
        }
        let currHeight;
        let block;
        return this.client.getTipInfo()
            .sendMessage({})
            .then(tip => {
                currHeight = parseInt(tip.metadata.height_of_longest_chain);
                return this.client.getNewBlockTemplate()
                    .sendMessage({pow_algo: 2});
            })
            .then(template => {
                block = template.new_block_template;
                return walletClient.getCoinbase()
                    .sendMessage({
                        "reward": template.minerData.reward,
                        "fee": template.minerData.total_fees,
                        "height": block.header.height
                    });
            }).then(coinbase => {
                    const cb = coinbase.transaction;
                    block.body.outputs = block.body.outputs.concat(cb.body.outputs);
                    block.body.kernels = block.body.kernels.concat(cb.body.kernels);
                    return this.client.getNewBlock().sendMessage(block);
                }
            ).then(b => {
                    return this.client.submitBlock().sendMessage(b.block);
                }
            ).then(empty => {
                    return this.client.getTipInfo().sendMessage({});
                }
            ).then(tipInfo => {
                expect(tipInfo.metadata.height_of_longest_chain).to.equal((currHeight + 1) + "");
            });
    }

    async getMinedCandidateBlock(existingBlockTemplate) {
        let builder = new TransactionBuilder();
        let blockTemplate = existingBlockTemplate || await this.getBlockTemplate();
        const privateKey = Buffer.from(toLittleEndian(blockTemplate.block.header.height, 256)).toString('hex');
        let cb = builder.generateCoinbase(blockTemplate.minerData.reward, privateKey, blockTemplate.minerData.total_fees, parseInt(blockTemplate.block.header.height) + 1);
        let template = blockTemplate.block;
        template.body.outputs = template.body.outputs.concat(cb.outputs);
        template.body.kernels = template.body.kernels.concat(cb.kernels);
        return {
            template: template, coinbase: {
                output: cb.outputs[0],
                privateKey: privateKey,
                amount: parseInt(blockTemplate.minerData.reward) + parseInt(blockTemplate.minerData.total_fees)
            }
        };
    }

    async mineBlockWithoutWallet(beforeSubmit, onError) {
        let template = await this.getMinedCandidateBlock();
        return this.submitTemplate(template, beforeSubmit).then(async () => {
            let tip = await this.getTipHeight();
            console.log("Node is at tip:", tip);
        }, err => {
            console.log("err submitting block:", err);
            if (onError) {
                if (!onError(err)) {
                    throw err;
                }
                // handled
            } else {
                throw err;
            }
        });
    }

    getSha3Difficulty(header) {
        const hash = new SHA3(256);
        hash.update(toLittleEndian(header.version, 16));
        hash.update(toLittleEndian(parseInt(header.height), 64));
        hash.update(header.prev_hash);
        let timestamp = parseInt(header.timestamp.seconds);
        hash.update(toLittleEndian(timestamp, 64));
        hash.update(header.output_mr);
        hash.update(header.range_proof_mr);
        hash.update(header.kernel_mr);
        hash.update(header.total_kernel_offset);
        hash.update(toLittleEndian(parseInt(header.nonce), 64));
        hash.update(toLittleEndian(header.pow.pow_algo));
        hash.update(toLittleEndian(parseInt(header.pow.accumulated_monero_difficulty), 64));
        hash.update(toLittleEndian(parseInt(header.pow.accumulated_blake_difficulty), 64));
        hash.update(header.pow.pow_data);
        let first_round = hash.digest();
        let hash2 = new SHA3(256);
        hash2.update(first_round);
        let res = hash2.digest('hex');
        return res;
    }
}


module.exports = BaseNodeClient;
