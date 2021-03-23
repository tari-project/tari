const {getFreePort} = require("./util");
const dateFormat = require('dateformat');
const fs = require('fs');
const {spawnSync, spawn, execSync} = require('child_process');
const {expect} = require('chai');
const MergeMiningProxyClient = require('./mergeMiningProxyClient');
const {createEnv} = require("./config");

let outputProcess;

class MergeMiningProxyProcess {
    constructor(name, baseNodeAddress, walletAddress, submitOrigin=true) {
        this.name = name;
        this.nodeAddress = baseNodeAddress.split(":")[0];
        this.nodeGrpcPort = baseNodeAddress.split(":")[1];
        this.walletAddress = walletAddress.split(":")[0];
        this.walletGrpcPort = walletAddress.split(":")[1];
        this.submitOrigin = submitOrigin
    }

    async init() {
        this.port = await getFreePort(19000, 25000);
        this.name = `MMProxy${this.port}-${this.name}`;
        this.baseDir = `./temp/base_nodes/${dateFormat(new Date(), "yyyymmddHHMM")}/${this.name}`;
        //console.log("MergeMiningProxyProcess init - assign server GRPC:", this.grpcPort);
    }

    run(cmd, args, saveFile) {
        return new Promise((resolve, reject) => {
            if (!fs.existsSync(this.baseDir)) {
                fs.mkdirSync(this.baseDir, {recursive: true});
                fs.mkdirSync(this.baseDir + "/log", {recursive: true});
            }

            let proxyAddress = "127.0.0.1:" + this.port;
            let envs = createEnv(this.name, false, "nodeid.json", this.walletAddress, this.walletGrpcPort, this.port, this.nodeAddress, this.nodeGrpcPort, this.baseNodePort, proxyAddress, [], []);
            var extraEnvs = {
            TARI_MERGE_MINING_PROXY__LOCALNET__PROXY_SUBMIT_TO_ORIGIN: this.submitOrigin,
            }
            let completeEnvs = {...envs,...extraEnvs};
            var ps = spawn(cmd, args, {
                cwd: this.baseDir,
                // shell: true,
                env: {...process.env, ...completeEnvs}
            });

            ps.stdout.on('data', (data) => {
                //console.log(`stdout: ${data}`);
                fs.appendFileSync(`${this.baseDir}/log/stdout.log`, data.toString());
                if (data.toString().match(/Listening on/)) {
                    resolve(ps);
                }
            });

            ps.stderr.on('data', (data) => {
                console.error(`stderr: ${data}`);
                fs.appendFileSync(`${this.baseDir}/log/stderr.log`, data.toString());
            });

            ps.on('close', (code) => {
                let ps = this.ps;
                this.ps = null;
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
        return await this.run(await this.compile(), ["--base-path", ".", "--init"], true);
    }

    async compile() {
        if (!outputProcess) {
            await this.run("cargo", ["build", "--release", "--bin", "tari_merge_mining_proxy", "-Z", "unstable-options", "--out-dir", __dirname + "/../temp/out"]);
            outputProcess = __dirname + "/../temp/out/tari_merge_mining_proxy";
        }
        return outputProcess;
    }

    stop() {
        return new Promise((resolve) => {
            if (!this.ps) {
                return resolve();
            }
            this.ps.on('close', (code) => {
                if (code) {
                    console.log(`child process exited with code ${code}`);
                }
                resolve();
            });
            this.ps.kill("SIGINT");
        });
    }

    createClient() {
        let address = "http://127.0.0.1:" + this.port;
        //console.log("MergeMiningProxyProcess createClient - client address:", address);
        return new MergeMiningProxyClient(address);
    }
}

module.exports = MergeMiningProxyProcess;
