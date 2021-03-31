const {getFreePort} = require("./util");
const dateFormat = require('dateformat');
const fs = require('fs');
const {spawnSync, spawn, execSync} = require('child_process');
const {expect} = require('chai');
const {createEnv} = require("./config");

let outputProcess;

class MiningNodeProcess {

    constructor(name, baseNodeAddress, walletAddress) {
        this.name = name;
        this.maxBlocks = 1;
        this.minDiff = 0;
        this.maxDiff = 100000;
        this.nodeAddress = baseNodeAddress.split(":")[0];
        this.nodeGrpcPort = baseNodeAddress.split(":")[1];
        this.walletAddress = walletAddress.split(":")[0];
        this.walletGrpcPort = walletAddress.split(":")[1];
    }

    async init(maxBlocks, minDiff, maxDiff) {
        this.maxBlocks = maxBlocks || this.maxBlocks;
        this.minDiff = minDiff || this.minDiff;
        this.maxDiff = maxDiff || this.maxDiff;
        this.baseDir = `./temp/base_nodes/${dateFormat(new Date(), "yyyymmddHHMM")}/${this.name}`;
    }

    run(cmd, args, saveFile) {
        return new Promise((resolve, reject) => {
            if (!fs.existsSync(this.baseDir)) {
                fs.mkdirSync(this.baseDir, {recursive: true});
                fs.mkdirSync(this.baseDir + "/log", {recursive: true});
            }

            let envs = createEnv(this.name, false, "nodeid.json", this.walletAddress, this.walletGrpcPort, "8080", this.nodeAddress, this.nodeGrpcPort, this.baseNodePort,"127.0.0.1:8084", [], []);

            var ps = spawn(cmd, args, {
                cwd: this.baseDir,
                // shell: true,
                env: {...process.env, ...envs}
            });

            ps.stdout.on('data', (data) => {
                //console.log(`stdout: ${data}`);
                fs.appendFileSync(`${this.baseDir}/log/stdout.log`, data.toString());
                resolve(ps);
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
        return await this.run(await this.compile(), ["--base-path", ".", "--init", "--daemon", "--max-blocks", this.maxBlocks, "--min-difficulty", this.minDiff, "--max-difficulty", this.maxDiff], true);
    }

    async compile() {
        if (!outputProcess) {
            await this.run("cargo", ["build", "--release", "--bin", "tari_mining_node", "-Z", "unstable-options", "--out-dir", __dirname + "/../temp/out"]);
            outputProcess = __dirname + "/../temp/out/tari_mining_node";
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

}

module.exports = MiningNodeProcess;
