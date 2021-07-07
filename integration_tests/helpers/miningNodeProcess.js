const dateFormat = require("dateformat");
const fs = require("fs");
const path = require("path");
const { spawn } = require("child_process");
const { expect } = require("chai");
const { createEnv } = require("./config");

let outputProcess;

class MiningNodeProcess {
  constructor(
    name,
    baseNodeAddress,
    baseNodeClient,
    walletAddress,
    logFilePath,
    mineOnTipOnly = true
  ) {
    this.name = `MiningNode-${name}`;
    this.maxBlocks = 1;
    this.mineTillHeight = 1000000;
    this.minDiff = 0;
    this.maxDiff = 100000;
    this.nodeAddress = baseNodeAddress.split(":")[0];
    this.nodeGrpcPort = baseNodeAddress.split(":")[1];
    this.baseNodeClient = baseNodeClient;
    this.walletAddress = walletAddress.split(":")[0];
    this.walletGrpcPort = walletAddress.split(":")[1];
    this.logFilePath = logFilePath ? path.resolve(logFilePath) : logFilePath;
    this.mineOnTipOnly = mineOnTipOnly;
    this.numMiningThreads = 1;
  }

  async init(
    maxBlocks,
    mineTillHeight,
    minDiff,
    maxDiff,
    mineOnTipOnly,
    numMiningThreads
  ) {
    this.maxBlocks = maxBlocks || this.maxBlocks;
    this.mineTillHeight = mineTillHeight || this.mineTillHeight;
    this.minDiff = minDiff || this.minDiff;
    this.maxDiff = Math.max(maxDiff || this.maxDiff, this.minDiff);
    this.baseDir = `./temp/base_nodes/${dateFormat(
      new Date(),
      "yyyymmddHHMM"
    )}/${this.name}`;
    this.mineOnTipOnly = mineOnTipOnly || this.mineOnTipOnly;
    this.numMiningThreads = numMiningThreads || this.numMiningThreads;
  }

  run(cmd, args) {
    return new Promise((resolve, reject) => {
      if (!fs.existsSync(this.baseDir)) {
        fs.mkdirSync(this.baseDir, { recursive: true });
        fs.mkdirSync(this.baseDir + "/log", { recursive: true });
      }

      const envs = createEnv(
        this.name,
        false,
        "nodeid.json",
        this.walletAddress,
        this.walletGrpcPort,
        "8080",
        this.nodeAddress,
        this.nodeGrpcPort,
        this.baseNodePort,
        "127.0.0.1:8084",
        {
          mineOnTipOnly: this.mineOnTipOnly,
          numMiningThreads: this.numMiningThreads,
        },
        []
      );

      const ps = spawn(cmd, args, {
        cwd: this.baseDir,
        // shell: true,
        env: { ...process.env, ...envs },
      });

      ps.stdout.on("data", (data) => {
        // console.log(`stdout: ${data}`);
        fs.appendFileSync(`${this.baseDir}/log/stdout.log`, data.toString());
        // resolve(ps);
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
    const args = [
      "--base-path",
      ".",
      "--init",
      "--daemon",
      "--max-blocks",
      this.maxBlocks,
      "--mine-until-height",
      this.mineTillHeight,
      "--min-difficulty",
      this.minDiff,
      "--max-difficulty",
      this.maxDiff,
    ];
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
        "tari_mining_node",
        "-Z",
        "unstable-options",
        "--out-dir",
        __dirname + "/../temp/out",
      ]);
      outputProcess = __dirname + "/../temp/out/tari_mining_node";
    }
    return outputProcess;
  }

  stop() {
    return new Promise((resolve) => {
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

  async mineBlocksUntilHeightIncreasedBy(numBlocks, minDifficulty) {
    const height =
      parseInt(await this.baseNodeClient.getTipHeight()) + parseInt(numBlocks);
    await this.init(numBlocks, height, minDifficulty, 9999999999, true, 1);
    await this.startNew();
    await this.stop();
    return await this.baseNodeClient.getTipHeight();
  }
}

module.exports = MiningNodeProcess;
