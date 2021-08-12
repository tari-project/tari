const { getFreePort } = require("./util");
const dateFormat = require("dateformat");
const fs = require("fs");
const path = require("path");
const { spawn } = require("child_process");
const { expect } = require("chai");
const { createEnv } = require("./config");
const WalletClient = require("./walletClient");
const csvParser = require("csv-parser");
var tari_crypto = require("tari_crypto");

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
    this.port = await getFreePort();
    this.name = `Wallet${this.port}-${this.name}`;
    this.grpcPort = await getFreePort();
    this.baseDir = `./temp/base_nodes/${dateFormat(
      new Date(),
      "yyyymmddHHMM"
    )}/${this.name}`;
    this.seedWordsFile = path.resolve(this.baseDir + "/config/seed_words.log");
  }

  getGrpcAddress() {
    return "127.0.0.1:" + this.grpcPort;
  }

  async connectClient() {
    let client = new WalletClient(this.name);
    await client.connect(this.getGrpcAddress());
    return client;
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

  run(cmd, args, saveFile, input_buffer, output) {
    return new Promise((resolve, reject) => {
      if (!fs.existsSync(this.baseDir)) {
        fs.mkdirSync(this.baseDir, { recursive: true });
        fs.mkdirSync(this.baseDir + "/log", { recursive: true });
      }

      let envs = {};
      const network =
        this.options && this.options.network
          ? this.options.network.toUpperCase()
          : "LOCALNET";
      envs[`TARI_BASE_NODE__COMMON__NETWORK`] = network;
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
          "127.0.0.1:8085",
          this.options,
          this.peerSeeds
        );
      } else if (this.options["grpc_console_wallet_address"]) {
        envs[`TARI_BASE_NODE__${network}__GRPC_CONSOLE_WALLET_ADDRESS`] =
          this.options["grpc_console_wallet_address"];
        this.grpcPort =
          this.options["grpc_console_wallet_address"].split(":")[1];
      }

      if (saveFile) {
        fs.appendFileSync(`${this.baseDir}/.env`, JSON.stringify(envs));
      }

      const ps = spawn(cmd, args, {
        cwd: this.baseDir,
        // shell: true,
        env: { ...process.env, ...envs },
      });

      if (input_buffer) {
        // If we want to simulate user input we can do so here.
        ps.stdin.write(input_buffer);
      }
      ps.stdout.on("data", (data) => {
        //console.log(`stdout: ${data}`);
        if (output !== undefined && output.buffer !== undefined) {
          output.buffer += data;
        }
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
        if (code == 112) {
          reject("Incorrect password");
        } else if (code) {
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

  async start(password) {
    const args = [
      "--base-path",
      ".",
      "--init",
      "--create_id",
      "--password",
      `${password ? password : "kensentme"}`,
      "--seed-words-file-name",
      this.seedWordsFile,
      "--non-interactive",
    ];
    if (this.recoverWallet) {
      args.push("--recover", "--seed-words", this.seedWords);
    }
    if (this.logFilePath) {
      args.push("--log-config", this.logFilePath);
    }
    return await this.run(await this.compile(), args, true);
  }

  async changePassword(oldPassword, newPassword) {
    const args = [
      "--base-path",
      ".",
      "--password",
      oldPassword,
      "--update-password",
    ];
    if (this.logFilePath) {
      args.push("--log-config", this.logFilePath);
    }
    // Set input_buffer to double confirmation of the new password
    return await this.run(
      await this.compile(),
      args,
      true,
      newPassword + "\n" + newPassword + "\n"
    );
  }

  async runCommand(command) {
    // we need to quit the wallet before running a command
    await this.stop();
    const args = [
      "--base-path",
      ".",
      "--password",
      "kensentme",
      "--command",
      command,
      "--non-interactive",
    ];
    if (this.logFilePath) {
      args.push("--log-config", this.logFilePath);
    }
    let output = { buffer: "" };
    // In case we killed the wallet fast send enter. Because it will ask for the logs again (e.g. whois test)
    await this.run(await this.compile(), args, true, "\n", output);
    return output;
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

  // Faucet outputs are only provided with an amount and spending key so we zero out the other output data
  // and update the input data to be the public key of the spending key, make the script private key the spending key
  // and then we can test if this output is still spendable when imported into the wallet.
  async readExportedOutputsAsFaucetOutputs() {
    let outputs = await this.readExportedOutputs();
    for (let i = 0; i < outputs.length; i++) {
      outputs[i].metadata_signature = {
        public_nonce_commitment: Buffer.from(
          "0000000000000000000000000000000000000000000000000000000000000000",
          "hex"
        ),
        signature_u: Buffer.from(
          "0000000000000000000000000000000000000000000000000000000000000000",
          "hex"
        ),
        signature_v: Buffer.from(
          "0000000000000000000000000000000000000000000000000000000000000000",
          "hex"
        ),
      };
      outputs[i].sender_offset_public_key = Buffer.from(
        "0000000000000000000000000000000000000000000000000000000000000000",
        "hex"
      );
      outputs[i].script_private_key = outputs[i].spending_key;
      let scriptPublicKey = tari_crypto.pubkey_from_secret(
        outputs[i].spending_key.toString("hex")
      );
      let input_data = Buffer.concat([
        Buffer.from([0x04]),
        Buffer.from(scriptPublicKey, "hex"),
      ]);
      outputs[i].input_data = input_data;
    }
    return outputs;
  }
}

module.exports = WalletProcess;
