//  Copyright 2021, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

const { exec, spawn } = require("child_process");
const { hideBin } = require("yargs/helpers");
const readline = require("readline");
const path = require("path");

const WEBHOOK_URL_ENV_NAME = "WEBHOOK_URL";

const yargs = () => require("yargs")(hideBin(process.argv));

/**
 * Send a webhook notification to `webhookUrl` or else the WEBHOOK_URL environment var
 * @param channel - the channel to send
 * @param message - the message to send
 * @param webhookUrlOverride - the optional webhook URL to send, if not supplied WEBHOOK_URL is used
 * @returns {Promise<Result<null, ExecException>>}
 */
function sendWebhookNotification(channel, message, webhookUrlOverride = null) {
  const hook = webhookUrlOverride || getWebhookUrlFromEnv();
  if (!hook) {
    return;
  }
  const data = JSON.stringify({ channel, text: message });
  const args = ` -i -X POST -H 'Content-Type: application/json' -d '${data}' ${hook}`;
  return new Promise((resolve, reject) => {
    exec("curl " + args, function (error, stdout, stderr) {
      if (error) {
        return reject(error);
      }
      if (stdout) console.log(stdout);
      if (stderr) console.error(stderr);
      resolve(null);
    });
  });
}

function getWebhookUrlFromEnv() {
  return process.env[WEBHOOK_URL_ENV_NAME];
}

function readLastNLines(file, n) {
  const fs = require("fs");
  return new Promise((resolve, reject) => {
    try {
      const stream = fs.createReadStream(file, {});
      let lineBuf = new Array(n);
      let s = readline.createInterface({ input: stream, crlfDelay: Infinity });
      s.on("line", (line) => {
        if (lineBuf.length + 1 > n) {
          lineBuf.shift();
        }
        lineBuf.push(line);
      });
      s.on("close", () => {
        resolve(lineBuf.filter((l) => l.trim().length > 0));
      });
      s.on("error", reject);
    } catch (err) {
      console.error(err);
      reject(err);
    }
  });
}

async function emptyFile(file) {
  const fs = require("fs").promises;
  await fs.mkdir(path.dirname(file), { recursive: true });
  try {
    await fs.truncate(file, 0);
  } catch (_e) {
    await fs.writeFile(file, "");
  }
}

async function monitorProcessOutput({
  process,
  onData,
  outputStream,
  noStdout,
}) {
  return new Promise((resolve, reject) => {
    let isResolved = false;
    process.stderr.on("data", (buf) => {
      let data = buf.toString().trim();
      if (!noStdout) {
        console.error(data);
      }
      if (outputStream) {
        outputStream.write(data);
      }
    });

    process.stdout.on("data", (buf) => {
      let data = buf.toString();
      if (!noStdout) {
        console.log(data);
      }
      if (outputStream) {
        outputStream.write(data);
      }
      if (isResolved) {
        return;
      }
      try {
        let ret = onData(data);
        if (ret === null || ret === undefined) {
          // No result yet
          return;
        }

        isResolved = true;
        resolve(ret);
      } catch (err) {
        isResolved = true;
        reject(err);
      }
    });

    process.once("error", (err) => {
      isResolved = true;
      reject(err);
    });
    process.once("exit", (code) => {
      isResolved = true;
      if (code === 0) {
        // If already resolved, this is a noop
        resolve(null);
      } else {
        reject(new Error(`Exited with error code ${code}`));
      }
    });
  });
}

const git = {
  command(params, cwd = null) {
    cwd = cwd || process.cwd();
    return new Promise((resolve, reject) => {
      let ps = spawn("git", params, { cwd });
      ps.stdout.on("data", (buf) => {
        console.log(buf.toString());
      });
      ps.stderr.on("data", (buf) => {
        console.error(buf.toString());
      });
      ps.on("close", (exitCode) => {
        if (exitCode && exitCode !== 0) {
          reject(`child process exited with code ${exitCode}`);
        } else {
          resolve(null);
        }
      });
    });
  },
  reset: (cwd = null) => git.command(["reset", "--hard"], cwd),
  pull: (cwd = null) => git.command(["pull", "--rebase"], cwd),
};

module.exports = {
  emptyFile,
  getWebhookUrl: getWebhookUrlFromEnv,
  git,
  monitorProcessOutput,
  readLastNLines,
  sendWebhookNotification,
  yargs,
};
