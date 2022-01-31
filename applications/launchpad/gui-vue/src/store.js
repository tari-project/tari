import {createStore} from 'vuex'
import {invoke} from '@tauri-apps/api/tauri'
import {listen} from "@tauri-apps/api/event";
import CBuffer from 'CBuffer';

const settings = {
    walletPassword: "tari",
    moneroMiningAddress: "5AJ8FwQge4UjT9Gbj4zn7yYcnpVQzzkqr636pKto59jQcu85CFsuYVeFgbhUdRpiPjUCkA4sQtWApUzCyTMmSigFG2hDo48",
    numMiningThreads: 1,
    tariNetwork: "dibbler",
    rootFolder: "/tmp/dibbler",
    dockerRegistry: "quay.io/tarilabs",
    dockerTag: "latest",
    monerodUrl: "http://monero-stagenet.exan.tech:38081",
    moneroUseAuth: false,
    moneroUsername: "",
    moneroPassword: ""
};

function handleSystemEvent(commit, payload) {
    if (payload.Type === "container") {
        return commit('updateContainerStatus',  {status: payload.Action, id: payload.Actor.ID});
    }
}

export const store = createStore({
    state() {
        return {
            settings,
            subscribedToEvents: false,
            unsubscribeSystemEvents: () => {},
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
        subscribedToEvents(state) {
            state.subscribedToEvents = true;
        },
        unsubscribeSystemEvents(state, fn) {
            state.unsubscribeSystemEvents = fn;
        },
        newContainer(state, type) {
            if (state.containers[type]) {
                console.log(`Container ${type} already exists. Old container to be replaced: `, state.containers[type]);
            }
            state.containers[type] = {
                logs: new CBuffer(1000),
                stats: {cpu: 0, mem: 0}
            };
            console.log(`Added new container ${type}`);
        },

        startContainer(state, {type, record}) {
            if (!state.containers[type]) {
                console.log(`Call newContainer before startContainer for ${type}`);
            }
            state.containers[type].id = record.id;
            state.containers[type].listeners = record.listeners;
            state.containers[type].logEventsName = record.logEventsName;
            state.containers[type].statsEventsName = record.statsEventsName;
            state.containers[type].name = record.name;
            if (Array.isArray(record.logs)) {
                for (let message of record.logs) {
                    state.containers[type].logs.push(message);
                }
            }
        },
        updateContainerStatus(state, update) {
            if (!update.id) {
                console.log(`Container status update did not include id`);
                return;
            }
            if (!update.status) {
                console.log(`Container status update did not include status`);
                return;
            }
            for (let c in state.containers) {
                let container = state.containers[c];
                if (container.id === update.id) {
                    container.status = update.status;
                }
            }
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
            console.log("Event received:", update);
            if (!update.type) {
                console.log(`Container status update did not include type`);
                return;
            }
            if (!update.stats) {
                console.log(`Container status update did not include stats`);
                return;
            }
            let cpu = 0;
            try {
                let cs = update.stats.cpu_stats;
                let pcs = update.stats.precpu_stats;
                const cpu_delta = cs.cpu_usage.total_usage - pcs.cpu_usage.total_usage;
                const system_cpu_delta = cs.system_cpu_usage - pcs.system_cpu_usage;
                const numCpu = cs.online_cpus;
                cpu = (cpu_delta / system_cpu_delta) * numCpu * 100.0
            } catch {
                console.log("Invalid CPU data");
            }
            let mem = 0;
            try {
                let ms = update.stats.memory_stats;
                mem = (ms.usage - (ms.stats.cache || 0))/(1024*1024);
            } catch {
                console.log("Invalid Memory data");
            }
            console.log(`${update.type} CPU ${cpu}, Memory: ${mem}`);
            state.containers[update.type].stats.cpu = cpu;
            state.containers[update.type].stats.mem = mem;
        },
    },
    actions: {
        async startContainer({state, commit}, type) {
            console.log(`Starting container ${type}`);
            try {
                if (!state.subscribedToEvents) {
                    console.log("Subscribing to events");
                    commit('subscribedToEvents');
                    await invoke("events");
                    let eventsUnsubscribe = await listen("tari://docker-system-event", (event) => {
                        console.log("System event: ", event.payload);
                        handleSystemEvent(commit, event.payload);
                    });
                    commit('unsubscribeSystemEvents', eventsUnsubscribe);
                }

                const settings = Object.assign({}, this.state.settings);
                console.log(settings);
                let record = await invoke("start_service", {serviceName: type, settings});
                console.log(`Got ${JSON.stringify(record)} as response`)
                // Subscribe to stat update events
                let logsUnsubscribe = await listen(record.logEventsName, (event) => {
                    console.log("Log event: ", event.payload);
                    commit('updateLog', {type, log: event.payload});
                });
                // Subscribe to log events
                console.log(`Listening on ${record.statsEventsName}`);
                let statsUnsubscribe = await listen(record.statsEventsName, (event) => {
                    console.log("Stats event: ", event.payload);
                    commit('updateContainerStats', {type, stats: event.payload});
                });
                record.listeners = [statsUnsubscribe, logsUnsubscribe];
                commit('startContainer', {type, record});
            } catch (err) {
                console.log("Error starting service: ", err);
            }
        },
        async stopContainer({state}, type) {
            console.log(`Stopping container ${type}`);
            try {
                await invoke("stop_service", {serviceName: type, settings});
                let service = state.containers[type];
                if (service.listeners) {
                    if (typeof (service.listeners.statsUnsubscribe) === 'function') {
                        console.log("Detaching stats listener");
                        service.listeners.statsUnsubscribe();
                    }
                    if (typeof (service.listeners.logsUnsubscribe) === 'function') {
                        console.log("Detaching Log listener");
                        service.listeners.logsUnsubscribe();
                    }
                }
            } catch (err) {
                console.log("Error starting service: ", err);
            }
        }
    }
})

export default store