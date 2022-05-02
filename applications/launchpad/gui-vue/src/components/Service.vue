
<template>
  <h1>{{ displayName }}</h1>
  <div>
    <p><b>Network:</b> {{ $store.state.settings.tariNetwork }}</p>
    <p><b>Workspace:</b> {{ $store.state.settings.rootFolder }}</p>
    <p v-if="serviceName === 'sha3_miner'">
      <b>Threads:</b> {{ $store.state.settings.numMiningThreads }}
    </p>
    <o-button @click="startContainer">Start</o-button>
    <o-button @click="stopContainer">Stop</o-button>
    <p><b>Status:</b> {{ status }}</p>

  </div>

  <div class="stats">
    <h2>Stats
      <o-icon pack="fas" icon="tachometer-alt"></o-icon>
    </h2>
    <p><b>CPU:</b> {{  cpu.toFixed(1) }} %</p>
    <p><b>Memory:</b> {{  mem.toFixed(1) }} MB</p>
  </div>

  <div class="logs">
    <hr/>
    <h2>Logs</h2>
    <o-button @click="$store.commit('updateLog', { type: serviceName, log: {message: 'bar'}})">
      Add logs
    </o-button>
    <table>
      <tr v-for="index in logs.length" v-bind:key="index">
        <td> {{ index }} </td><td> {{ logs.get(index) }} </td>
      </tr>
    </table>
    <hr/>
  </div>
</template>

<script>

import store from '../store';

async function startContainer() {
  console.log(`Starting ${this.displayName} (${this.serviceName})...`);
  await this.$store.dispatch('startContainer', this.serviceName);
}

async function stopContainer() {
  await this.$store.dispatch('stopContainer', this.serviceName);
}

export default {
  name: 'service',
  props: {
    displayName: String,
    serviceName: String,
  },
  setup(props) {
    store.commit('newContainer', props.serviceName);
  },

  computed: {
    logs() {
      return this.$store.state.containers[this.serviceName].logs;
    },
    status() {
      return this.$store.state.containers[this.serviceName].status;
    },
    cpu() {
      return this.$store.state.containers[this.serviceName].stats.cpu;
    },
    mem() {
      return this.$store.state.containers[this.serviceName].stats.mem;
    }
  },

  data() {
    return {
    }
  },
  methods: {
    startContainer,
    stopContainer,
  }
}
</script>

<!-- Add "scoped" attribute to limit CSS to this component only -->
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
