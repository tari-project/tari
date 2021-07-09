const { getFreePort } = require("./util");
const dateFormat = require("dateformat");
const fs = require("fs");
const path = require("path");
const { spawn } = require("child_process");
const { expect } = require("chai");
const { createEnv } = require("./config");
const WalletClient = require("./walletClient");
const csvParser = require("csv-parser");

let outputProcess;

class WalletProcess {
  constructor(name, excludeTestEnvars, options, logFilePath, seedWords) {
    this.name = name;
    this.options = options;
    this.logFilePath = logFilePath ? path.resolve(logFilePath) : logFilePath;
    this.recoverWallet = !!seedWords;
    this.seedWords = seedWords;
    this.excludeTestEnvars = excludeTestEnvars;
  }

  async init() {
    this.port = await getFreePort(19000, 25000);
    this.name = `Wallet${this.port}-${this.name}`;
    this.grpcPort = await getFreePort(19000, 25000);
    this.baseDir = `./temp/base_nodes/${dateFormat(
      new Date(),
      "yyyymmddHHMM"
    )}/${this.name}`;
    this.seedWordsFile = path.resolve(this.baseDir + "/config/seed_words.log");
  }

  getGrpcAddress() {
    return "127.0.0.1:" + this.grpcPort;
  }

  getClient() {
    return new WalletClient(this.getGrpcAddress(), this.name);
  }

  getSeedWords() {
    try {
      return fs.readFileSync(this.seedWordsFile, "utf8");
    } catch (err) {
      console.error("\n", this.name, ": Seed words file not found!\n", err);
    }
  }

  setPeerSeeds(addresses) {
    this.peerSeeds = addresses.join(",");
  }

  run(cmd, args, saveFile) {
    return new Promise((resolve, reject) => {
      if (!fs.existsSync(this.baseDir)) {
        fs.mkdirSync(this.baseDir, { recursive: true });
        fs.mkdirSync(this.baseDir + "/log", { recursive: true });
      }

      let envs = {};
      if (!this.excludeTestEnvars) {
        envs = createEnv(
          this.name,
          true,
          "cwalletid.json",
          "127.0.0.1",
          this.grpcPort,
          this.port,
          "127.0.0.1",
          "8080",
          "8081",
          "127.0.0.1:8084",
          this.options,
          this.peerSeeds
        );
      } else if (this.options["grpc_console_wallet_address"]) {
        const network =
          this.options && this.options.network
            ? this.options.network.toUpperCase()
            : "LOCALNET";

        envs[`TARI_BASE_NODE__${network}__GRPC_CONSOLE_WALLET_ADDRESS`] =
          this.options["grpc_console_wallet_address"];
      }

      if (saveFile) {
        fs.appendFileSync(`${this.baseDir}/.env`, JSON.stringify(envs));
      }

      const ps = spawn(cmd, args, {
        cwd: this.baseDir,
        // shell: true,
        env: { ...process.env, ...envs },
      });

      ps.stdout.on("data", (data) => {
        //console.log(`stdout: ${data}`);
        fs.appendFileSync(`${this.baseDir}/log/stdout.log`, data.toString());
        if (
          (!this.recoverWallet &&
            data.toString().match(/Starting grpc server/)) ||
          (this.recoverWallet &&
            data.toString().match(/Initializing logging according/))
        ) {
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
    return await this.start();
  }

  async compile() {
    if (!outputProcess) {
      await this.run("cargo", [
        "build",
        "--release",
        "--bin",
        "tari_console_wallet",
        "-Z",
        "unstable-options",
        "--out-dir",
        process.cwd() + "/temp/out",
      ]);
      outputProcess = process.cwd() + "/temp/out/tari_console_wallet";
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

  async start() {
    const args = [
      "--base-path",
      ".",
      "--init",
      "--create_id",
      "--password",
      "kensentme",
      "--seed-words-file-name",
      this.seedWordsFile,
      "--daemon",
    ];
    if (this.recoverWallet) {
      args.push("--recover", "--seed-words", this.seedWords);
    }
    if (this.logFilePath) {
      args.push("--log-config", this.logFilePath);
    }
    return await this.run(await this.compile(), args, true);
  }

  async exportSpentOutputs() {
    const args = [
      "--init",
      "--base-path",
      ".",
      "--auto-exit",
      "--password",
      "kensentme",
      "--command",
      "export-spent-utxos --csv-file exported_outputs.csv",
    ];
    outputProcess = __dirname + "/../temp/out/tari_console_wallet";
    await this.run(outputProcess, args, true);
  }

  async exportUnspentOutputs() {
    const args = [
      "--init",
      "--base-path",
      ".",
      "--auto-exit",
      "--password",
      "kensentme",
      "--command",
      "export-utxos --csv-file exported_outputs.csv",
    ];
    outputProcess = __dirname + "/../temp/out/tari_console_wallet";
    await this.run(outputProcess, args, true);
  }

  async readExportedOutputs() {
    const filePath = path.resolve(this.baseDir + "/exported_outputs.csv");
    expect(fs.existsSync(filePath)).to.equal(
      true,
      "outputs export csv must exist"
    );

    let unblinded_outputs = await new Promise((resolve) => {
      let unblinded_outputs = [];
      fs.createReadStream(filePath)
        .pipe(csvParser())
        .on("data", (row) => {
          let unblinded_output = {
            value: parseInt(row.value),
            spending_key: Buffer.from(row.spending_key, "hex"),
            features: {
              flags: 0,
              maturity: parseInt(row.maturity) || 0,
            },
            script: Buffer.from(row.script, "hex"),
            input_data: Buffer.from(row.input_data, "hex"),
            script_private_key: Buffer.from(row.script_private_key, "hex"),
            sender_offset_public_key: Buffer.from(
              row.sender_offset_public_key,
              "hex"
            ),
            metadata_signature: {
              public_nonce_commitment: Buffer.from(row.public_nonce, "hex"),
              signature_u: Buffer.from(row.signature_u, "hex"),
              signature_v: Buffer.from(row.signature_v, "hex"),
            },
          };
          unblinded_outputs.push(unblinded_output);
        })
        .on("end", () => {
          resolve(unblinded_outputs);
        });
    });

    return unblinded_outputs;
  }
}

module.exports = WalletProcess;
