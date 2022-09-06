// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

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
    this.name = `Miner-${name}`;
    this.maxBlocks = 1;
    this.mineTillHeight = 1000000;
    this.walletAddress = walletAddress;
    this.baseNodeAddress = baseNodeAddress;
    this.minDiff = 0;
    this.maxDiff = 100000;
    this.baseNodeClient = baseNodeClient;
    this.logFilePath = logFilePath ? path.resolve(logFilePath) : logFilePath;
    this.mineOnTipOnly = mineOnTipOnly;
    this.numMiningThreads = 4;
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

    // Can't use the || shortcut here because `false` is a valid value
    this.mineOnTipOnly =
      mineOnTipOnly === undefined || mineOnTipOnly === null
        ? this.mineOnTipOnly
        : mineOnTipOnly;
    this.numMiningThreads = numMiningThreads || this.numMiningThreads;
    if (!fs.existsSync(this.baseDir)) {
      fs.mkdirSync(this.baseDir + "/log", { recursive: true });
    }
  }

  run(cmd, args) {
    return new Promise((resolve, reject) => {
      const envs = createEnv({
        walletGrpcAddress: this.walletAddress,
        baseNodeGrpcAddress: this.baseNodeAddress,
        options: {
          mineOnTipOnly: this.mineOnTipOnly,
          numMiningThreads: this.numMiningThreads,
        },
      });
      Object.keys(envs).forEach((k) => {
        args.push("-p");
        args.push(`${k}=${envs[k]}`);
      });

      const ps = spawn(cmd, args, {
        cwd: this.baseDir,
        // shell: true,
        env: { ...process.env },
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

  runCommand(cmd, args, opts = { env: {} }) {
    return new Promise((resolve, reject) => {
      const ps = spawn(cmd, args, {
        cwd: this.baseDir,
        // shell: true,
        env: { ...process.env, ...opts.env },
      });

      ps.stdout.on("data", (data) => {
        // console.log(`stdout: ${data}`);
        fs.appendFileSync(`${this.baseDir}/log/stdout.log`, data.toString());
        resolve(ps);
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
    });
  }

  async startNew() {
    await this.init();
    const args = [
      "--base-path",
      ".",
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
      await this.runCommand("cargo", [
        "build",
        "--release",
        "--locked",
        "--bin",
        "tari_miner",
        "-Z",
        "unstable-options",
        "--out-dir",
        __dirname + "/../temp/out",
      ]);
      outputProcess = __dirname + "/../temp/out/tari_miner";
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

  async mineBlocksUntilHeightIncreasedBy(
    numBlocks,
    minDifficulty,
    mineOnTipOnly
  ) {
    const height =
      parseInt(await this.baseNodeClient.getTipHeight()) + parseInt(numBlocks);
    await this.init(
      numBlocks,
      height,
      minDifficulty,
      9999999999,
      mineOnTipOnly,
      1
    );
    await this.startNew();
    await this.stop();
    const tipHeight = await this.baseNodeClient.getTipHeight();
    console.log(`[${this.name}] Tip at ${tipHeight}`);
    return tipHeight;
  }
}

module.exports = MiningNodeProcess;
