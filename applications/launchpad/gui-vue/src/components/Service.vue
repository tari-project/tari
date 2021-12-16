<template>
  <h1>{{ name }}</h1>
  <div>
    <p><b>Network:</b> {{ $store.state.networkConfig.tari_network }}</p>
    <p><b>Workspace:</b> {{ $store.state.networkConfig.root_folder }}</p>
    <o-button @click="startContainer">Start</o-button>
    <o-button @click="stopContainer">Stop</o-button>
    <p><b>Status:</b> {{ status }}</p>

  </div>

  <div class="stats">
    <h2>Stats <o-icon pack="fas" icon="tachometer-alt"> </o-icon></h2>
    <p><b>CPU:</b> {{ stats.cpu }}%</p>
    <p><b>Memory:</b> {{ stats.mem }} MB</p>
  </div>

  <div class="logs">
    <hr/>
    <h2>Logs</h2>
    <o-table
        :data="logs"
        :columns="columns"
        :striped="true"
        :narrowed="true"
        :hoverable="true"
        :sticky-header="true"
        :debounce-search="100"
    >
    </o-table>
    <hr/>
  </div>
</template>

<script>
// import {invoke} from '@tauri-apps/api/tauri'
// import {listen} from '@tauri-apps/api/event'

function startContainer() {
  console.log(`Starting ${this.name}...`)
}

function stopContainer() {
  console.log(`Stopping ${this.name}...`)
}

export default {
  name: 'service',
  props: {
    name: String,
  },

  data() {
    const logs = [
      {id: 0, timestamp: (new Date()).toISOString(), message: "Logs go here"}
    ];
    const columns = [
      {
        field: 'id',
        label: 'ID',
        width: '100',
        numeric: true,
        searchable: false
      },
      {
        field: 'timestamp',
        label: 'Time',
        searchable: false
      },
      {
        field: 'message',
        label: 'Message',
        searchable: true
      }
    ];
    const stats = {
      cpu: 0,
      mem: 0,
    };
    return {
      status: "None",
      logs,
      columns,
      stats
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

</style>
