<template>

  <h1>Tari Launchpad</h1>
  <Suspense>
    <hr/>
    <containers></containers>
    <hr/>
  </Suspense>
  <launch-docker></launch-docker>
</template>

<script>
import containers from './components/Containers.vue';
import launchDocker from "./components/LaunchDocker";


export default {
  name: 'App',
  data() {
  },
  components: {
    launchDocker,
    containers,
  }
}
</script>

<style>
#app {
  font-family: Avenir, Helvetica, Arial, sans-serif;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
  text-align: center;
  color: #2c3e50;
  margin-top: 60px;
}

.section {
  margin: 20px 0;
  padding: 5px 10px;
}
</style>
