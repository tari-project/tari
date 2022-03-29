// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

<template>
  <div class="containers">
    <h1>Containers</h1>
    <o-button @click="pullImages">Pull images</o-button>
    <table>
      <tr v-for="(image,i) in imageList" :key="i">
      <td><b>{{ image.displayName }}</b></td>
      <td>{{ image.status }}</td>
      <td class="progress">{{ image.progress }}</td>
      </tr>
    </table>
    <h2>Errors</h2>
    <p class="error">{{ errors }}</p>
  </div>
</template>

<script>
import {invoke} from '@tauri-apps/api/tauri'
import {listen} from '@tauri-apps/api/event'

async function pullImages() {
  console.log("Pulling images");
  try {
    const unlisten = await listen('image-pull-progress', event => {
      const name = event.payload.image.split(':')[0];
      const progInfo = event.payload.info;
      this.imageList[name].status = progInfo.status || "-";
      this.imageList[name].progress = progInfo.progress || "";
    });
    await invoke('pull_images');
    await unlisten();
  } catch (err) {
    console.log("Could not pull images");
    console.log(err);
    this.errors = err;
  }
  console.log("Image pull complete");
}

export default {
  name: 'containers',

  data() {
    const errors = "None";
    const imageList = {
      'quay.io/tarilabs/tor': {displayName: 'tor', status: "Unknown", progress: ""},
      'quay.io/tarilabs/tari_base_node' : {displayName: 'base node', status: "Unknown", progress: ""},
      'quay.io/tarilabs/tari_console_wallet': {displayName: 'wallet', status: "Unknown", progress: ""},
      'quay.io/tarilabs/tari_sha3_miner': {displayName: 'SHA3 miner', status: "Unknown", progress: ""},
      'quay.io/tarilabs/tari_mm_proxy': {displayName: 'Merge miner proxy', status: "Unknown", progress: ""},
      'quay.io/tarilabs/xmrig': {displayName: 'xmrig', status: "Unknown", progress: ""},
      'quay.io/tarilabs/monerod': {displayName: 'monerod', status: "Unknown", progress: ""},
      'quay.io/tarilabs/frontail': {displayName: 'frontail', status: "Unknown", progress: ""},
    }
    return {imageList, errors}
  },
  methods: {pullImages}
}
</script>

<!-- Add "scoped" attribute to limit CSS to this component only -->
<style scoped>

table {
  text-align: left;
  border: 1px;
}
td {
  padding: 5px;
}
td.progress {
  font-family: "Courier New", monospace;
  font-size: 8pt;
}

p.error {
  color: red;
}
</style>
