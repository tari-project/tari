
<o-tab-item value="1" label="Setup">
<div>
  <h1>Workspace</h1>
  <o-input v-model="workspaceName" placeholder="MyWorkspace"></o-input>
  <o-button @click="createWorkspace">Create workspace</o-button>
  <o-button @click="openWorkspace">Open workspace</o-button>
  <p>
    <b>Workspace status:</b><span>{{ workspaceStatus }}</span>
  </p>
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
    <div class="field">
      <o-checkbox id="base_node" v-model="options.has_base_node">
        Spin up a Base Node
      </o-checkbox>
    </div>
    <div class="field">
      <o-checkbox v-model="options.has_wallet">
        Spin up a wallet
      </o-checkbox>
    </div>
    <div class="field">
      <div v-if="options.has_wallet">
        <input v-model="options.wallet_password" placeholder="password"/>
      </div>
    </div>
    <div class="field">
      <o-checkbox v-model="options.has_sha3_miner">
        Spin up a SHA3 miner
      </o-checkbox>
      <div v-if="options.has_sha3_miner">
        <input v-model.number="options.sha3_mining_threads" placeholder="# SHA3 mining threads"/>
      </div>
    </div>
    <div class="field">
      <o-checkbox v-model="options.has_mm_proxy">
        Spin up a Monero Miner
      </o-checkbox>
    </div>
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
<o-button size="large" icon-left="check" @click="launch">Launch!</o-button>

</o-tab-item>