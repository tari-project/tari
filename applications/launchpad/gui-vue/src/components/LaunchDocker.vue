<!-- Copyright 2021. The Tari Project -->
<!-- -->
<!-- Redistribution and use in source and binary forms, with or without modification, are permitted provided that the -->
<!-- following conditions are met: -->
<!-- -->
<!-- 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following -->
<!-- disclaimer. -->
<!-- -->
<!-- 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the -->
<!-- following disclaimer in the documentation and/or other materials provided with the distribution. -->
<!-- -->
<!-- 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote -->
<!-- products derived from this software without specific prior written permission. -->
<!-- -->
<!-- THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, -->
<!-- INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE -->
<!-- DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, -->
<!-- SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR -->
<!-- SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, -->
<!-- WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE -->
<!-- USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE. -->
<!-- -->

<template>
  <h1>Workspace</h1>
  <input v-model="workspaceName" placeholder="MyWorkspace"/>
  <button @click="createWorkspace">Create workspace</button>
  <button @click="openWorkspace">Open workspace</button>
  <p>
    <b>Workspace status:</b><span>{{ workspaceStatus }}</span>
  </p>
  <h2>Identity</h2>
  <div v-for="(id, image) in ids" v-bind:key="image">
    <h3>{{image}}</h3>
  <ul>
    <li><b>Node Id:</b> {{ id.node_id }}</li>
    <li><b>public_key: </b> {{ id.public_key }}</li>
    <li><b>features: </b> {{ id.features }}</li>
    <li><b>secret_key: </b> {{ id.secret_key }}</li>
    <li><b>public_address: </b> {{ id.public_address }}</li>
  </ul>
  </div>
  <hr/>
  <div>
    <h1>Options</h1>
    <div>
      <div id="v-model-select">
        <select v-model="options.tari_network">
          <option disabled value="">Please select one</option>
          <option value="weatherwax">Weatherwax</option>
          <option value="mainnet">Mainnet</option>
        </select>
        <span>Selected: {{ options.tari_network }}</span>
      </div>
      <ul>
        <li>Wait for Tor: <input v-model.number="options.wait_for_tor" placeholder="Value in seconds"/> seconds</li>
        <li>Docker registry: <input v-model="options.docker_registry" placeholder="Docker registry"/></li>
        <li>Docker tag: <input v-model="options.docker_tag" placeholder="Docker tag"/></li>
      </ul>
    </div>

    <div>
      <input type="checkbox" id="base_node" v-model="options.has_base_node"/>
      <label for="base_node">Spin up a Base Node</label>
    </div>

    <div>
      <input type="checkbox" id="wallet" v-model="options.has_wallet"/>
      <label for="wallet">Spin up a wallet</label>
      <div v-if="options.has_wallet">
        <input v-model="options.wallet_password" placeholder="password"/>
      </div>
    </div>


    <div>
      <input type="checkbox" id="sha3_miner" v-model="options.has_sha3_miner"/>
      <label for="sha3_miner">Spin up a SHA3 miner</label>
      <div v-if="options.has_sha3_miner">
        <input v-model.number="options.sha3_mining_threads" placeholder="# SHA3 mining threads"/>
      </div>
    </div>

    <div>
      <input type="checkbox" id="mm_proxy" v-model="options.has_mm_proxy"/>
      <label for="mm_proxy">Spin up a Monero Miner</label>
      <div v-if="options.has_mm_proxy">
        <ul>
          <li><input v-model="options.monerod_url" placeholder="Monerod URL"/></li>
          <li><input v-model="options.monero_mining_address" placeholder="Monero address"/></li>
          <li><input type="checkbox" id="monero_use_auth" v-model="options.monero_use_auth"/>
            <label for="monero_use_auth">Monerod requires Auth URL</label></li>
          <div v-if="options.monero_use_auth">
            <li><input v-model="options.monero_username" placeholder="Monerod username"/></li>
            <li><input v-model="options.monero_password" placeholder="Monerod password"/></li>
          </div>
        </ul>
      </div>
    </div>

    <input type="checkbox" id="xmrig" v-model="options.has_xmrig"/>
    <label for="xmrig">Spin up XMRig</label>
  </div>

  <button @click="launch">Launch!</button>

  <div class="logs">
    <hr/>
    <h2>Logs</h2>
    <table>
      <tr v-for="(log, index) in logs" v-bind:key="index">
        <td>{{ log }}</td>
      </tr>
    </table>
    <hr/>
  </div>
