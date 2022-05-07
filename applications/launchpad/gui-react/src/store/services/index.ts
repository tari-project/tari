import { createSlice, PayloadAction } from '@reduxjs/toolkit'

import {
  StatsEventPayload,
  ServiceStatus,
  ServicesState,
  Service,
} from './types'
import { start, stop } from './thunks'

const getInitialServiceStatus = (): ServiceStatus => ({
  id: '',
  pending: false,
  running: false,
  stats: {
    cpu: 0,
    memory: 0,
    unsubscribe: () => null,
  },
})

export const initialState: ServicesState = {
  services: {},
  servicesStatus: {
    [Service.Tor]: getInitialServiceStatus(),
    [Service.BaseNode]: getInitialServiceStatus(),
    [Service.Wallet]: getInitialServiceStatus(),
    [Service.SHA3Miner]: getInitialServiceStatus(),
    [Service.MMProxy]: getInitialServiceStatus(),
    [Service.XMrig]: getInitialServiceStatus(),
    [Service.Monerod]: getInitialServiceStatus(),
    [Service.Frontail]: getInitialServiceStatus(),
  },
}

const servicesSlice = createSlice({
  name: 'services',
  initialState,
  reducers: {
    stats: (
      state,
      action: PayloadAction<{ service: Service; stats: StatsEventPayload }>,
    ) => {
      const { stats, service } = action.payload

      const cs = stats.cpu_stats
      const pcs = stats.precpu_stats
      const cpu_delta = cs.cpu_usage.total_usage - pcs.cpu_usage.total_usage
      const system_cpu_delta = cs.system_cpu_usage - pcs.system_cpu_usage
      const numCpu = cs.online_cpus
      const cpu = (cpu_delta / system_cpu_delta) * numCpu * 100.0

      state.servicesStatus[service].stats.cpu = cpu

      const ms = stats.memory_stats
      const memory = (ms.usage - (ms.stats.cache || 0)) / (1024 * 1024)

      state.servicesStatus[service].stats.memory = memory
    },
  },
  extraReducers: builder => {
    builder.addCase(start.pending, (state, { meta }) => {
      state.servicesStatus[meta.arg].pending = true
      state.servicesStatus[meta.arg].error = undefined
    })
    builder.addCase(start.fulfilled, (state, action) => {
      state.servicesStatus[action.meta.arg].pending = false
      state.servicesStatus[action.meta.arg].running = true
      state.servicesStatus[action.meta.arg].id = action.payload.id
      state.servicesStatus[action.meta.arg].stats.unsubscribe =
        action.payload.unsubscribeStats
    })
    builder.addCase(start.rejected, (state, action) => {
      state.servicesStatus[action.meta.arg].pending = false
      state.servicesStatus[action.meta.arg].running = false
      state.servicesStatus[action.meta.arg].error = action.error.message
      console.log(`ERROR STARTING SERVICE ${action.meta.arg}`)
      console.log(action.error)
    })

    builder.addCase(stop.pending, (state, { meta }) => {
      state.servicesStatus[meta.arg].pending = true
      state.servicesStatus[meta.arg].error = undefined
    })
    builder.addCase(stop.fulfilled, (state, action) => {
      state.servicesStatus[action.meta.arg].pending = false
      state.servicesStatus[action.meta.arg].running = false
      state.servicesStatus[action.meta.arg].stats.cpu = 0
      state.servicesStatus[action.meta.arg].stats.memory = 0
    })
    builder.addCase(stop.rejected, (state, action) => {
      state.servicesStatus[action.meta.arg].pending = false
      state.servicesStatus[action.meta.arg].running = false
      state.servicesStatus[action.meta.arg].error = action.error.message
      console.log(`ERROR STOPPING SERVICE ${action.meta.arg}`)
      console.log(action.error)
    })
  },
})

export const actions = { start, stop }

export default servicesSlice.reducer
