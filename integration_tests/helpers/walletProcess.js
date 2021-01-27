const {getFreePort} = require("./util");
const dateFormat = require('dateformat');
const fs = require('fs');
const {spawnSync, spawn, execSync} = require('child_process');
const {expect} = require('chai');
const WalletClient = require('./walletClient')
;
class WalletProcess {

    constructor(name) {
        this.name = name;
    }

    async init() {
        this.port = await getFreePort(19000, 25000);
        this.name = `Wallet${this.port}-${this.name}`;
        this.nodeFile = "cwalletid.json";
        this.baseDir = `./temp/base_nodes/${dateFormat(new Date(), "yyyymmddHHMM")}/${this.name}`;
           await this.run("cargo",
                 ["run", "--release", "--bin", "tari_console_wallet", "--", "--base-path", ".", "--create-id", "--init"]);
    }

  ensureNodeInfo() {
        while (true) {
            if (fs.existsSync(this.baseDir + "/" + this.nodeFile)) {
                break;
            }
        }

        this.nodeInfo = JSON.parse(fs.readFileSync(this.baseDir + "/" + this.nodeFile, 'utf8'));

    }

    getPubKey() {
        this.ensureNodeInfo();
       return  this.nodeInfo["public_key"];
    }


    getGrpcAddress() {
        return "127.0.0.1:" + this.port;
    }

    getClient() {
     return new WalletClient(this.getGrpcAddress());
    }

    setPeerSeeds(addresses) {
        this.peerSeeds = addresses.join(",");
    }

