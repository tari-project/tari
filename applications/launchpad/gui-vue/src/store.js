import { createStore } from 'vuex'

export const store = createStore({
    state () {
        return {
            networkConfig: {
                tari_network: "weatherwax",
                root_folder: "/tmp/tari",
                docker_registry: null,
                docker_tag: null,
            }
        }
    },
    mutations: {
        setNetwork(state, network) {
            state.networkConfig.tari_network = network;
        },
        setRootFolder(state, folder) {
            state.networkConfig.root_folder = folder;
        }
    }
})

export default store