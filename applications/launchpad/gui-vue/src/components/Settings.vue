// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

<template>
  <h1>Settings</h1>
  <div>
    <o-field label="Tari network">
      <div>
        <select v-model="$store.state.settings.tariNetwork">
          <option value="dibbler">Dibbler</option>
          <option value="igor">Igor</option>
          <option value="mainnet">Mainnet</option>
        </select>
      </div>
    </o-field>
    <o-field label="Root Folder">
      <o-input v-model="$store.state.settings.rootFolder"></o-input>
    </o-field>
    <o-field label="walletPassword">
      <o-input type="password" v-model="$store.state.settings.walletPassword" password-reveal></o-input>
    </o-field>
    <o-field label="moneroMiningAddress">
      <o-input v-model="$store.state.settings.moneroMiningAddress"></o-input>
    </o-field>
    <o-field label="Number of SHA3 mining threads">
      <o-input v-model.number="$store.state.settings.numMiningThreads"></o-input>
    </o-field>
    <o-field label="Docker Registry">
      <o-input v-model="$store.state.settings.dockerRegistry"></o-input>
    </o-field>
    <o-field label="Docker Tag">
      <o-input v-model="$store.state.settings.dockerTag"></o-input>
    </o-field>
    <o-field label="Monero Node URL">
      <o-input v-model="$store.state.settings.monerodUrl"></o-input>
    </o-field>
    <o-checkbox v-model="$store.state.settings.moneroUseAuth">
      Monero Node uses authentication?
    </o-checkbox>
    <div v-if="$store.state.settings.moneroUseAuth">
      <o-field label="Monero Node username">
        <o-input v-model="$store.state.settings.moneroUsername"></o-input>
      </o-field>
      <o-field label="Monero Node password">
        <o-input type="password" password-reveal v-model="$store.state.settings.moneroPassword"></o-input>
      </o-field>
    </div>

  </div>
  <div>
    <o-button size="large" @click="applySettings">Apply</o-button>
  </div>
</template>

<script>
import {mapMutations} from 'vuex'

function applySettings() {
  // If a workspace is running, warn that it must be shut down
  // Create a new default workspace
}

export default {
  name: 'settings',
  data() {
    return {};
  },
  methods: {
    applySettings,
    ...mapMutations([
      'setNetwork',
      'setRootFolder',
      'setWalletPassword',
      'setMoneroMiningAddress',
      'setNumMiningThreads',
      'setDockerRegistry',
      'setDockerTag'
    ])
  }
}
</script>

<!-- Add "scoped" attribute to limit CSS to this component only -->
<style scoped>

</style>