</template>

<script>
import {invoke} from '@tauri-apps/api/tauri'
import {save, open} from '@tauri-apps/api/dialog'
import {listen} from "@tauri-apps/api/event";

const imageNames = [
  "tor",
  "tari_base_node",
  "tari_console_wallet",
  "xmrig",
  "tari_sha3_miner",
  "tari_mm_proxy",
  "monerod"
];

async function getWorkspaceFolder(fn) {
  const options = {
    defaultPath: "/tmp",
    directory: true,
    multiple: false
  }
  try {
    return await fn(options);
  } catch (err) {
    console.log("Error selecting workspace folder.", err);
    throw err(err);
  }
}

async function createWorkspace() {
  try {
    const folder = await getWorkspaceFolder(save);
    this.options.root_folder = folder;
    console.log(`Workspace folder: ${folder}`);
    await invoke("create_new_workspace", {rootPath: folder});
    this.workspaceStatus = `Created successfully (${this.root_folder}).`;
  } catch (err) {
    this.workspaceStatus = `Error: ${err}`;
  }
}

async function openWorkspace() {
  try {
    const folder = await getWorkspaceFolder(open);
    this.options.root_folder = folder;
    console.log(`Workspace folder: ${folder}`);
    this.workspaceStatus = `Workspace loaded (${this.root_folder}).`;
  } catch (err) {
    this.workspaceStatus = `Error: ${err}`;
  }
}

async function launch() {
  try {
    const options = this.options;
    console.log(`Launching docker with`, options);
    for (let name of imageNames) {
      await listen(`tari://docker_log_${name}`, event => {
        this.logs.push(JSON.stringify(event.payload));
      });
    }
    console.log("Listeners ready");
    await invoke("launch_docker", {name: this.workspaceName, config: options});
    console.log("3..2..1..LiftOff!");
  } catch (err) {
    console.log(`Error: ${err}`);
  }
}

export default {
  name: "LaunchDocker",
  data() {
    const options = {
      root_folder: "/tmp/tari",
      tari_network: "weatherwax",
      has_base_node: true,
      has_wallet: false,
      has_sha3_miner: false,
      has_mm_proxy: false,
      has_xmrig: false,
      wait_for_tor: 5,
      wallet_password: null,
      sha3_mining_threads: null,
      monerod_url: null,
      monero_username: null,
      monero_password: null,
      monero_use_auth: null,
      monero_mining_address: null,
      docker_registry: null,
      docker_tag: null,
    };
    const ids = {
      base_node: {
        node_id: "none",
        public_key: "none",
        features: "none",
        secret_key: "secret",
        public_address: "none"
      },
      wallet: {
        node_id: "none",
        public_key: "none",
        features: "none",
        secret_key: "secret",
        public_address: "none"
      },
    };
    return {
      workspaceStatus: "N/A",
      workspaceName: "MyWorkspace",
      logs: ["Logs go here"],
      options,
      ids
    }
  },
  methods: {
    getWorkspaceFolder,
    createWorkspace,
    openWorkspace,
    launch
  }
}
</script>

<style scoped>
  .logs {
    margin-top: 10px;
    padding: 20px;
    max-height: 600px;
    width: 90%;
    overflow: scroll;
    color: black;
    font-family: "Courier New", monospace;
    font-size: 10pt;
    text-align: left;
  }

  div.logs>table {
    border-collapse: collapse;
    border: #5c6773;
  }

  div.logs td {
    border: 1px solid slategrey;
  }

  div.logs tr:nth-child(even) {
    background-color: #9aa4ae;
  }
</style>