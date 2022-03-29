// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

<template>

  <h1>Tari Launchpad</h1>
  <Suspense>
    <template>
      <hr/>
      <containers></containers>
      <hr/>
    </template>
  </Suspense>
  <launch-docker></launch-docker>
</template>

<script>
import containers from './components/Containers.vue';
import launchDocker from "./components/LaunchDocker";

export default {
  name: 'App',
  data() {
    return {};
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
