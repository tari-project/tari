import { createStore } from 'vuex'
import {invoke} from '@tauri-apps/api/tauri'
import {listen} from "@tauri-apps/api/event";
// import {listen} from '@tauri-apps/api/event'

const settings = {
    walletPassword: "tari",
    moneroMiningAddress: "",
    numMiningThreads: 1,
    tariNetwork: "weatherwax",
    rootFolder: "/tmp/tari",
    dockerRegistry: null,
    dockerTag: null,
    monerodUrl: "http://monero-stagenet.exan.tech:38081",
    moneroUseAuth: false,
    moneroUsername: "",
    moneroPassword: ""
};

export const store = createStore({
    state () {
        return {
            settings,
            containers: {}
        }
    },
    mutations: {
        setNetwork(state, network) {
            state.settings.tariNetwork = network;
        },
        setRootFolder(state, folder) {
            state.settings.rootFolder = folder;
        },
        setWalletPassword(state, value) {
            state.settings.walletPassword = value;
        },
        setMoneroMiningAddress(state, value) {
            state.settings.moneroMiningAddress = value;
        },
        setNumMiningThreads(state, value) {
            state.settings.numMiningThreads = value;
        },
        setDockerRegistry(state, value) {
            state.settings.dockerRegistry = value;
        },
        setDockerTag(state, value) {
            state.settings.dockerTag = value;
        },
        newContainer(state, type, record) {
            if (state.containers[type]) {
                console.log(`Container ${name} already exists. Old container to be replaced: `, state.containers[type]);
            }
            state.containers[type] = record;
        },
        updateContainerStatus(state, update) {
            if (!update.type) {
                console.log(`Container status update did not include type`);
                return;
            }
            if (!update.status) {
                console.log(`Container status update did not include status`);
                return;
            }
            state.containers[update.type].status = update.status;
        },
        updateLog(state, update) {
            if (!update.type) {
                console.log(`Container status update did not include type`);
                return;
            }
            if (!update.log) {
                console.log(`Container status update did not include log message`);
                return;
            }
            state.containers[update.type].logs.push(update.log);
        },
        updateContainerStats(state, update) {
            if (!update.type) {
                console.log(`Container status update did not include type`);
                return;
            }
            if (!update.stats) {
                console.log(`Container status update did not include stats`);
                return;
            }
            state.containers[update.type].stats = update.stats;
        },
    },
    actions: {
        async startContainer({ commit }, type) {
            console.log(`Starting container ${type}`);
            try {
                let response = await invoke("startService", type);
                // Subscribe to stat update events
                let statsUnsubscribe = await listen();
                // Subscribe to log events
                let logsUnsubscribe = await listen();
                response.listeners = [ statsUnsubscribe, logsUnsubscribe ];
                commit("newContainer", response);
            } catch (err) {
                console.log("Error starting service: ", err);
            }
        }
    }
})

export default store