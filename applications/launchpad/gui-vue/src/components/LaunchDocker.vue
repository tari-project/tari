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
  <o-tabs v-model="activeTab">
    <o-tab-item value="0" label="Images">
      <suspense>
        <Containers></Containers>
      </suspense>
    </o-tab-item>

    <o-tab-item value="2" label="Tor">
      <service display-name="Tor" service-name="tor"></service>
    </o-tab-item>

    <o-tab-item value="3" label="Base Node">
      <service display-name="Base Node" service-name="base_node"></service>
    </o-tab-item>

    <o-tab-item value="4" label="Wallet">
      <service display-name="Wallet" service-name="wallet"></service>
    </o-tab-item>

    <o-tab-item value="5" label="SHA3 Miner">
      <service display-name="SHA3 miner" service-name="sha3_miner"></service>
    </o-tab-item>

    <o-tab-item value="6" label="Merged miner proxy">
      <service display-name="Merge mining proxy" service-name="mm_proxy"></service>
    </o-tab-item>

    <o-tab-item value="7" label="XMRig">
      <service display-name="Monero merge mining" service-name="xmrig"></service>
    </o-tab-item>

    <o-tab-item value="8" label="Settings">
      <settings></settings>
    </o-tab-item>
  </o-tabs>


</template>

<script>
import {invoke} from '@tauri-apps/api/tauri'
import {save, open} from '@tauri-apps/api/dialog'
import {listen} from "@tauri-apps/api/event";
import {defineAsyncComponent} from "vue";
import service from "@/components/Service";
import settings from "@/components/Settings";

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
  components: {
    Containers: defineAsyncComponent(() => import("@/components/Containers")),
    service,
    settings
  },
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
      ids,
      activeTab: '0'
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

div.logs > table {
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
