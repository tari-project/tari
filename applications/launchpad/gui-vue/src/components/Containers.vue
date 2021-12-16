<template>
  <div class="containers">
    <h1>Containers</h1>
    <o-button @click="pullImages">Pull images</o-button>
    <ul>
      <li v-for="(image, i) of imageList" :key="i"><b>{{ image }}</b>:{{ ' ' }}{{ info[image].status }} /
        {{ info[image].progress }}%
      </li>
    </ul>
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
      console.log(event);
      const name = event.payload.name;
      const progInfo = event.payload.info;
      this.info[name].status = progInfo.status || "-";
      this.info[name].progress = progInfo.progress || 0;
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
  async setup() {
    console.log("Getting image list");
    let imageList = [];
    try {
      imageList.value = await invoke("image_list");
    } catch (err) {
      console.log(err);
      console.log("Using default image list");
    }
    return {imageList}
  },

  data() {
    const info = {};
    const errors = "None";
    console.log("ImageList:", this.imageList);
    this.imageList.forEach(p => info[p] = {status: "Unknown", progress: 0});
    return {info, errors}
  },
  methods: {pullImages}
}
</script>

<!-- Add "scoped" attribute to limit CSS to this component only -->
<style scoped>
ul {
  font-size: 10pt;
  list-style-type: square;
  padding: 5px;
}

li {
  text-align: left;
  margin: 0 10px;
}

p.error {
  color: red;
}
</style>
