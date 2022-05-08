import { createSlice, PayloadAction } from '@reduxjs/toolkit'

import {
  StatsEventPayload,
  ContainerStatus,
  ServicesState,
  ContainerId,
  Service,
  SystemEventAction,
} from './types'
import { start, stop } from './thunks'

const getInitialServiceStatus = (
  lastAction: SystemEventAction,
): ContainerStatus => ({
  lastAction,
  stats: {
    cpu: 0,
    memory: 0,
    unsubscribe: () => null,
  },
})

export const initialState: ServicesState = {
  containers: {},
  services: {
    [Service.Tor]: { pending: false, containerId: '' },
    [Service.BaseNode]: { pending: false, containerId: '' },
    [Service.Wallet]: { pending: false, containerId: '' },
    [Service.SHA3Miner]: { pending: false, containerId: '' },
    [Service.MMProxy]: { pending: false, containerId: '' },
    [Service.XMrig]: { pending: false, containerId: '' },
    [Service.Monerod]: { pending: false, containerId: '' },
    [Service.Frontail]: { pending: false, containerId: '' },
  },
}

const servicesSlice = createSlice({
  name: 'services',
  initialState,
  reducers: {
    stats: (
      state,
      action: PayloadAction<{
        containerId: ContainerId
        stats: StatsEventPayload
      }>,
    ) => {
      const { stats, containerId } = action.payload

      const cs = stats.cpu_stats
      const pcs = stats.precpu_stats
      const cpu_delta = cs.cpu_usage.total_usage - pcs.cpu_usage.total_usage
      const system_cpu_delta = cs.system_cpu_usage - pcs.system_cpu_usage
      const numCpu = cs.online_cpus
      const cpu = (cpu_delta / system_cpu_delta) * numCpu * 100.0

      state.containers[containerId].stats.cpu = cpu

      const ms = stats.memory_stats
      const memory = (ms.usage - (ms.stats.cache || 0)) / (1024 * 1024)

      state.containers[containerId].stats.memory = memory
    },
    updateStatus: (
      state,
      action: PayloadAction<{ containerId: string; action: SystemEventAction }>,
    ) => {
      if (!state.containers[action.payload.containerId]) {
        state.containers[action.payload.containerId] = getInitialServiceStatus(
          action.payload.action,
        )
      } else {
        state.containers[action.payload.containerId].lastAction =
          action.payload.action
      }

      const [service] =
        Object.entries(state.services).find(
          ([, status]) => status.containerId === action.payload.containerId,
        ) || []

      switch (action.payload.action) {
        case SystemEventAction.Destroy:
          state.containers[action.payload.containerId].stats.memory = 0
          state.containers[action.payload.containerId].stats.cpu = 0
          state.containers[action.payload.containerId].stats.unsubscribe() // case for container destroyed outside of our application
          state.containers[action.payload.containerId].stats.unsubscribe = () =>
            undefined
          if (service) {
            state.services[service as Service].containerId = ''
          }
          break
      }
    },
  },
  extraReducers: builder => {
    builder.addCase(start.pending, (state, { meta }) => {
      state.services[meta.arg].pending = true
    })
    builder.addCase(start.fulfilled, (state, action) => {
      if (!state.containers[action.payload.id]) {
        // TODO throw an error - we *must* already have some system event for this container, right?

        return
      }

      state.containers[action.payload.id].stats.unsubscribe =
        action.payload.unsubscribeStats
      state.services[action.meta.arg].pending = false
      state.services[action.meta.arg].containerId = action.payload.id
    })
    builder.addCase(start.rejected, (state, action) => {
      console.log(`ERROR STARTING SERVICE ${action.meta.arg}`)
      console.log(action.error)
    })

    builder.addCase(stop.pending, (state, { meta }) => {
      state.services[meta.arg].pending = true
    })
    builder.addCase(stop.fulfilled, (state, { meta }) => {
      state.services[meta.arg].pending = false
      state.services[meta.arg].containerId = ''
    })
    builder.addCase(stop.rejected, (state, action) => {
      console.log(`ERROR STOPPING SERVICE ${action.meta.arg}`)
      console.log(action.error)
    })
  },
})

const { actions: syncActions } = servicesSlice
export const actions = { start, stop, ...syncActions }

export default servicesSlice.reducer
