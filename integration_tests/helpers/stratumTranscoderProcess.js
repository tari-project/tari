const { getFreePort } = require("./util");
const dateFormat = require("dateformat");
const fs = require("fs");
const path = require("path");
const { spawn } = require("child_process");
const { expect } = require("chai");
const StratumTranscoderClient = require("./stratumTranscoderClient");
const { createEnv } = require("./config");

let outputProcess;

class StratumTranscoderProcess {
  constructor(name, baseNodeAddress, walletAddress, logFilePath) {
    this.name = name;
    this.baseNodeAddress = baseNodeAddress;
    this.walletAddress = walletAddress;
    this.logFilePath = logFilePath ? path.resolve(logFilePath) : logFilePath;
    this.client = null;
  }

  async init() {
    this.port = await getFreePort();
    this.name = `StratumTranscoder${this.port}-${this.name}`;
    this.baseDir = `./temp/base_nodes/${dateFormat(
      new Date(),
      "yyyymmddHHMM"
    )}/${this.name}`;
  }

  run(cmd, args) {
    return new Promise((resolve, reject) => {
      if (!fs.existsSync(this.baseDir)) {
        fs.mkdirSync(this.baseDir, { recursive: true });
        fs.mkdirSync(this.baseDir + "/log", { recursive: true });
      }

      const transcoderFullAddress = "127.0.0.1:" + this.port;

      const envs = createEnv({
        walletGrpcAddress: this.walletAddress,
        baseNodeAddress: this.baseNodeAddress,
        transcoderFullAddress,
      });

      const ps = spawn(cmd, args, {
        cwd: this.baseDir,
        // shell: true,
        env: { ...process.env, ...envs },
      });

      ps.stdout.on("data", (data) => {
        // console.log(`stdout: ${data}`);
        fs.appendFileSync(`${this.baseDir}/log/stdout.log`, data.toString());
        if (data.toString().match(/Listening on/)) {
          resolve(ps);
        }
      });

      ps.stderr.on("data", (data) => {
        console.error(`stderr: ${data}`);
        fs.appendFileSync(`${this.baseDir}/log/stderr.log`, data.toString());
      });

      ps.on("close", (code) => {
        const ps = this.ps;
        this.ps = null;
        if (code) {
          console.log(`child process exited with code ${code}`);
          reject(`child process exited with code ${code}`);
        } else {
          resolve(ps);
        }
      });

      expect(ps.error).to.be.an("undefined");
      this.ps = ps;
    });
  }

  async startNew() {
    await this.init();
    const args = ["--base-path", ".", "--init"];
    if (this.logFilePath) {
      args.push("--log-config", this.logFilePath);
    }
    return await this.run(await this.compile(), args, true);
  }

  async compile() {
    if (!outputProcess) {
      await this.run("cargo", [
        "build",
        "--release",
        "--bin",
        "tari_stratum_transcoder",
        "-Z",
        "unstable-options",
        "--out-dir",
        __dirname + "/../temp/out",
      ]);
      outputProcess = __dirname + "/../temp/out/tari_stratum_transcoder";
    }
    return outputProcess;
  }

  stop() {
    return new Promise((resolve) => {
      this.client = null;
      if (!this.ps) {
        return resolve();
      }
      this.ps.on("close", (code) => {
        if (code) {
          console.log(`child process exited with code ${code}`);
        }
        resolve();
      });
      this.ps.kill("SIGINT");
    });
  }

  getClient() {
    if (this.client) {
      return this.client;
    } else {
      this.client = this.createClient();
      return this.client;
    }
  }

  createClient() {
    const address = "http://127.0.0.1:" + this.port;
    return new StratumTranscoderClient(
      address,
      this.baseNodeAddress,
      this.walletAddress
    );
  }
}

module.exports = StratumTranscoderProcess;