    createEnvs() {
        let envs = {
            RUST_BACKTRACE: 1,
            TARI_BASE_NODE__NETWORK: "localnet",
            TARI_BASE_NODE__LOCALNET__GRPC_BASE_NODE_ADDRESS: "127.0.0.1:1",
            TARI_BASE_NODE__LOCALNET__GRPC_CONSOLE_WALLET_ADDRESS: `127.0.0.1:${this.port}`,
            // Defaults:
            TARI_BASE_NODE__LOCALNET__DATA_DIR: "localnet",
            TARI_BASE_NODE__LOCALNET__DB_TYPE: "lmdb",
            TARI_BASE_NODE__LOCALNET__ORPHAN_STORAGE_CAPACITY: "10",
            TARI_BASE_NODE__LOCALNET__PRUNING_HORIZON: "0",
            TARI_BASE_NODE__LOCALNET__PRUNED_MODE_CLEANUP_INTERVAL: "10000",
            TARI_BASE_NODE__LOCALNET__CORE_THREADS: "10",
            TARI_BASE_NODE__LOCALNET__MAX_THREADS: "512",
            TARI_BASE_NODE__LOCALNET__IDENTITY_FILE: "none.json",
            TARI_BASE_NODE__LOCALNET__BASE_NODE_IDENTITY_FILE: "none.json",
            TARI_BASE_NODE__LOCALNET__BASE_NODE_TOR_IDENTITY_FILE: "node_tor_id.json",
            TARI_BASE_NODE__LOCALNET__WALLET_IDENTITY_FILE: "walletid.json",
            TARI_BASE_NODE__LOCALNET__CONSOLE_WALLET_IDENTITY_FILE: "cwalletid.json",
            TARI_BASE_NODE__LOCALNET__WALLET_TOR_IDENTITY_FILE: "wallet_tor_id.json",
            TARI_BASE_NODE__LOCALNET__CONSOLE_WALLET_TOR_IDENTITY_FILE: "wallet_tor_id.json",
            TARI_BASE_NODE__LOCALNET__TRANSPORT: "tcp",
            TARI_BASE_NODE__LOCALNET__TCP_LISTENER_ADDRESS: "/ip4/0.0.0.0/tcp/" + this.port,
            TARI_BASE_NODE__LOCALNET__ALLOW_TEST_ADDRESSES: true,
            TARI_BASE_NODE__LOCALNET__PUBLIC_ADDRESS: "/ip4/127.0.0.1/tcp/" + this.port,
            TARI_BASE_NODE__LOCALNET__GRPC_ENABLED: "true",
            TARI_BASE_NODE__LOCALNET__ENABLE_WALLET: false,
            TARI_BASE_NODE__LOCALNET__DNS_SEEDS_NAME_SERVER: "1.1.1.1:53",
            TARI_BASE_NODE__LOCALNET__DNS_SEEDS_USE_DNSSEC: "true",
            TARI_BASE_NODE__LOCALNET__BLOCK_SYNC_STRATEGY: "ViaBestChainMetadata",
            TARI_BASE_NODE__LOCALNET__ENABLE_MINING: "false",
            TARI_BASE_NODE__LOCALNET__NUM_MINING_THREADS: "1",
            TARI_BASE_NODE__LOCALNET__ORPHAN_DB_CLEAN_OUT_THRESHOLD: "0",
            TARI_MERGE_MINING_PROXY__LOCALNET__MONEROD_URL: "aasdf",
            TARI_MERGE_MINING_PROXY__LOCALNET__MONEROD_USE_AUTH: "false",
            TARI_MERGE_MINING_PROXY__LOCALNET__MONEROD_USERNAME: "asdf",
            TARI_MERGE_MINING_PROXY__LOCALNET__MONEROD_PASSWORD: "asdf",
            TARI_MERGE_MINING_PROXY__LOCALNET__PROXY_HOST_ADDRESS: "127.0.0.1:30071",
            TARI_BASE_NODE__LOCALNET__DB_INIT_SIZE_MB: 100,
            TARI_BASE_NODE__LOCALNET__DB_RESIZE_THRESHOLD_MB: 10,
            TARI_BASE_NODE__LOCALNET__DB_GROW_SIZE_MB: 20,
            TARI_MERGE_MINING_PROXY__LOCALNET__WAIT_FOR_INITIAL_SYNC_AT_STARTUP: false
        }
        if (this.peerSeeds) {
            envs.TARI_BASE_NODE__LOCALNET__PEER_SEEDS = this.peerSeeds;
        }else {
            //  Nowheresville
            envs.TARI_BASE_NODE__LOCALNET__PEER_SEEDS = ["5cfcf17f41b01980eb4fa03cec5ea12edbd3783496a2b5dabf99e4bf6410f460::/ip4/10.0.0.50/tcp/1"]

        }
        return envs;
    }

    run(cmd, args) {
        return new Promise((resolve, reject) => {
            if (!fs.existsSync(this.baseDir)) {
                fs.mkdirSync(this.baseDir, {recursive: true});
                fs.mkdirSync(this.baseDir + "/log", {recursive: true});
            }
            var ps = spawn(cmd, args, {
                cwd: this.baseDir,
                shell: true,
                env: {...process.env, ...this.createEnvs()}
            });

            ps.stdout.on('data', (data) => {
                //console.log(`stdout: ${data}`);
                fs.appendFileSync(`${this.baseDir}/log/stdout.log`, data.toString());
                if (data.toString().match(/Starting grpc server/)) {
                    resolve(ps);
                }
            });

            ps.stderr.on('data', (data) => {
                // console.error(`stderr: ${data}`);
                fs.appendFileSync(`${this.baseDir}/log/stderr.log`, data.toString());
            });

            ps.on('close', (code) => {
                if (code) {
                    console.log(`child process exited with code ${code}`);
                    reject(`child process exited with code ${code}`);
                } else {
                    resolve(ps);
                }
            });

            expect(ps.error).to.be.an('undefined');
            this.ps = ps;
        });
    }

    async startNew() {
        await this.init();
        return this.run("cargo", ["run", "--release", "--bin tari_console_wallet", "--", "--base-path", ".", "--password", "kensentme", "--daemon"]);
    }

    stop() {
        this.ps.kill("SIGINT");
    }

}

module.exports = WalletProcess;
