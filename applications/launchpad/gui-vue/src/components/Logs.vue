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
  <div v-if="ready">
    <iframe src="http://localhost:18130" width="100%" height="1000"></iframe>
  </div>
  <div v-else>
    <o-button @click="startServer">
      Start Log server
    </o-button>
  </div>
</template>

<script>

async function checkServer(self) {
  try {
    let id = setInterval(async () => {
      try {
        await fetch("http://localhost:18130", {method: "HEAD", mode: "no-cors", cache: "no-cache"});
        this.ready = true;
        clearInterval(id);
      } catch (err) {
        if (!(err instanceof TypeError)) {
          return;
        }
        if (err.message === "Failed to fetch" || err.message === "Load failed") {
          console.log("Frontails not ready. Waiting some more..");
        } else {
          console.log("Frontails detected.", err.message);
          self.ready = true;
          clearInterval(id);
        }
      }
    }, 5000);
  } catch (err) {
    console.log(err);
  }
}

async function startServer() {
  console.log(`Starting Frontail...`);
  await this.$store.dispatch('startContainer', 'frontail');
}

export default {
  name: 'logs',

  async mounted() {
    await checkServer(this)
  },

  data() {
    return {
      ready: false
    }
  },
  methods: {startServer}
}
</script>

<!-- Add "scoped" attribute to limit CSS to this component only -->
<style scoped>
</style>
