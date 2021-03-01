const {getFreePort} = require("./util");
const dateFormat = require('dateformat');
const fs = require('fs');
const {spawnSync, spawn, execSync} = require('child_process');
const {expect} = require('chai');
const {createEnv} = require("./config");
const WalletClient = require('./walletClient');

let outputProcess;

class WalletProcess {

    constructor(name, options) {
        this.name = name;
        this.options = options;
    }

    async init() {
        this.port = await getFreePort(19000, 25000);
        this.name = `Wallet${this.port}-${this.name}`;
        this.grpcPort = await getFreePort(19000, 25000);
        this.baseDir = `./temp/base_nodes/${dateFormat(new Date(), "yyyymmddHHMM")}/${this.name}`;
    }

    getGrpcAddress() {
        let address = "127.0.0.1:" + this.grpcPort;
        return address;
    }

    getClient() {
        return new WalletClient(this.getGrpcAddress(), this.name);
    }

    setPeerSeeds(addresses) {
        this.peerSeeds = addresses.join(",");
    }

    run(cmd, args, saveFile) {
        return new Promise((resolve, reject) => {
            if (!fs.existsSync(this.baseDir)) {
                fs.mkdirSync(this.baseDir, {recursive: true});
                fs.mkdirSync(this.baseDir + "/log", {recursive: true});
            }

            let envs = createEnv(this.name, true, "cwalletid.json", "127.0.0.1", this.grpcPort, this.port, "127.0.0.1",
                "8080", "8081", "127.0.0.1:8084", this.options, this.peerSeeds)

            var ps = spawn(cmd, args, {
                cwd: this.baseDir,
                shell: true,
                env: {...process.env, ...envs}
            });

            ps.stdout.on('data', (data) => {
                //console.log(`stdout: ${data}`);
                fs.appendFileSync(`${this.baseDir}/log/stdout.log`, data.toString());
                if (data.toString().match(/Starting grpc server/)) {
                    resolve(ps);
                }
            });

            ps.stderr.on('data', (data) => {
                console.error(`stderr: ${data}`);
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
        return await this.run(await this.compile(), ["--base-path", ".", "--init", "--create_id", "--password", "kensentme", "--daemon"], true);
    }

    async compile() {
        if (!outputProcess) {
            await this.run("cargo", ["build", "--release", "--bin", "tari_console_wallet", "-Z", "unstable-options", "--out-dir", __dirname + "/../temp/out"]);
            outputProcess = __dirname + "/../temp/out/tari_console_wallet";
        }
        return outputProcess;
    }

    stop() {
        return new Promise((resolve) => {
            this.ps.on('close', (code) => {
                if (code) {
                    console.log(`child process exited with code ${code}`);
                }
                resolve();
            });
            this.ps.kill("SIGTERM");
        });
    }

}

module.exports = WalletProcess;
